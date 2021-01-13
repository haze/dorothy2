// 1. dont do token estimation, the bot will break under better workloads
// 2. dont respond to messages 1:1, itll branch the conversation sometimes and requires an extra api call to merge back together
// 3. save contexts
// 4. add more fine grained tuning permissions
mod commands;
mod engines;
mod error;
mod transformers;

use engines::MessageSessionHandler;
pub use engines::*;
use serenity::{
    framework::{
        standard::{Args, CommandResult},
        StandardFramework,
    },
    http::Http,
    model::{
        channel::Message,
        gateway::Ready,
        id::{ChannelId, GuildId},
    },
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::{sync::mpsc, time};

const COMMAND_IDENTIFIER: &str = "!";

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct ChatTarget {
    guild_id: GuildId,
    channel_id: ChannelId,
}

#[derive(Default)]
struct Handler {
    session_map: ThreadsafeSessionMap,
    /// Used for prolonging the delay for tasks that need to generate responses when multiple
    /// people are talking, instead of responding 1:1
    chat_timeout_map: RwLock<HashMap<ChatTarget, ChatTargetTimeoutCommunicator>>,
    gpt3_token: String,
}

struct ChatTargetTimeoutCommunicator {
    new_message_sender: mpsc::UnboundedSender<()>,
    finished: Arc<AtomicBool>,
}

pub enum Session {
    GPT2(gpt2::GPT2MessageHandler),
    GPT3(gpt3::GPT3MessageHandler),
}

impl Session {
    async fn reset(&mut self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        match self {
            Session::GPT2(session) => session.reset(ctx, msg, args).await,
            Session::GPT3(session) => session.reset(ctx, msg, args).await,
        }
    }

    async fn disable(&self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        Ok(())
    }
    async fn enable(&self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        Ok(())
    }

    async fn info(&self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        match self {
            Session::GPT2(session) => session.info(ctx, msg, args).await,
            Session::GPT3(session) => session.info(ctx, msg, args).await,
        }
    }
}

impl std::fmt::Display for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Session::GPT2(session) => write!(f, "{}", session),
            Session::GPT3(session) => write!(f, "{}", session),
        }
    }
}

type ThreadsafeSessionMap = Arc<RwLock<HashMap<ChatTarget, Session>>>;

fn get_chat_target_from_message(message: &Message) -> Option<ChatTarget> {
    message.guild_id.map(|guild_id| ChatTarget {
        guild_id,
        channel_id: message.channel_id,
    })
}

impl Handler {
    fn new(gpt3_token: String) -> (Handler, ThreadsafeSessionMap) {
        let session_map = Arc::new(RwLock::new(HashMap::new()));
        (
            Handler {
                session_map: Arc::clone(&session_map),
                chat_timeout_map: RwLock::new(HashMap::new()),
                gpt3_token,
            },
            session_map,
        )
    }

    async fn should_respond_to_target(&self, chat_target: &ChatTarget) -> bool {
        self.session_map.read().await.contains_key(chat_target)
    }
}

async fn timeout_task(mut payload: TimeoutTaskPayload) {
    let wait_dur = time::Duration::from_millis(2_500);
    let mut delay = time::delay_for(wait_dur);
    loop {
        eprintln!("selecting");
        tokio::select! {
            _ = &mut delay => break,
            _ = payload.new_message_receiver.recv() => {
                delay.reset(time::Instant::now() + time::Duration::from_millis(1_500));
            }
        }
    }
    eprintln!("do work now!");
    payload.finished_flag.store(true, Ordering::SeqCst);
    // 0. start typing
    // 1. turn session into string, template out to prompt model
    // 2. request completion
    // 3. add ai generated line
    // 4. send ai generated line as response
    // ???
    // profit
    let http = payload.http;
    if let Err(why) = payload.channel_id.broadcast_typing(&http).await {
        eprintln!("Failed to broadcast typing: {:?}", &why);
    }

    let mut session_map_write = payload.session_map.write().await;
    let mut session = if let Some(mut session) = session_map_write.get_mut(&payload.chat_target) {
        session
    } else {
        eprintln!("Failed to find session in map after timeout");
        return;
    };
    match session {
        Session::GPT2(session) => {}
        Session::GPT3(session) => {
            let gpt3_payload = gpt3::Payload {
                token: payload.gpt3_token.clone(),
                channel_id: payload.channel_id,
            };
            session.perform_work(&http, gpt3_payload).await;
        }
    }
}

struct TimeoutTaskPayload {
    new_message_receiver: mpsc::UnboundedReceiver<()>,
    channel_id: ChannelId,
    finished_flag: Arc<AtomicBool>,
    gpt3_token: String,
    http: Arc<Http>,
    session_map: ThreadsafeSessionMap,
    chat_target: ChatTarget,
}

// async_trait is pretty gnarly with lifetimes :(
#[serenity::async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, message: Message) {
        if message.author.bot || message.content.starts_with(COMMAND_IDENTIFIER) {
            return;
        }
        let chat_target = match get_chat_target_from_message(&message) {
            Some(target) => target,
            None => return,
        };

        if !self.should_respond_to_target(&chat_target).await {
            return;
        }

        if !message.content.starts_with(">") {
            return;
        }

        let mut session_map_write = self.session_map.write().await;
        if let Some(ref mut session) = session_map_write.get_mut(&chat_target) {
            let author_nick = message.author_nick(&ctx).await;
            // TODO(haze): bad haze, bad code
            let text = message
                .content_safe(&ctx)
                .await
                .trim_start_matches(">")
                .to_string();
            match session {
                Session::GPT2(session) => {}
                Session::GPT3(session) => {
                    if let Err(why) = session
                        .record(
                            transformers::conversation::LogItem {
                                author_name: Some(message.author.name.clone()),
                                sent_by_ai: false,
                                author_nick,
                                text,
                            },
                            &*self.gpt3_token,
                        )
                        .await
                    {
                        eprintln!("Failed to record line: {:?}, {:?}", message.content, why);
                    } else {
                        eprintln!("token count: {}", session.token_count);
                    }
                }
            }
        }
        drop(session_map_write);

        let timeout_map_read = self.chat_timeout_map.read().await;
        if let Some(Some(sender)) = timeout_map_read.get(&chat_target).map(|sender| {
            if sender.finished.load(Ordering::SeqCst) {
                None
            } else {
                Some(sender)
            }
        }) {
            if let Err(why) = sender.new_message_sender.send(()) {
                eprintln!("Failed to send prolonging message: {:?}", &why);
            }
        } else {
            drop(timeout_map_read);
            let mut timeout_map_write = self.chat_timeout_map.write().await;
            // first message, and we aren't waiting on a timeout
            // bounded by discord on the network side
            let (tx, rx) = mpsc::unbounded_channel();
            // TODO(haze): collect task handle
            let finished_flag = Arc::new(AtomicBool::default());
            timeout_map_write.insert(
                chat_target.clone(),
                ChatTargetTimeoutCommunicator {
                    new_message_sender: tx,
                    finished: Arc::clone(&finished_flag),
                },
            );
            tokio::spawn(timeout_task(TimeoutTaskPayload {
                session_map: Arc::clone(&self.session_map),
                chat_target: chat_target.clone(),
                channel_id: message.channel_id,
                gpt3_token: self.gpt3_token.clone(),
                http: Arc::clone(&ctx.http),
                new_message_receiver: rx,
                finished_flag,
            }));
            let session_map_read = self.session_map.read().await;
            if let Some(ref session) = session_map_read.get(&chat_target) {
                eprintln!("enabled {}", session);
            }
        }
    }

    async fn ready(&self, _ctx: Context, _data_about_bot: Ready) {
        eprintln!("Connected");
    }
}

pub struct SessionMapKey;
impl TypeMapKey for SessionMapKey {
    type Value = ThreadsafeSessionMap;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenv::dotenv().ok();
    // 1. get discord and gpt3 keys from environment
    let discord_token =
        std::env::var("DISCORD_TOKEN").expect("Could not find discord token in environment");
    let gpt3_token = std::env::var("GPT3_TOKEN").expect("Could not find gpt3 token in environment");

    let http = Http::new_with_token(&discord_token);

    // get owner discord id
    let (owners, bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            if let Some(team) = info.team {
                owners.insert(team.owner_user_id);
            } else {
                owners.insert(info.owner.id);
            }

            (owners, info.id)
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    let framework = StandardFramework::new()
        .configure(|c| {
            c.with_whitespace(true)
                .delimiters(vec![" ", "\n"])
                .on_mention(Some(bot_id))
                .prefix(COMMAND_IDENTIFIER)
                .owners(owners)
        })
        .after(commands::after)
        .group(&commands::ADMIN_GROUP);

    // start serenity bot
    let (handler, session_map) = Handler::new(gpt3_token);
    let mut client = Client::new(&discord_token)
        .event_handler(handler)
        .framework(framework)
        .await?;

    {
        let mut data = client.data.write().await;
        data.insert::<SessionMapKey>(session_map);
    }

    client.start().await?;

    Ok(())
}
