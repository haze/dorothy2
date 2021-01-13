/// This file is the preferred interface for remote GPT3
use crate::{
    transformers::{
        self,
        conversation::{self, LogItem},
        LogTransformer,
    },
    Session,
};
use serenity::{
    framework::standard::{Args, CommandError, CommandResult},
    model::{channel::Message, id::ChannelId},
    prelude::Context,
};

use std::fmt;
// const GPT_MAX_TOKEN_LEN: usize = 2_049;

pub struct GPT3MessageHandler {
    pub transformer: TransformerKind,
    pub message_log: Vec<LogItem>,
    pub configuration: CompletionParameters,
    pub token_count: usize,
}

impl fmt::Display for GPT3MessageHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GPT3 tokens={} config={:?} transformer={:?}",
            self.token_count, self.configuration, self.transformer
        )
    }
}

impl GPT3MessageHandler {
    pub fn new(transformer: TransformerKind) -> GPT3MessageHandler {
        let configuration = transformer.default_configuration();
        GPT3MessageHandler {
            transformer,
            message_log: Vec::new(),
            configuration,
            token_count: 0,
        }
    }

    pub fn set_engine(&mut self, engine: String) {
        self.configuration.engine = engine;
    }

    pub async fn record(&mut self, log_item: LogItem, gpt_token: &str) -> crate::error::Result<()> {
        self.message_log.push(log_item);
        self.update_token_count(gpt_token).await
    }

    pub fn make_string(&self) -> Result<String, std::fmt::Error> {
        let mut buf = String::new();
        self.transformer.prepare(&mut buf)?;
        for log_item in &self.message_log {
            self.transformer.transform(&mut buf, log_item)?;
        }
        Ok(buf)
    }

    pub async fn update_token_count(&mut self, gpt_token: &str) -> crate::error::Result<()> {
        let current_state = self.make_string()?;
        self.update_token_count_raw(gpt_token, current_state).await
    }

    pub async fn update_token_count_raw(
        &mut self,
        gpt_token: &str,
        text: String,
    ) -> crate::error::Result<()> {
        if let Some(token_count) = count_tokens(gpt_token, text, self.configuration.clone())
            .await
            .map_err(|s_err| crate::error::Error::Surf(s_err.to_string()))?
        {
            self.token_count = token_count;
        }
        Ok(())
    }

    pub fn make_prompt(
        &self,
        partial_completion: Option<&String>,
    ) -> Result<String, std::fmt::Error> {
        let mut start = self.make_string()?;
        self.transformer.append_prompt(&mut start)?;
        if let Some(completion) = partial_completion {
            start.push_str(&*completion);
        }
        Ok(start)
    }

    pub fn get_stop_params(&self) -> Option<Vec<String>> {
        self.transformer.get_stop_params()
    }

    pub async fn ensure_is_safe(&mut self, gpt_token: &str) -> crate::error::Result<()> {
        // 500 token hard cap
        // TODO(haze): rethink about
        while self.token_count > 500 {
            self.message_log.drain(0..self.message_log.len() / 2);
            self.update_token_count(gpt_token).await?;
            println!(
                "Token count after trimming: {}, {} log lines",
                self.token_count,
                self.message_log.len()
            );
        }
        Ok(())
    }

    /// Performs a GPT3 completion
    pub async fn get_response(
        &self,
        gpt_token: &str,
        params: CompletionParameters,
    ) -> crate::error::Result<Option<String>> {
        let mut answer_buf = String::new();
        loop {
            let prompt = self.make_prompt(if answer_buf.is_empty() {
                None
            } else {
                Some(&answer_buf)
            })?;
            println!("\n---\n{}\n---\n", &*prompt);
            let response = create_completion(
                gpt_token,
                CompletionParameters {
                    prompt: Some(prompt),
                    n: Some(1),
                    best_of: Some(1),
                    stop: self.get_stop_params(),
                    ..params.clone()
                },
            )
            .await
            .map_err(|e| crate::error::Error::Surf(e.to_string()))?;
            match response {
                CompletionResponse::Success { choices, .. } => {
                    if let Some(first_choice) = choices.first() {
                        dbg!(&first_choice);
                        answer_buf.push_str(&*first_choice.text);
                        if let Some(FinishReason::Stop) = first_choice.finish_reason {
                            return Ok(Some(answer_buf));
                        }
                    }
                }
                CompletionResponse::Error {
                    error: CompletionError { message, .. },
                } => {
                    eprintln!("Failed to create completion: {:?}", &message);
                    return Ok(None);
                }
            }
        }
    }
}

pub struct Payload {
    pub token: String,
    pub channel_id: ChannelId,
}

#[serenity::async_trait]
impl super::MessageSessionHandler for GPT3MessageHandler {
    type Payload = Payload;

    async fn reset(&mut self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        self.message_log.clear();
        msg.react(&ctx, '✅').await?;
        Ok(())
    }

    async fn enable(ctx: &Context, msg: &Message, mut args: Args) -> Result<Session, CommandError> {
        let (transformer, engine) = log_transformer_from_serenity_args(ctx, &mut args).await?;
        let mut handler = GPT3MessageHandler::new(transformer);
        handler.set_engine(engine);

        msg.react(&ctx, '✅').await?;
        Ok(Session::GPT3(handler))
    }

    async fn info(&self, ctx: &Context, msg: &Message, args: Args) -> CommandResult {
        let config = &self.configuration;
        if let Err(why) = msg
            .channel_id
            .send_message(&ctx, |c_m| {
                c_m.embed(|e| {
                    // add config
                    let mut e = e
                        .field("engine", config.engine.clone(), true)
                        .field(
                            "temperature",
                            config
                                .temperature
                                .map(|val| val.to_string())
                                .unwrap_or_else(|| String::from("None")),
                            true,
                        )
                        .field(
                            "top_p",
                            config
                                .top_p
                                .map(|val| val.to_string())
                                .unwrap_or_else(|| String::from("None")),
                            true,
                        )
                        .field(
                            "presence_penalty",
                            config
                                .presence_penalty
                                .map(|val| val.to_string())
                                .unwrap_or_else(|| String::from("None")),
                            true,
                        )
                        .field(
                            "frequency_penalty",
                            config
                                .frequency_penalty
                                .map(|val| val.to_string())
                                .unwrap_or_else(|| String::from("None")),
                            true,
                        )
                        .field("tokens", self.token_count.to_string(), true);
                    if let Some(context) = self.transformer.get_context() {
                        e = e.description(context.clone());
                    }
                    e
                })
            })
            .await
        {
            eprintln!("Failed to send info embed: {:?}", &why);
        }
        Ok(())
    }

    async fn perform_work(&mut self, http: &serenity::http::Http, payload: Self::Payload) {
        match self
            .get_response(&*payload.token, self.configuration.clone())
            .await
        {
            Ok(Some(gpt3_response)) => {
                let gpt3_response = gpt3_response.trim();
                if gpt3_response.is_empty() {
                    eprintln!("GPT33 Generated an empty response, try again.");
                    return;
                }
                if let Err(why) = self
                    .record(
                        transformers::conversation::LogItem {
                            author_name: None,
                            author_nick: None,
                            text: gpt3_response.to_string(),
                            sent_by_ai: true,
                        },
                        &*payload.token,
                    )
                    .await
                {
                    eprintln!("Failed to record line: {:?}", why);
                } else {
                    eprintln!("before token check token count: {}", self.token_count);
                    if let Err(why) = self.ensure_is_safe(&*payload.token).await {
                        eprintln!(
                            "Failed to delete enough chat logs to ensure safe self: {}",
                            &why
                        );
                    }
                    let mut message_builder = serenity::utils::MessageBuilder::new();
                    let message = message_builder.push_safe(gpt3_response);
                    if let Err(why) = payload
                        .channel_id
                        .send_message(&http, |m| m.content(message))
                        .await
                    {
                        eprintln!("Failed to send message to {}", &why);
                    }
                }
            }
            Ok(None) => {
                eprintln!("GPT3 returned no response");
            }
            Err(why) => {
                eprintln!("Failed to create completion: {}", &why);
            }
        }
    }
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
    fn default_configuration(&self) -> CompletionParameters {
        match self {
            TransformerKind::Conversation(trans) => trans,
        }
        .default_configuration()
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

fn get_engine_url(engine: &str) -> String {
    format!("https://api.openai.com/v1/engines/{}/completions", engine)
}

async fn create_completion(
    api_key: &str,
    params: CompletionParameters,
) -> Result<CompletionResponse, surf::Error> {
    let url = get_engine_url(&*params.engine);
    let req = surf::post(url).header("Authorization", format!("Bearer {}", api_key));
    let body = req
        .body(surf::Body::from_json(&params)?)
        .recv_string()
        .await?;
    // println!("{}", &body);
    eprintln!("Read {} long response", body.len());
    Ok(serde_json::from_str(&body).expect("omg"))
}

#[derive(Default, Debug, Clone, serde::Serialize)]
pub struct CompletionParameters {
    #[serde(skip)]
    pub engine: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_of: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum FinishReason {
    Length,
    Stop,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum CompletionResponse {
    Success {
        id: String,
        object: Option<serde_json::Value>,

        // TODO(haze): replace with chrono time
        created: usize,
        model: String,
        choices: Vec<Choice>,
    },
    Error {
        error: CompletionError,
    },
}

#[derive(Debug, serde::Deserialize)]
struct CompletionError {
    code: Option<usize>,
    message: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, serde::Deserialize)]
struct Choice {
    text: String,
    index: usize,
    logprobs: Option<LogProbs>,
    finish_reason: Option<FinishReason>,
}

#[derive(Debug, serde::Deserialize)]
struct LogProbs {
    tokens: Vec<String>,
}

async fn count_tokens(
    api_key: &str,
    text: String,
    params: CompletionParameters,
) -> Result<Option<usize>, surf::Error> {
    let response = create_completion(
        api_key,
        CompletionParameters {
            prompt: Some(text),
            logprobs: Some(10),
            max_tokens: Some(0),
            echo: Some(true),
            ..params
        },
    )
    .await?;
    if let CompletionResponse::Success { choices, .. } = response {
        if let Some(first_choice_logprobs) =
            choices.first().and_then(|choice| choice.logprobs.as_ref())
        {
            return Ok(Some(first_choice_logprobs.tokens.len()));
        }
    }
    Ok(None)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_conversation_to_string() {
        let token = "test_token";
        let mut session =
            GPT3MessageHandler::new(TransformerKind::Conversation(conversation::Transformer {
                ai_name: String::from("Ai"),
                context: Some(String::from("context here!")),
            }));
        session.record(
            LogItem {
                author_name: Some(String::from("foo")),
                author_nick: Some(String::from("foo-nick")),
                text: String::from("bar"),
                sent_by_ai: false,
            },
            &token,
        );
        session.record(
            LogItem {
                author_name: Some(String::from("fredi")),
                author_nick: Some(String::from("foo-nick")),
                text: String::from("hello, world"),
                sent_by_ai: false,
            },
            &token,
        );
        session.record(
            LogItem {
                author_name: None,
                author_nick: None,
                text: String::from("hello, human"),
                sent_by_ai: true,
            },
            &token,
        );
        assert_eq!(
            session
                .to_string()
                .expect("Session should be able to be converted into a string")
                .len(),
            61
        );
    }
}
