//! `/providers` -- configure custom model profiles for Cloudflare, OpenAI, DeepSeek, and xAI.

use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};
use std::fs;
use toml::Value as TomlValue;
use toml::map::Map as TomlMap;

pub struct ProvidersCommand;

impl SlashCommand for ProvidersCommand {
    fn name(&self) -> &str {
        "providers"
    }

    fn aliases(&self) -> &[&str] {
        &["provider", "key"]
    }

    fn description(&self) -> &str {
        "Configure custom model profiles for Cloudflare, OpenAI, DeepSeek, or xAI"
    }

    fn usage(&self) -> &str {
        "/providers [cloudflare|openai|deepseek|xai] [token/key/account] [value]"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        false
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("[provider] [token/key/account] [value]")
    }

    fn suggest_args(&self, _ctx: &AppCtx, args_query: &str) -> Option<Vec<ArgItem>> {
        let query = args_query.trim_start();
        
        // If query is empty or doesn't have a space, show provider choices
        if !query.contains(' ') {
            return Some(vec![
                ArgItem {
                    display: "cloudflare".to_string(),
                    match_text: "cloudflare".to_string(),
                    insert_text: "cloudflare ".to_string(),
                    description: "Workers AI (2-step setup: token + account ID)".to_string(),
                },
                ArgItem {
                    display: "openai".to_string(),
                    match_text: "openai".to_string(),
                    insert_text: "openai ".to_string(),
                    description: "GPT models (1-step setup: API key)".to_string(),
                },
                ArgItem {
                    display: "deepseek".to_string(),
                    match_text: "deepseek".to_string(),
                    insert_text: "deepseek ".to_string(),
                    description: "DeepSeek models (1-step setup: API key)".to_string(),
                },
                ArgItem {
                    display: "xai".to_string(),
                    match_text: "xai".to_string(),
                    insert_text: "xai ".to_string(),
                    description: "Grok models (1-step setup: API key)".to_string(),
                },
            ]);
        }

        // Otherwise suggest placeholders depending on the selected provider and typed arguments
        let parts: Vec<&str> = query.split_whitespace().collect();
        if let Some(&provider) = parts.first() {
            match provider.to_lowercase().as_str() {
                "cloudflare" => {
                    if parts.len() == 1 {
                        // User typed "/providers cloudflare "
                        return Some(vec![
                            ArgItem {
                                display: "token".to_string(),
                                match_text: "token".to_string(),
                                insert_text: "token ".to_string(),
                                description: "Step 1: Set Cloudflare API Token".to_string(),
                            },
                            ArgItem {
                                display: "account".to_string(),
                                match_text: "account".to_string(),
                                insert_text: "account ".to_string(),
                                description: "Step 2: Set Cloudflare Account ID".to_string(),
                            },
                            ArgItem {
                                display: "<api_token> <account_id>".to_string(),
                                match_text: "".to_string(),
                                insert_text: "".to_string(),
                                description: "Or perform combined one-line setup".to_string(),
                            },
                        ]);
                    } else if parts.len() == 2 {
                        // User typed "/providers cloudflare token " or "/providers cloudflare account "
                        match parts[1].to_lowercase().as_str() {
                            "token" => {
                                return Some(vec![ArgItem {
                                    display: "<api_token>".to_string(),
                                    match_text: "".to_string(),
                                    insert_text: "".to_string(),
                                    description: "Enter/paste your Cloudflare API token".to_string(),
                                }]);
                            }
                            "account" => {
                                return Some(vec![ArgItem {
                                    display: "<account_id>".to_string(),
                                    match_text: "".to_string(),
                                    insert_text: "".to_string(),
                                    description: "Enter/paste your Cloudflare Account ID".to_string(),
                                }]);
                            }
                            _ => {}
                        }
                    }
                }
                "openai" => {
                    if parts.len() == 1 {
                        return Some(vec![ArgItem {
                            display: "<api_key>".to_string(),
                            match_text: "".to_string(),
                            insert_text: "".to_string(),
                            description: "Enter your OpenAI API key".to_string(),
                        }]);
                    }
                }
                "deepseek" => {
                    if parts.len() == 1 {
                        return Some(vec![ArgItem {
                            display: "<api_key>".to_string(),
                            match_text: "".to_string(),
                            insert_text: "".to_string(),
                            description: "Enter your DeepSeek API key".to_string(),
                        }]);
                    }
                }
                "xai" => {
                    if parts.len() == 1 {
                        return Some(vec![ArgItem {
                            display: "<api_key>".to_string(),
                            match_text: "".to_string(),
                            insert_text: "".to_string(),
                            description: "Enter your xAI API key".to_string(),
                        }]);
                    }
                }
                _ => {}
            }
        }

        None
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let parts: Vec<&str> = args.split_whitespace().collect();

        // No args → show provider picker menu
        if parts.is_empty() {
            return CommandResult::Message(
                "╭─ Fusion Provider Setup ───────────────────────────────────╮\n\
                 │  /providers cloudflare   Workers AI (account ID + token)   │\n\
                 │  /providers openai       GPT models (API key)              │\n\
                 │  /providers deepseek     DeepSeek models (API key)         │\n\
                 │  /providers xai          Grok models (API key)             │\n\
                 ╰────────────────────────────────────────────────────────────╯"
                    .to_string(),
            );
        }

        let provider = parts[0].to_lowercase();
        match provider.as_str() {
            "cloudflare" | "openai" | "deepseek" | "xai" => {
                // Open the TUI wizard overlay — no credentials needed in the command
                CommandResult::Action(crate::app::actions::Action::OpenProviderWizard {
                    provider,
                })
            }
            _ => CommandResult::Error(format!(
                "Unknown provider '{}'. Supported: cloudflare, openai, deepseek, xai",
                provider
            )),
        }
    }
}

/// Apply the wizard result for a given provider — called by the router on submit.
///
/// `values` is the ordered list of field inputs from [`ProviderWizardState`].
pub fn apply_wizard_result(
    provider: &str,
    values: &[String],
) -> Result<String, Box<dyn std::error::Error>> {
    match provider {
        "cloudflare" => {
            let account_id = values.first().map(|s| s.as_str()).filter(|s| !s.is_empty());
            let api_token  = values.get(1).map(|s| s.as_str()).filter(|s| !s.is_empty());
            save_all_cf_overrides(api_token, account_id)?;
            Ok("✅ Cloudflare configured! All Workers AI models (Kimi K2, GLM, Qwen, Gemma, Llama, DeepSeek R1) are ready.\nSwitch anytime with /model".to_string())
        }
        "openai" => {
            let key = values.first().map(|s| s.as_str()).unwrap_or("");
            save_provider_override("gpt-4o", "gpt-4o", "OpenAI GPT-4o", Some(key), Some("https://api.openai.com/v1"), 128000)?;
            save_provider_override("gpt-4o-mini", "gpt-4o-mini", "OpenAI GPT-4o Mini", Some(key), Some("https://api.openai.com/v1"), 128000)?;
            Ok("✅ OpenAI configured! Models 'gpt-4o' and 'gpt-4o-mini' are ready.\nSwitch anytime with /model".to_string())
        }
        "deepseek" => {
            let key = values.first().map(|s| s.as_str()).unwrap_or("");
            save_provider_override("deepseek-chat", "deepseek-chat", "DeepSeek Chat", Some(key), Some("https://api.deepseek.com"), 64000)?;
            save_provider_override("deepseek-coder", "deepseek-coder", "DeepSeek Coder", Some(key), Some("https://api.deepseek.com"), 64000)?;
            Ok("✅ DeepSeek configured! Models 'deepseek-chat' and 'deepseek-coder' are ready.\nSwitch anytime with /model".to_string())
        }
        "xai" => {
            let key = values.first().map(|s| s.as_str()).unwrap_or("");
            save_provider_override("grok-build", "grok-build", "Grok Build", Some(key), Some("https://api.x.ai/v1"), 500000)?;
            Ok("✅ xAI configured! Model 'grok-build' is ready.\nSwitch anytime with /model".to_string())
        }
        other => Err(format!("Unknown provider: {}", other).into()),
    }
}

fn save_all_cf_overrides(
    api_key: Option<&str>,
    account_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cf_models = [
        ("cloudflare-kimi-k2.7", "@cf/moonshotai/kimi-k2.7-code", "Kimi K2.7 Code", 131072),
        ("cloudflare-glm-4-7-flash", "@cf/zai-org/glm-4.7-flash", "GLM 4.7 Flash", 131072),
        ("cloudflare-glm-5-2", "@cf/zai-org/glm-5.2", "GLM 5.2 Coder", 262144),
        ("cloudflare-qwen-3", "@cf/qwen/qwen3-30b-a3b-fp8", "Qwen3 30B-A3B", 32768),
        ("cloudflare-qwen-coder", "@cf/qwen/qwen2.5-coder-32b-instruct", "Qwen 2.5 Coder 32B", 32768),
        ("cloudflare-gemma-3-27b", "@cf/google/gemma-3-27b-it", "Gemma 3 27B", 131072),
        ("cloudflare-llama-3-3-70b", "@cf/meta/llama-3.3-70b-instruct-fp8-fast", "Llama 3.3 70B", 8192),
        ("cloudflare-deepseek-r1", "@cf/deepseek-ai/deepseek-r1-0528-qwen3-8b", "DeepSeek R1 8B", 16384),
    ];

    for (model_key, model_id, name, context_window) in cf_models {
        let base_url = account_id.map(|id| format!("https://api.cloudflare.com/client/v4/accounts/{}/ai/v1", id));
        save_provider_override(model_key, model_id, name, api_key, base_url.as_deref(), context_window)?;
    }

    Ok(())
}

fn save_provider_override(
    model_key: &str,
    model_id: &str,
    name: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
    context_window: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = xai_grok_config::grok_home().join("config.toml");
    let content = fs::read_to_string(&path).unwrap_or_default();
    
    let mut root: TomlValue = toml::from_str(&content).unwrap_or_else(|_| TomlValue::Table(TomlMap::new()));
    if !root.is_table() {
        root = TomlValue::Table(TomlMap::new());
    }
    
    let table = root.as_table_mut().unwrap();
    let model_section = table
        .entry("model")
        .or_insert_with(|| TomlValue::Table(TomlMap::new()));
        
    if let Some(model_table_outer) = model_section.as_table_mut() {
        let model_table = model_table_outer
            .entry(model_key.to_string())
            .or_insert_with(|| TomlValue::Table(TomlMap::new()))
            .as_table_mut()
            .unwrap();
            
        model_table.insert("model".to_string(), TomlValue::String(model_id.to_string()));
        model_table.insert("name".to_string(), TomlValue::String(name.to_string()));
        model_table.insert("context_window".to_string(), TomlValue::Integer(context_window));
        model_table.insert("temperature".to_string(), TomlValue::Float(0.7));
        model_table.insert("top_p".to_string(), TomlValue::Float(0.95));
        model_table.insert("api_backend".to_string(), TomlValue::String("chat_completions".to_string()));
        
        if let Some(key) = api_key {
            model_table.insert("api_key".to_string(), TomlValue::String(key.to_string()));
        }
        if let Some(url) = base_url {
            model_table.insert("base_url".to_string(), TomlValue::String(url.to_string()));
        }
    }
    
    let toml_str = toml::to_string_pretty(&root)?;
    fs::write(&path, toml_str)?;
    
    Ok(())
}
