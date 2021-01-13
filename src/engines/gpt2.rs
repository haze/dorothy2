/// This file is the preferred interface for local GPT2
use crate::transformers::{self, conversation::LogItem, TransformerKind};
use crate::Session;
use serenity::{
    framework::standard::{Args, CommandError, CommandResult},
    model::{channel::Message, id::ChannelId},
    prelude::Context,
};
use std::fmt;

#[derive(Debug)]
pub struct GPT2MessageHandler {
    pub transformer: TransformerKind,
    pub message_log: Vec<LogItem>,
    pub configuration: Configuration,
}

impl GPT2MessageHandler {
    pub fn new(transformer: TransformerKind) -> GPT2MessageHandler {
        let configuration = transformer.default_gpt2_configuration();
        GPT2MessageHandler {
            transformer,
            message_log: Vec::new(),
            configuration,
        }
    }
}

#[derive(Debug)]
pub struct Configuration {}

impl fmt::Display for GPT2MessageHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[serenity::async_trait]
impl super::MessageSessionHandler for GPT2MessageHandler {
    type Payload = i32;

    async fn perform_work(&mut self, http: &serenity::http::Http, payload: Self::Payload) {}
    async fn reset(&mut self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        Ok(())
    }
    async fn enable(ctx: &Context, msg: &Message, args: Args) -> Result<Session, CommandError> {
        let (transformer, engine) = log_transformer_from_serenity_args(ctx, &mut args).await?;
        let mut handler = GPT2MessageHandler::new(transformer);
        handler.set_engine(engine);

        msg.react(&ctx, '✅').await?;

        Ok(Session::GPT2(GPT2MessageHandler {}))
    }
    async fn info(&self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        Ok(())
    }
}

pub async fn log_transformer_from_serenity_args(
    context: &serenity::prelude::Context,
    args: &mut serenity::framework::standard::Args,
) -> Result<(TransformerKind, String), crate::commands::StringError> {
    // 0th, the engine
    let engine = {
        if let Ok(arg) = args.single::<String>() {
            match &*arg.to_lowercase() {
                "default" => String::from("davinci"),
                _ => arg,
            }
        } else {
            return Err("Missing engine (if you aren't sure, use `default`)".into());
        }
    };
    // first, the transform type
    let transform_type = args.single::<String>()?.to_lowercase();
    Ok(match &*transform_type {
        // if we get single, then get the ai name (otherwise, default to the bots name)
        "conversation" | "convo" => {
            let bot_name = context.http.get_current_application_info().await?.name;
            let ai_name = {
                let temp = args
                    .single_quoted()
                    // it's ok to do this since I don't expet this method to be called frequently
                    .unwrap_or_else(|_| bot_name.clone());
                if temp == "_" {
                    bot_name
                } else {
                    temp
                }
            };
            let context = {
                let rest = args.rest();
                let trimmed = rest.trim().trim_matches('`').trim_matches('"').trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            .map(|context| context.replace("{name}", &*ai_name));
            dbg!(&context);
            (
                TransformerKind::Conversation(conversation::Transformer { ai_name, context }),
                engine,
            )
        }
        _ => return Err("Invalid conversation type".into()),
    })
}
