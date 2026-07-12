use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use fusion_core::config::{is_cloudflare_model, Config, Provider};
use fusion_core::error::FusionError;

// ── Rate-limit / concurrency guards (shared across parent + sub-agents) ──────
// Strategy adapted from reference/opencode (retry headers), reference/pi
// (retryable vs terminal limits), reference/codex (jittered exponential
// backoff), and reference/grok-cli (Retry-After + multi-attempt 429 recovery).

/// Max concurrent in-flight LLM HTTP requests process-wide.
/// Parent + spawned sub-agents share this gate so swarm bursts do not trip
/// provider rate limits.
const DEFAULT_LLM_MAX_CONCURRENT: usize = 2;
/// Extra attempts after the first failure (total tries = 1 + this).
const DEFAULT_LLM_MAX_RETRIES: u32 = 5;
/// Initial backoff when no Retry-After header is present (opencode: 2s).
const RETRY_INITIAL_DELAY: Duration = Duration::from_secs(2);
/// Cap for header-less exponential backoff (opencode: 30s).
const RETRY_MAX_DELAY_NO_HEADERS: Duration = Duration::from_secs(30);
/// Absolute cap when the provider asks for a very long wait (pi: 60s default).
const RETRY_MAX_DELAY_WITH_HEADERS: Duration = Duration::from_secs(60);

static LLM_API_GATE: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn llm_api_gate() -> Arc<Semaphore> {
    LLM_API_GATE
        .get_or_init(|| {
            let max = std::env::var("FUSION_LLM_MAX_CONCURRENT")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|&n| n > 0)
                .unwrap_or(DEFAULT_LLM_MAX_CONCURRENT);
            Arc::new(Semaphore::new(max))
        })
        .clone()
}

/// Configure the process-wide concurrent LLM request limit.
/// Call before the first `chat()` for it to take effect (OnceLock).
pub fn configure_llm_max_concurrent(max: usize) {
    let max = max.max(1);
    let _ = LLM_API_GATE.set(Arc::new(Semaphore::new(max)));
}

async fn acquire_llm_permit() -> Result<OwnedSemaphorePermit, FusionError> {
    llm_api_gate()
        .acquire_owned()
        .await
        .map_err(|_| FusionError::Llm("LLM request gate closed".into()))
}

pub mod faux {
    use super::ChatResult;
    use std::sync::{Mutex, OnceLock};
    use std::collections::VecDeque;

    static FAUX_RESPONSES: OnceLock<Mutex<VecDeque<Result<ChatResult, String>>>> = OnceLock::new();

    pub fn get_faux_responses() -> &'static Mutex<VecDeque<Result<ChatResult, String>>> {
        FAUX_RESPONSES.get_or_init(|| Mutex::new(VecDeque::new()))
    }

    pub fn set_responses(responses: Vec<Result<ChatResult, String>>) {
        let mut lock = get_faux_responses().lock().unwrap();
        lock.clear();
        lock.extend(responses);
    }

    pub fn append_responses(responses: Vec<Result<ChatResult, String>>) {
        let mut lock = get_faux_responses().lock().unwrap();
        lock.extend(responses);
    }

    pub fn clear_responses() {
        let mut lock = get_faux_responses().lock().unwrap();
        lock.clear();
    }
}

/// A single chat message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system", "user", "assistant", "tool"
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: Some(tool_calls),
        }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            name: None,
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

/// Options for a chat completion request.
#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub messages: Vec<ChatMessage>,
    pub tools: Option<Vec<serde_json::Value>>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

/// A parsed tool call from the model response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Result of a chat completion.
#[derive(Debug, Clone)]
pub struct ChatResult {
    pub content: String,
    pub reasoning_content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

/// Streaming token/reasoning event emitted to the agent.
#[derive(Debug, Clone)]
pub enum LlmEvent {
    Thinking(String),
    TextDelta(String),
}

/// Unified LLM client that routes to the correct provider.
pub struct LlmClient {
    config: Config,
    http: reqwest::Client,
}

impl LlmClient {
    pub fn new(config: &Config) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(300))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            config: config.clone(),
            http,
        }
    }

    /// Retrieve the currently active model ID.
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Update the target model dynamically.
    pub fn update_model(&mut self, model: &str) {
        self.config.model = model.to_string();
    }

    /// Hot-swap the entire config (API key, provider, account ID, etc.)
    /// while reusing the existing HTTP connection pool.
    pub fn update_config(&mut self, config: &Config) {
        self.config = config.clone();
    }

    /// Send a chat completion request. If event_tx is provided, it streams chunks in real-time.
    ///
    /// All clients (parent agent + spawned sub-agents) share a process-wide
    /// concurrency gate and retry policy so parallel swarm work does not burst
    /// the provider into 429 lockouts.
    pub async fn chat(
        &self,
        options: ChatOptions,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<LlmEvent>>,
    ) -> Result<ChatResult, FusionError> {
        let max_retries = std::env::var("FUSION_LLM_MAX_RETRIES")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(DEFAULT_LLM_MAX_RETRIES);

        let mut attempt: u32 = 0;

        loop {
            // Hold the permit for the full request so concurrent sub-agents
            // queue instead of flooding the API.
            let _permit = acquire_llm_permit().await?;

            match self.chat_attempt(options.clone(), event_tx.clone()).await {
                Ok(res) => return Ok(res),
                Err(err) => {
                    if attempt >= max_retries || !err.retryable {
                        return Err(FusionError::Llm(err.message));
                    }

                    attempt += 1;
                    let delay = retry_delay(attempt, err.retry_after, err.is_rate_limit);
                    // Drop permit before sleeping so other agents can proceed.
                    drop(_permit);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    async fn chat_attempt(
        &self,
        options: ChatOptions,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<LlmEvent>>,
    ) -> Result<ChatResult, FusionError> {
        if self.config.provider == Provider::Faux {
            let mut lock = faux::get_faux_responses().lock().unwrap();
            if let Some(res) = lock.pop_front() {
                match res {
                    Ok(chat_res) => {
                        if let Some(ref tx) = event_tx {
                            if let Some(ref reasoning) = chat_res.reasoning_content {
                                let _ = tx.send(LlmEvent::Thinking(reasoning.clone()));
                            }
                            if !chat_res.content.is_empty() {
                                let _ = tx.send(LlmEvent::TextDelta(chat_res.content.clone()));
                            }
                        }
                        return Ok(chat_res);
                    }
                    Err(e) => {
                        return Err(FusionError::Llm(e));
                    }
                }
            } else {
                return Err(FusionError::Llm("No faux responses queued".into()));
            }
        }

        let account_id = self
            .config
            .cloudflare_account_id
            .clone()
            .unwrap_or_default();
        let api_key = self.config.api_key.clone();

        let is_cf =
            self.config.provider == Provider::Cloudflare || is_cloudflare_model(&self.config.model);

        let (url, auth_header) = if is_cf {
            if account_id.is_empty() {
                return Err(FusionError::Llm(
                    "CLOUDFLARE_ACCOUNT_ID is required when using Cloudflare Workers AI.\n\
                     Tip: export CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN, or set them in fusion.toml \
                     under [provider.cloudflare]."
                        .into(),
                ));
            }
            if api_key.is_empty() {
                return Err(FusionError::Llm(
                    "CLOUDFLARE_API_TOKEN is required for Cloudflare Workers AI.\n\
                     Tip: export CLOUDFLARE_API_TOKEN, or set api_key in fusion.toml \
                     under [provider.cloudflare]."
                        .into(),
                ));
            }
            let url = format!(
                "https://api.cloudflare.com/client/v4/accounts/{}/ai/v1/chat/completions",
                account_id
            );
            (url, format!("Bearer {}", api_key))
        } else {
            let base_url = if self.config.base_url.is_empty() {
                "https://api.openai.com/v1".to_string()
            } else {
                self.config.base_url.clone()
            };
            let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
            (url, format!("Bearer {}", api_key))
        };

        // Determine max_tokens
        let max_tokens = options.max_tokens.or_else(|| {
            fusion_core::models::lookup_model(&self.config.model)
                .and_then(|info| info.max_tokens_for(fusion_core::models::TokenLevel::Normal))
                .or(Some(4096))
        });

        let is_streaming = event_tx.is_some();

        // Construct request payload
        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": serialize_messages(&options.messages, is_cf),
            "temperature": options.temperature.unwrap_or(0.4),
            "stream": is_streaming,
        });

        if let Some(max) = max_tokens {
            body["max_tokens"] = serde_json::json!(max);
        }

        if let Some(ref tools) = options.tools {
            body["tools"] = serde_json::json!(tools);
        }

        let request_builder = self
            .http
            .post(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .json(&body);

        if !is_streaming {
            let resp = request_builder
                .send()
                .await
                .map_err(|e| FusionError::Llm(format!("Failed to send request: {}", e)))?;

            let status = resp.status();
            if !status.is_success() {
                let body_text = resp.text().await.unwrap_or_default();
                return Err(FusionError::Llm(format!(
                    "API error (Status {}): {}",
                    status, body_text
                )));
            }

            let response_json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| FusionError::Llm(format!("Failed to parse JSON response: {}", e)))?;

            let mut accumulated_content = String::new();
            let mut accumulated_reasoning = String::new();
            let mut final_tool_calls = Vec::new();

            if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
                if let Some(choice) = choices.first() {
                    if let Some(message) = choice.get("message") {
                        if let Some(content_str) = message.get("content").and_then(|c| c.as_str()) {
                            accumulated_content = content_str.to_string();
                        }

                        let reasoning_str = message
                            .get("reasoning_content")
                            .or_else(|| message.get("reasoning"))
                            .and_then(|r| r.as_str());
                        if let Some(r_str) = reasoning_str {
                            accumulated_reasoning = r_str.to_string();
                        }

                        if let Some(tool_calls_arr) =
                            message.get("tool_calls").and_then(|t| t.as_array())
                        {
                            for tc_val in tool_calls_arr {
                                if let Some(tool_call) = parse_complete_tool_call(tc_val) {
                                    final_tool_calls.push(tool_call);
                                }
                            }
                        }
                    }
                }
            }

            // Apply the same text-based tool calling fallbacks
            if final_tool_calls.is_empty() && !accumulated_content.is_empty() {
                final_tool_calls = parse_fallback_tool_calls(&accumulated_content);
            }

            return Ok(ChatResult {
                content: accumulated_content,
                reasoning_content: if accumulated_reasoning.is_empty() {
                    None
                } else {
                    Some(accumulated_reasoning)
                },
                tool_calls: final_tool_calls,
            });
        }

        let mut event_source = EventSource::new(request_builder)
            .map_err(|e| FusionError::Llm(format!("Failed to connect to SSE stream: {}", e)))?;

        let mut accumulated_content = String::new();
        let mut accumulated_reasoning = String::new();
        let mut accumulated_tool_calls: Vec<ToolCallBuilder> = Vec::new();
        let mut stream_filter = StreamFilter::new();

        while let Some(event) = event_source.next().await {
            match event {
                Ok(Event::Open) => {}
                Ok(Event::Message(msg)) => {
                    let data = msg.data.trim();
                    if data == "[DONE]" {
                        break;
                    }

                    if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    // content chunk
                                    if let Some(content_str) =
                                        delta.get("content").and_then(|c| c.as_str())
                                    {
                                        if !content_str.is_empty() {
                                            accumulated_content.push_str(content_str);
                                            if let Some(ref tx) = event_tx {
                                                if let Some(clean_chunk) = stream_filter.feed(content_str) {
                                                    let _ = tx.send(LlmEvent::TextDelta(clean_chunk));
                                                }
                                            }
                                        }
                                    }

                                    // reasoning chunk
                                    let reasoning_str = delta
                                        .get("reasoning_content")
                                        .or_else(|| delta.pointer("/choices/0/delta/reasoning"))
                                        .or_else(|| delta.get("reasoning"))
                                        .and_then(|r| r.as_str());

                                    if let Some(r_str) = reasoning_str {
                                        if !r_str.is_empty() {
                                            accumulated_reasoning.push_str(r_str);
                                            if let Some(ref tx) = event_tx {
                                                let _ =
                                                    tx.send(LlmEvent::Thinking(r_str.to_string()));
                                            }
                                        }
                                    }

                                    // OpenAI sends argument strings in fragments. Some compatible
                                    // providers (including Cloudflare models) send a complete JSON
                                    // object instead, so preserve both transport forms.
                                    if let Some(tool_calls_arr) =
                                        delta.get("tool_calls").and_then(|t| t.as_array())
                                    {
                                        accumulate_tool_call_deltas(
                                            tool_calls_arr,
                                            &mut accumulated_tool_calls,
                                        );
                                    }
                                }

                                // A few compatible gateways put the completed tool call on the
                                // choice message even while the response itself is streamed.
                                if let Some(tool_calls_arr) = choice
                                    .pointer("/message/tool_calls")
                                    .and_then(|t| t.as_array())
                                {
                                    accumulate_tool_call_deltas(
                                        tool_calls_arr,
                                        &mut accumulated_tool_calls,
                                    );
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    event_source.close();
                    let err_msg = match e {
                        reqwest_eventsource::Error::InvalidStatusCode(status, resp) => {
                            let body = resp.text().await.unwrap_or_default();
                            format!("Invalid status code {}: {}", status, body)
                        }
                        other => other.to_string(),
                    };
                    return Err(FusionError::Llm(format!("Stream error: {}", err_msg)));
                }
            }
        }

        if let Some(ref tx) = event_tx {
            if let Some(flushed) = stream_filter.flush() {
                let _ = tx.send(LlmEvent::TextDelta(flushed));
            }
        }

        let mut final_tool_calls: Vec<ToolCall> = accumulated_tool_calls
            .into_iter()
            .filter(|b| !b.name.is_empty())
            .map(|b| ToolCall {
                id: b.id,
                name: b.name,
                arguments: parse_accumulated_arguments(&b.arguments),
            })
            .collect();

        // FALLBACK: Parse XML-style and plain-text tool calls from the text response body
        let mut final_content = accumulated_content.clone();
        if final_tool_calls.is_empty() && !accumulated_content.is_empty() {
            final_tool_calls = parse_fallback_tool_calls(&accumulated_content);
            if !final_tool_calls.is_empty() {
                final_content = strip_fallback_tool_calls(&accumulated_content);
            }
        }

        Ok(ChatResult {
            content: final_content,
            reasoning_content: if accumulated_reasoning.is_empty() {
                None
            } else {
                Some(accumulated_reasoning)
            },
            tool_calls: final_tool_calls,
        })
    }
}

fn is_retryable_error(message: &str) -> bool {
    [
        "408",
        "429",
        "500",
        "502",
        "503",
        "504",
        "Too Many Requests",
    ]
    .iter()
    .any(|status| message.contains(status))
}

#[derive(Default, Clone)]
struct ToolCallBuilder {
    id: String,
    name: String,
    arguments: String,
}

fn parse_complete_tool_call(value: &serde_json::Value) -> Option<ToolCall> {
    let name = value
        .pointer("/function/name")
        .or_else(|| value.get("name"))
        .and_then(serde_json::Value::as_str)?;
    let arguments = value
        .pointer("/function/arguments")
        .or_else(|| value.get("arguments"))
        .map(parse_argument_value)
        .unwrap_or_else(|| malformed_arguments("missing arguments"));

    Some(ToolCall {
        id: value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        name: name.to_string(),
        arguments,
    })
}

fn accumulate_tool_call_deltas(values: &[serde_json::Value], builders: &mut Vec<ToolCallBuilder>) {
    for value in values {
        let index = value
            .get("index")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        if index >= builders.len() {
            builders.resize(index + 1, ToolCallBuilder::default());
        }
        let builder = &mut builders[index];

        if let Some(id) = value.get("id").and_then(serde_json::Value::as_str) {
            builder.id = id.to_string();
        }
        if let Some(name) = value
            .pointer("/function/name")
            .or_else(|| value.get("name"))
            .and_then(serde_json::Value::as_str)
        {
            builder.name = name.to_string();
        }
        if let Some(arguments) = value
            .pointer("/function/arguments")
            .or_else(|| value.get("arguments"))
        {
            match arguments {
                serde_json::Value::String(fragment) => builder.arguments.push_str(fragment),
                serde_json::Value::Null => {}
                complete => {
                    builder.arguments = serde_json::to_string(complete).unwrap_or_default();
                }
            }
        }
    }
}

fn parse_argument_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(raw) => parse_accumulated_arguments(raw),
        other => other.clone(),
    }
}

fn parse_accumulated_arguments(raw: &str) -> serde_json::Value {
    if raw.trim().is_empty() {
        return malformed_arguments("empty arguments");
    }
    serde_json::from_str(raw)
        .unwrap_or_else(|error| malformed_arguments(&format!("invalid argument JSON: {}", error)))
}

fn malformed_arguments(reason: &str) -> serde_json::Value {
    serde_json::json!({ "_fusion_tool_error": reason })
}

fn extract_local_image_paths(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut current = content;

    while let Some(start_idx) = current.find("file://") {
        let path_start = start_idx + 7; // skip "file://"
        let mut path_end = path_start;
        for c in current[path_start..].chars() {
            if c == ')' || c == ']' || c == '\n' || c == '\r' || c == ' ' || c == '"' || c == '\'' {
                break;
            }
            path_end += c.len_utf8();
        }

        if path_end > path_start {
            let path = current[path_start..path_end].to_string();
            let ext = std::path::Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext == "png" || ext == "jpg" || ext == "jpeg" || ext == "webp" || ext == "gif" {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        current = &current[path_end..];
    }
    paths
}

fn serialize_messages(messages: &[ChatMessage], is_cf: bool) -> serde_json::Value {
    use base64::Engine;

    let mut serialized = Vec::new();
    for msg in messages {
        let mut map = serde_json::Map::new();

        if is_cf {
            if msg.role == "tool" {
                map.insert("role".to_string(), serde_json::json!("user"));
                map.insert(
                    "content".to_string(),
                    serde_json::json!(format!(
                        "<tool_response name=\"{}\">{}</tool_response>",
                        msg.name.as_deref().unwrap_or(""),
                        msg.content
                    )),
                );
                serialized.push(serde_json::Value::Object(map));
                continue;
            }

            if msg.role == "assistant" && msg.tool_calls.is_some() {
                map.insert("role".to_string(), serde_json::json!("assistant"));
                let mut content = msg.content.clone();
                if let Some(ref tcs) = msg.tool_calls {
                    for tc in tcs {
                        if !content.is_empty() {
                            content.push_str("\n");
                        }
                        let args_str = if tc.arguments.is_string() {
                            tc.arguments.as_str().unwrap_or_default().to_string()
                        } else {
                            serde_json::to_string(&tc.arguments).unwrap_or_default()
                        };
                        content.push_str(&format!(
                            "<tool_call name=\"{}\">{}</tool_call>",
                            tc.name, args_str
                        ));
                    }
                }
                map.insert("content".to_string(), serde_json::json!(content));
                serialized.push(serde_json::Value::Object(map));
                continue;
            }
        }

        map.insert("role".to_string(), serde_json::json!(msg.role));

        if msg.role == "user" {
            let image_paths = extract_local_image_paths(&msg.content);
            if !image_paths.is_empty() {
                let mut content_parts = Vec::new();

                // Add the text prompt part
                content_parts.push(serde_json::json!({
                    "type": "text",
                    "text": msg.content
                }));

                // Add each image part
                for path_str in image_paths {
                    let path = std::path::Path::new(&path_str);
                    if path.exists() && path.is_file() {
                        if let Ok(bytes) = std::fs::read(path) {
                            let base64_data =
                                base64::engine::general_purpose::STANDARD.encode(&bytes);
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("png")
                                .to_lowercase();
                            let media_type = match ext.as_str() {
                                "jpg" | "jpeg" => "image/jpeg",
                                "webp" => "image/webp",
                                "gif" => "image/gif",
                                _ => "image/png",
                            };

                            content_parts.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": {
                                    "url": format!("data:{};base64,{}", media_type, base64_data)
                                }
                            }));
                        }
                    }
                }

                map.insert(
                    "content".to_string(),
                    serde_json::Value::Array(content_parts),
                );
            } else {
                map.insert("content".to_string(), serde_json::json!(msg.content));
            }
        } else {
            map.insert("content".to_string(), serde_json::json!(msg.content));
        }

        if let Some(ref name) = msg.name {
            map.insert("name".to_string(), serde_json::json!(name));
        }

        if let Some(ref tool_call_id) = msg.tool_call_id {
            map.insert("tool_call_id".to_string(), serde_json::json!(tool_call_id));
        }

        if let Some(ref tool_calls) = msg.tool_calls {
            let mut tc_arr = Vec::new();
            for tc in tool_calls {
                let mut tc_map = serde_json::Map::new();
                tc_map.insert("id".to_string(), serde_json::json!(tc.id));
                tc_map.insert("type".to_string(), serde_json::json!("function"));

                let mut func_map = serde_json::Map::new();
                func_map.insert("name".to_string(), serde_json::json!(tc.name));

                let args_str = if tc.arguments.is_string() {
                    tc.arguments.as_str().unwrap_or_default().to_string()
                } else {
                    serde_json::to_string(&tc.arguments).unwrap_or_default()
                };
                func_map.insert("arguments".to_string(), serde_json::json!(args_str));

                tc_map.insert("function".to_string(), serde_json::Value::Object(func_map));
                tc_arr.push(serde_json::Value::Object(tc_map));
            }
            map.insert("tool_calls".to_string(), serde_json::Value::Array(tc_arr));
        }

        serialized.push(serde_json::Value::Object(map));
    }
    serde_json::Value::Array(serialized)
}

fn parse_first_json_object(input: &str) -> Option<(serde_json::Value, usize)> {
    let start = input.find('{')?;
    let mut stream =
        serde_json::Deserializer::from_str(&input[start..]).into_iter::<serde_json::Value>();
    let value = stream.next()?.ok()?;
    Some((value, start + stream.byte_offset()))
}

/// Helper function to parse fallback tool calls from natural text or XML tags.
/// Invalid or truncated payloads are ignored rather than executed as `{}`.
pub(crate) fn parse_fallback_tool_calls(accumulated_content: &str) -> Vec<ToolCall> {
    let mut final_tool_calls = Vec::new();

    // Pattern 1: <tool_call name="...">...</tool_call>
    let mut search_content = accumulated_content;
    while let Some(start_idx) = search_content.find("<tool_call name=\"") {
        let name_start = start_idx + 17;
        if let Some(name_end) = search_content[name_start..].find("\"") {
            let tool_name = &search_content[name_start..name_start + name_end];
            let rest = &search_content[name_start + name_end + 2..];
            if let Some((arguments, consumed)) = parse_first_json_object(rest) {
                final_tool_calls.push(ToolCall {
                    id: format!(
                        "call_fb_{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                    ),
                    name: tool_name.to_string(),
                    arguments,
                });
                search_content = &rest[consumed..];
                continue;
            }
        }
        break;
    }

    // Pattern 2: Calling tool [name] with arguments: [JSON]
    if final_tool_calls.is_empty() {
        let mut search_content = accumulated_content;
        while let Some(start_idx) = search_content.find("Calling tool ") {
            let name_start = start_idx + 13;
            if let Some(with_args_idx) = search_content[name_start..].find(" with arguments:") {
                let tool_name = search_content[name_start..name_start + with_args_idx]
                    .trim()
                    .to_string();
                let rest = &search_content[name_start + with_args_idx + 16..];
                if let Some((arguments, consumed)) = parse_first_json_object(rest) {
                    final_tool_calls.push(ToolCall {
                        id: format!(
                            "call_fb_{}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis()
                        ),
                        name: tool_name,
                        arguments,
                    });
                    search_content = &rest[consumed..];
                    continue;
                }
            }
            break;
        }
    }

    final_tool_calls
}

pub(crate) fn strip_fallback_tool_calls(content: &str) -> String {
    let mut clean = content.to_string();

    // 1. Strip XML-style: <tool_call ...>...</tool_call>
    while let Some(start_idx) = clean.find("<tool_call name=\"") {
        if let Some(end_idx) = clean[start_idx..].find("</tool_call>") {
            let total_end = start_idx + end_idx + 12; // length of "</tool_call>" is 12
            clean.replace_range(start_idx..total_end, "");
        } else {
            // If unterminated, strip from <tool_call to end of string
            clean.replace_range(start_idx.., "");
            break;
        }
    }

    // 2. Strip plain-text style: Calling tool ... with arguments: ...
    while let Some(start_idx) = clean.find("Calling tool ") {
        let name_start = start_idx + 13;
        if let Some(with_args_idx) = clean[name_start..].find(" with arguments:") {
            let rest = &clean[name_start + with_args_idx + 16..];
            if let Some((_, consumed)) = parse_first_json_object(rest) {
                let total_end = name_start + with_args_idx + 16 + consumed;
                clean.replace_range(start_idx..total_end, "");
                continue;
            }
        }
        break;
    }

    clean.trim().to_string()
}

pub(crate) struct StreamFilter {
    buffer: String,
    in_xml: bool,
    in_calling: bool,
}

impl StreamFilter {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            in_xml: false,
            in_calling: false,
        }
    }

    pub fn feed(&mut self, chunk: &str) -> Option<String> {
        self.buffer.push_str(chunk);

        // Check if we detect starting of a tool call
        if !self.in_xml && !self.in_calling {
            // Check if buffer contains starting sequence of XML tool call
            if let Some(pos) = self.buffer.find('<') {
                let candidate = &self.buffer[pos..];
                if "<tool_call".starts_with(candidate) || candidate.starts_with("<tool_call") {
                    self.in_xml = true;
                    let prefix = self.buffer[..pos].to_string();
                    self.buffer = candidate.to_string();
                    if !prefix.is_empty() {
                        return Some(prefix);
                    }
                    return None;
                }
            }

            // Check if buffer contains starting sequence of plain-text tool call
            if let Some(pos) = self.buffer.find('C') {
                let candidate = &self.buffer[pos..];
                if "Calling tool ".starts_with(candidate) || candidate.starts_with("Calling tool ") {
                    self.in_calling = true;
                    let prefix = self.buffer[..pos].to_string();
                    self.buffer = candidate.to_string();
                    if !prefix.is_empty() {
                        return Some(prefix);
                    }
                    return None;
                }
            }

            // No suspicious prefix detected, return everything
            let out = self.buffer.clone();
            self.buffer.clear();
            return Some(out);
        }

        if self.in_xml {
            // We are buffering XML-style tool call.
            // Check if we find the closing tag "</tool_call>"
            if let Some(pos) = self.buffer.find("</tool_call>") {
                self.buffer = self.buffer[pos + 12..].to_string(); // Skip </tool_call>
                self.in_xml = false;
                let remainder = self.buffer.clone();
                self.buffer.clear();
                return self.feed(&remainder);
            }
            return None;
        }

        if self.in_calling {
            // We are buffering "Calling tool ..."
            if let Some(args_pos) = self.buffer.find(" with arguments:") {
                let json_part = &self.buffer[args_pos + 16..];
                if let Some((_, consumed)) = parse_first_json_object(json_part) {
                    let total_end = args_pos + 16 + consumed;
                    self.buffer = self.buffer[total_end..].to_string();
                    self.in_calling = false;
                    let remainder = self.buffer.clone();
                    self.buffer.clear();
                    return self.feed(&remainder);
                }
            }
            // If the buffer grows too large without seeing JSON, it might not be a tool call.
            if self.buffer.len() > 1000 {
                let out = self.buffer.clone();
                self.buffer.clear();
                self.in_calling = false;
                return Some(out);
            }
            return None;
        }

        None
    }

    pub fn flush(&mut self) -> Option<String> {
        if !self.buffer.is_empty() {
            let out = self.buffer.clone();
            self.buffer.clear();
            self.in_xml = false;
            self.in_calling = false;
            return Some(out);
        }
        None
    }
}

/// Convenience constructor.
pub fn create_llm_client(config: &Config) -> LlmClient {
    LlmClient::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fusion_core::config::{Config, Provider};

    #[tokio::test]
    async fn test_kimi_live() {
        let config = Config {
            model: "@cf/moonshotai/kimi-k2.7-code".to_string(),
            small_model: None,
            yolo: false,
            provider: Provider::Cloudflare,
            cloudflare_account_id: std::env::var("CLOUDFLARE_ACCOUNT_ID").ok(),
            api_key: std::env::var("CLOUDFLARE_API_TOKEN").unwrap_or_default(),
            base_url: String::new(),
            config_path: None,
            settings: std::collections::HashMap::new(),
        };

        let client = LlmClient::new(&config);
        let options = ChatOptions {
            messages: vec![ChatMessage::user("hi")],
            tools: None,
            temperature: Some(0.4),
            max_tokens: Some(100),
        };

        match client.chat(options, None).await {
            Ok(res) => println!("Success: {:?}", res),
            Err(e) => println!("Error: {:?}", e),
        }
    }
    #[test]
    fn test_multimodal_serialization() {
        let temp_dir = std::env::temp_dir();
        let img_path = temp_dir.join("test_vision_img.png");
        let _ = std::fs::write(&img_path, b"fake_png_data");

        let prompt = format!(
            "Explain this: [image 1](file://{})",
            img_path.to_string_lossy()
        );
        let messages = vec![ChatMessage::user(prompt)];

        let serialized = serialize_messages(&messages, false);
        let content_arr = serialized[0]["content"].as_array().unwrap();

        assert_eq!(content_arr.len(), 2);
        assert_eq!(content_arr[0]["type"], "text");
        assert!(content_arr[0]["text"]
            .as_str()
            .unwrap()
            .contains("Explain this:"));

        assert_eq!(content_arr[1]["type"], "image_url");
        let url = content_arr[1]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(url.contains("ZmFrZV9wbmdfZGF0YQ==")); // base64 of "fake_png_data"

        let _ = std::fs::remove_file(img_path);
    }

    #[test]
    fn test_parse_fallback_tool_calls() {
        // Test standard XML tag parsing with closing tag
        let content = "<tool_call name=\"write_file\">{\"path\": \"src/index.css\", \"content\": \"body {}\"}</tool_call>";
        let calls = parse_fallback_tool_calls(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(calls[0].arguments["path"], "src/index.css");

        // Test truncated XML tag without closing tag
        let content = "<tool_call name=\"delegate_write\">{\"task_description\": \"Build fitness page\", \"files\": [\"index.html\"]}";
        let calls = parse_fallback_tool_calls(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "delegate_write");
        assert_eq!(calls[0].arguments["files"][0], "index.html");

        // Test nested braces parsing
        let content = "<tool_call name=\"run_command\">{\"command\": \"npm run build\", \"env\": {\"NODE_ENV\": \"production\"}}";
        let calls = parse_fallback_tool_calls(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "run_command");
        assert_eq!(calls[0].arguments["env"]["NODE_ENV"], "production");

        // Test plain-text fallback format (Calling tool ...)
        let content = "I will write the code now. Calling tool search_replace with arguments: {\"path\": \"index.js\", \"old\": \"a\", \"new\": \"b\"}";
        let calls = parse_fallback_tool_calls(content);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_replace");
        assert_eq!(calls[0].arguments["new"], "b");

        // Invalid/truncated JSON must never become an executable empty call.
        let content = "<tool_call name=\"write_file\">{\"path\": \"index.css\", \"content\":";
        assert!(parse_fallback_tool_calls(content).is_empty());
    }

    #[test]
    fn test_streamed_object_tool_arguments_are_preserved() {
        let values = vec![serde_json::json!({
            "index": 0,
            "id": "call_write",
            "function": {
                "name": "write_file",
                "arguments": {
                    "path": "src/index.css",
                    "content": "body { color: red; }"
                }
            }
        })];
        let mut builders = Vec::new();

        accumulate_tool_call_deltas(&values, &mut builders);
        let builder = builders.pop().unwrap();
        let arguments = parse_accumulated_arguments(&builder.arguments);

        assert_eq!(builder.name, "write_file");
        assert_eq!(arguments["path"], "src/index.css");
        assert_eq!(arguments["content"], "body { color: red; }");
    }

    #[test]
    fn test_streamed_string_tool_arguments_are_joined() {
        let first = vec![serde_json::json!({
            "index": 0,
            "function": { "name": "write_file", "arguments": "{\"path\":\"a.css\"," }
        })];
        let second = vec![serde_json::json!({
            "index": 0,
            "function": { "arguments": "\"content\":\"x\"}" }
        })];
        let mut builders = Vec::new();

        accumulate_tool_call_deltas(&first, &mut builders);
        accumulate_tool_call_deltas(&second, &mut builders);
        let arguments = parse_accumulated_arguments(&builders[0].arguments);

        assert_eq!(arguments["path"], "a.css");
        assert_eq!(arguments["content"], "x");
    }

    #[test]
    fn test_strip_fallback_tool_calls() {
        let content = "I will write files now. <tool_call name=\"write_file\">{\"path\": \"src/index.css\"}</tool_call> Done.";
        let stripped = strip_fallback_tool_calls(content);
        assert_eq!(stripped, "I will write files now.  Done.");

        let content_plain = "I will run the command. Calling tool run_command with arguments: {\"command\": \"npm test\"} End.";
        let stripped_plain = strip_fallback_tool_calls(content_plain);
        assert_eq!(stripped_plain, "I will run the command.  End.");
    }

    #[test]
    fn test_stream_filter() {
        let mut filter = StreamFilter::new();
        
        let chunk1 = filter.feed("Hello world! <tool_c");
        assert_eq!(chunk1, Some("Hello world! ".to_string()));
        
        let chunk2 = filter.feed("all name=\"test\">{\"x\": 1}</tool_call> Goodbye");
        assert_eq!(chunk2, Some(" Goodbye".to_string()));

        let mut filter2 = StreamFilter::new();
        let chunk3 = filter2.feed("Calling tool test ");
        assert_eq!(chunk3, None); // Should buffer because it might match "Calling tool "
        let chunk4 = filter2.feed("with arguments: {\"a\": 1} Done.");
        assert_eq!(chunk4, Some(" Done.".to_string()));
    }
}
