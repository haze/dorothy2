pub mod conversation;
use crate::{gpt2, gpt3};
use conversation::LogItem;
use gpt3::CompletionParameters;

pub trait LogTransformer {
    fn default_configuration(&self) -> CompletionParameters;
    fn default_gpt2_configuration(&self) -> CompletionParameters;
    fn on_human_line_observed(&mut self, line: &str);
    fn on_ai_line_observed(&mut self, line: &str);

    fn stop_tokens(&self) -> Option<Vec<String>>;

    fn append_prompt(&self, buf: impl std::fmt::Write) -> std::fmt::Result;
    fn prepare(&self, buf: impl std::fmt::Write) -> std::fmt::Result;
    fn transform(&self, buf: impl std::fmt::Write, log_item: &LogItem) -> std::fmt::Result;
}

#[derive(Debug)]
pub enum TransformerKind {
    Conversation(conversation::Transformer),
}

impl TransformerKind {
    pub fn get_context(&self) -> &Option<String> {
        match self {
            TransformerKind::Conversation(convo) => &convo.context,
        }
    }
    pub fn set_context(&mut self, context: &str) {
        match self {
            TransformerKind::Conversation(convo) => convo.context = Some(context.to_string()),
        }
    }
    pub fn default_gpt2_configuration(&self) -> gpt2::Configuration {
        match self {
            TransformerKind::Conversation(trans) => trans,
        }
        .default_gpt2_configuration()
    }

    pub fn default_gpt3_configuration(&self) -> gpt3::CompletionParameters {
        match self {
            TransformerKind::Conversation(trans) => trans,
        }
        .default_gpt3_configuration()
    }

    fn prepare(&self, buf: impl std::fmt::Write) -> std::fmt::Result {
        match self {
            TransformerKind::Conversation(trans) => trans,
        }
        .prepare(buf)
    }
    fn transform(&self, buf: impl std::fmt::Write, log_item: &LogItem) -> std::fmt::Result {
        match self {
            TransformerKind::Conversation(trans) => trans,
        }
        .transform(buf, log_item)
    }
    fn append_prompt(&self, buf: impl std::fmt::Write) -> std::fmt::Result {
        match self {
            TransformerKind::Conversation(trans) => trans,
        }
        .append_prompt(buf)
    }
    fn get_stop_params(&self) -> Option<Vec<String>> {
        match self {
            TransformerKind::Conversation(trans) => trans,
        }
        .stop_tokens()
    }
}
