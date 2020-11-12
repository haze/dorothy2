use crate::Session;
use serenity::{
    framework::standard::{Args, CommandError, CommandResult},
    model::{channel::Message, id::ChannelId},
    prelude::Context,
};
use std::fmt;

#[derive(Debug)]
pub struct GPT2MessageHandler {}

impl fmt::Display for GPT2MessageHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[serenity::async_trait]
impl super::MessageSessionHandler for GPT2MessageHandler {
    type Payload = i32;

    async fn on_message(&mut self, http: &serenity::http::Http, payload: Self::Payload) {}
    async fn reset(&mut self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        Ok(())
    }
    async fn enable(ctx: &Context, msg: &Message, args: Args) -> Result<Session, CommandError> {
        Ok(Session::GPT2(GPT2MessageHandler {}))
    }
    async fn info(&self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        Ok(())
    }
}
