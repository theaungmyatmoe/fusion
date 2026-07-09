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

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| FusionError::Llm(format!("HTTP error: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(FusionError::Llm(format!(
                "Cloudflare AI error ({}): {}",
                status, text
            )));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| FusionError::Llm(format!("JSON parse error: {}", e)))?;

        // Workers AI returns varied shapes — handle them all
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

        Ok(ChatResult {
            content,
            reasoning_content,
            tool_calls,
        })
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
