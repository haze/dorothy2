use crate::MessageSessionHandler;
use serenity::{
    framework::standard::{
        macros::{command, group, hook},
        ArgError, Args, CommandResult,
    },
    model::channel::Message,
    prelude::Context,
};
use std::sync::Arc;

#[derive(Debug)]
pub struct StringError(String);

impl std::fmt::Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl std::error::Error for StringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl From<&str> for StringError {
    fn from(data: &str) -> Self {
        StringError(data.to_string())
    }
}

impl<T> From<ArgError<T>> for StringError
where
    T: std::fmt::Display,
{
    fn from(arg_error: ArgError<T>) -> Self {
        StringError(match arg_error {
            ArgError::Eos => String::from("Not enough arguments provided"),
            ArgError::Parse(parse_error) => format!("Failed parsing an argument: {}", parse_error),
            _ => String::from("Unknown argument parsing error"),
        })
    }
}

impl From<serenity::Error> for StringError {
    fn from(error: serenity::Error) -> Self {
        StringError(format!("{}", error))
    }
}

#[group]
pub struct ConversationTuning;

#[group]
#[only_in(guilds)]
#[commands(enable, disable, reset, info)]
pub struct Admin;

#[command]
#[owners_only]
/// enable will create a session for the target for the message, if it exists
async fn enable(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let chat_target = crate::get_chat_target_from_message(msg)
        .ok_or_else(|| StringError::from("Could not create chat target for message"))?;
    let data_read = ctx.data.read().await;
    let session_map = data_read
        .get::<crate::SessionMapKey>()
        .ok_or_else(|| StringError::from("Could not get read copy of session map"))?;
    let session_map = Arc::clone(&session_map);
    drop(data_read);
    let session_map_read = session_map.read().await;
    if session_map_read.contains_key(&chat_target) {
        Err(StringError::from("Chat target already has a session").into())
    } else {
        drop(session_map_read);
        let mut session_map_write = session_map.write().await;
        let session_name: String = args.single()?;
        let session = match &*session_name.to_lowercase() {
            // "gpt2" => {}
            "gpt3" => crate::gpt3::GPT3MessageHandler::enable(ctx, msg, args).await?,
            _ => {
                return Err(StringError(format!(
                    "No complection engine found for {}",
                    session_name,
                ))
                .into());
            }
        };
        session_map_write.insert(chat_target, session);
        Ok(())
    }
}

#[command]
#[owners_only]
/// reset clears the mssage log
async fn reset(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let chat_target = crate::get_chat_target_from_message(msg)
        .ok_or_else(|| StringError::from("Could not create chat target for message"))?;
    let data_read = ctx.data.read().await;
    let session_map = data_read
        .get::<crate::SessionMapKey>()
        .ok_or_else(|| StringError::from("Could not get read copy of session map"))?;
    let session_map = Arc::clone(&session_map);
    drop(data_read);
    let session_map_read = session_map.read().await;
    if session_map_read.contains_key(&chat_target) {
        drop(session_map_read);
        let mut session_map_write = session_map.write().await;
        if let Some(session) = session_map_write.get_mut(&chat_target) {
            session.reset(ctx, msg, args).await
        } else {
            Err(StringError::from("Chat target does not has a session").into())
        }
    } else {
        Err(StringError::from("Chat target does not has a session").into())
    }
}

#[command]
#[owners_only]
/// info resets the context
async fn info(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let chat_target = crate::get_chat_target_from_message(msg)
        .ok_or_else(|| StringError::from("Could not create chat target for message"))?;
    let data_read = ctx.data.read().await;
    let session_map = data_read
        .get::<crate::SessionMapKey>()
        .ok_or_else(|| StringError::from("Could not get read copy of session map"))?;
    let session_map = Arc::clone(&session_map);
    drop(data_read);
    let session_map_read = session_map.read().await;
    if let Some(session) = session_map_read.get(&chat_target) {
        session.info(ctx, msg, args).await
    } else {
        Err(StringError::from("Chat target does not has a session").into())
    }
}
#[command]
#[owners_only]
/// disable will remove a session from the chat map, if it exists
async fn disable(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let chat_target = crate::get_chat_target_from_message(msg)
        .ok_or_else(|| StringError::from("Could not create chat target for message"))?;
    let data_read = ctx.data.read().await;
    let session_map = data_read
        .get::<crate::SessionMapKey>()
        .ok_or_else(|| StringError::from("Could not get read copy of session map"))?;
    let session_map = Arc::clone(&session_map);
    drop(data_read);
    let session_map_read = session_map.read().await;
    if session_map_read.contains_key(&chat_target) {
        drop(session_map_read);
        let mut session_map_write = session_map.write().await;
        session_map_write.remove(&chat_target);
        msg.react(&ctx, '✅').await?;
        Ok(())
    } else {
        Err(StringError::from("Chat target does not has a session").into())
    }
}

#[hook]
pub async fn after(
    ctx: &Context,
    msg: &Message,
    command_name: &str,
    command_result: CommandResult,
) {
    match command_result {
        Ok(()) => eprintln!("Processed command '{}'", command_name),
        Err(cmd_why) => {
            if let Err(send_msg_why) = msg
                .channel_id
                .send_message(&ctx.http, |m| m.embed(|e| e.description(&cmd_why)))
                .await
            {
                eprintln!(
                    "Failed to report command failure: {:?}, {:?}",
                    send_msg_why, cmd_why
                )
            }
        }
    }
}
