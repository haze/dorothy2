pub mod gpt2;
pub mod gpt3;
use crate::Session;
use serenity::{
    framework::standard::{Args, CommandError, CommandResult},
    model::channel::Message,
    prelude::Context,
};

#[serenity::async_trait]
pub trait MessageSessionHandler {
    type Payload;

    async fn perform_work(&mut self, http: &serenity::http::Http, payload: Self::Payload);
    async fn info(&self, ctx: &Context, msg: &Message, args: Args) -> CommandResult;
    async fn enable(ctx: &Context, msg: &Message, args: Args) -> Result<Session, CommandError>;
    async fn reset(&mut self, ctx: &Context, msg: &Message, args: Args) -> CommandResult;
}
