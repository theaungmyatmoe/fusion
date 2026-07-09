use fusion_core::config::Config;
use fusion_core::error::FusionError;
use reqwest::Client;

use crate::client::{ChatOptions, ChatResult, ToolCall};

/// Client for Cloudflare Workers AI REST API.
pub struct CloudflareClient {
    http: Client,
    account_id: String,
    api_key: String,
    model: String,
}

impl CloudflareClient {
    pub fn new(config: &Config) -> Self {
        Self {
            http: Client::new(),
            account_id: config.cloudflare_account_id.clone().unwrap_or_default(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
        }
    }

    pub async fn chat(&self, options: ChatOptions) -> Result<ChatResult, FusionError> {
        if self.account_id.is_empty() {
            return Err(FusionError::Llm(
                "CLOUDFLARE_ACCOUNT_ID is required when using Cloudflare Workers AI.\n\
                 Tip: export CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN, or set them in fusion.toml \
                 under [provider.cloudflare]."
                    .into(),
            ));
        }
        if self.api_key.is_empty() {
            return Err(FusionError::Llm(
                "CLOUDFLARE_API_TOKEN is required for Cloudflare Workers AI.\n\
                 Tip: export CLOUDFLARE_API_TOKEN, or set api_key in fusion.toml \
                 under [provider.cloudflare]."
                    .into(),
            ));
        }

        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{}",
            self.account_id, self.model
        );

        let mut body = serde_json::json!({
            "messages": options.messages,
            "temperature": options.temperature.unwrap_or(0.6),
        });

        if let Some(max_tokens) = options.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(ref tools) = options.tools {
            body["tools"] = serde_json::json!(tools);
        }

        // Retry loop with exponential backoff for rate limits
        let max_retries = 3u32;
        let last_err = String::new();

        for attempt in 0..=max_retries {
            let resp = self
                .http
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| FusionError::Llm(format!("HTTP error: {}", e)))?;

            let status = resp.status();

            if status.as_u16() == 429 {
                // Rate limited — check retry-after header if present, fallback to exponential backoff
                let mut delay_secs = 2u64.pow(attempt + 1); // 2s, 4s, 8s, 16s
                if let Some(header_val) = resp.headers().get("retry-after") {
                    if let Ok(val_str) = header_val.to_str() {
                        if let Ok(parsed_secs) = val_str.parse::<u64>() {
                            delay_secs = parsed_secs;
                        }
                    }
                }

                if attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                    continue;
                } else {
                    return Err(FusionError::Llm(
                        "Rate limited by Cloudflare (429). Tried 3 retries. \
                         Wait a moment and try again, or switch models with /model."
                            .into(),
                    ));
                }
            }

            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                // Extract a clean error message from the JSON if possible
                let clean_msg = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|v| {
                        v["errors"][0]["message"]
                            .as_str()
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| {
                        if text.len() > 200 {
                            format!("{}...", &text[..200])
                        } else {
                            text
                        }
                    });
                return Err(FusionError::Llm(format!(
                    "Cloudflare API error ({}): {}",
                    status.as_u16(), clean_msg
                )));
            }

            // Success — parse the response
            let data: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| FusionError::Llm(format!("JSON parse error: {}", e)))?;

            let content = data
                .pointer("/result/choices/0/message/content")
                .and_then(|v| v.as_str())
                .or_else(|| data.pointer("/result/response").and_then(|v| v.as_str()))
                .or_else(|| data.pointer("/result/text").and_then(|v| v.as_str()))
                .or_else(|| data["result"].as_str())
                .unwrap_or("")
                .to_string();

            let reasoning_content = data
                .pointer("/result/choices/0/message/reasoning_content")
                .or_else(|| data.pointer("/result/choices/0/message/reasoning"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let tool_calls = parse_tool_calls(
                data.pointer("/result/choices/0/message/tool_calls"),
            );

            return Ok(ChatResult {
                content,
                reasoning_content,
                tool_calls,
            });
        }

        Err(FusionError::Llm(last_err))
    }
}

fn parse_tool_calls(value: Option<&serde_json::Value>) -> Vec<ToolCall> {
    let Some(arr) = value.and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|tc| {
            let id = tc["id"].as_str().unwrap_or("").to_string();
            let name = tc["function"]["name"]
                .as_str()
                .or_else(|| tc["name"].as_str())
                .unwrap_or("")
                .to_string();

            let arguments = if let Some(args_str) = tc["function"]["arguments"].as_str() {
                serde_json::from_str(args_str).unwrap_or(serde_json::Value::Object(Default::default()))
            } else if let Some(args) = tc["arguments"].as_str() {
                serde_json::from_str(args).unwrap_or(serde_json::Value::Object(Default::default()))
            } else {
                serde_json::Value::Object(Default::default())
            };

            if name.is_empty() {
                None
            } else {
                Some(ToolCall {
                    id,
                    name,
                    arguments,
                })
            }
        })
        .collect()
}
