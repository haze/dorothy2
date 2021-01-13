use super::LogTransformer;
use crate::gpt3::CompletionParameters;

#[derive(Debug)]
pub struct LogItem {
    pub author_name: Option<String>,
    pub author_nick: Option<String>,
    pub text: String,
    pub sent_by_ai: bool,
}

impl LogItem {
    pub fn user_identifier(&self) -> String {
        if let Some(ref nick) = self.author_nick {
            format!("User ({})", nick)
        } else if let Some(ref name) = self.author_name {
            format!("User ({})", name)
        } else {
            String::from("Somebody")
        }
    }
}

#[derive(Debug)]
pub struct Transformer {
    pub ai_name: String,
    pub context: Option<String>,
}

impl LogTransformer for Transformer {
    fn stop_tokens(&self) -> Option<Vec<String>> {
        Some(vec![
            self.ai_name.clone(),
            '\n'.to_string(),
            String::from("User "),
        ])
    }

    fn default_gpt3_configuration(&self) -> crate::gpt3::CompletionParameters {
        CompletionParameters {
            temperature: Some(0.9_f64),
            top_p: Some(1.0_f64),
            frequency_penalty: Some(0.3_f64),
            best_of: Some(1),
            presence_penalty: Some(0.6_f64),
            ..CompletionParameters::default()
        }
    }
    fn default_gpt2_configuration(&self) -> crate::gpt2::Configuration {}

    fn on_ai_line_observed(&mut self, _line: &str) {}
    fn on_human_line_observed(&mut self, _line: &str) {}

    fn append_prompt(&self, mut buf: impl std::fmt::Write) -> std::fmt::Result {
        write!(buf, "{}: ", self.ai_name)
    }

    fn prepare(&self, mut buf: impl std::fmt::Write) -> std::fmt::Result {
        if let Some(ref ctx) = self.context {
            write!(buf, "{}\n\n", ctx)?;
        }
        Ok(())
    }

    fn transform(&self, mut buf: impl std::fmt::Write, log_item: &LogItem) -> std::fmt::Result {
        // TODO(haze): wasted space here
        let user_identifier = log_item.user_identifier();
        write!(
            buf,
            "{}: ",
            if log_item.sent_by_ai {
                &self.ai_name
            } else {
                &user_identifier
            }
        )?;
        writeln!(buf, "{}", log_item.text)
    }
}
