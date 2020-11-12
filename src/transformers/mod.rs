pub mod conversation;
use crate::gpt3::CompletionParameters;
use conversation::LogItem;

pub trait LogTransformer {
    fn default_configuration(&self) -> CompletionParameters;
    fn on_human_line_observed(&mut self, line: &str);
    fn on_ai_line_observed(&mut self, line: &str);

    fn stop_tokens(&self) -> Option<Vec<String>>;

    fn append_prompt(&self, buf: impl std::fmt::Write) -> std::fmt::Result;
    fn prepare(&self, buf: impl std::fmt::Write) -> std::fmt::Result;
    fn transform(&self, buf: impl std::fmt::Write, log_item: &LogItem) -> std::fmt::Result;
}
