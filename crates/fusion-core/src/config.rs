use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::FusionError;

/// Supported LLM providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Cloudflare,
    Xai,
    #[serde(rename = "openai")]
    OpenAi,
    Auto,
    Faux,
}

impl Default for Provider {
    fn default() -> Self {
        Provider::Auto
    }
}

/// Runtime configuration for Fusion.
#[derive(Debug, Clone)]
pub struct Config {
    pub provider: Provider,
    pub model: String,
    pub small_model: Option<String>,
    pub api_key: String,
    pub base_url: String,
    pub cloudflare_account_id: Option<String>,
    pub yolo: bool,
    pub config_path: Option<PathBuf>,
    pub settings: HashMap<String, serde_json::Value>,
}

// ── TOML config shape (Codex-style, primary format) ──────────────────────────

/// Top-level `fusion.toml` structure — Codex-style TOML config.
///
/// Example `fusion.toml`:
/// ```toml
/// model = "@cf/moonshotai/kimi-k2.7-code"
/// yolo = false
///
/// [provider.cloudflare]
/// account_id = "abc123"
/// api_key = "cfat_..."
///
/// [provider.xai]
/// api_key = "xai-..."
/// ```
#[derive(Debug, Default, Deserialize)]
struct TomlConfig {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    small_model: Option<String>,
    #[serde(default)]
    yolo: Option<bool>,
    #[serde(default)]
    provider: Option<TomlProviderSection>,
    #[serde(default)]
    settings: Option<HashMap<String, toml::Value>>,
}

#[derive(Debug, Default, Deserialize)]
struct TomlProviderSection {
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    cloudflare: Option<TomlProviderEntry>,
    #[serde(default)]
    xai: Option<TomlProviderEntry>,
    #[serde(default)]
    openai: Option<TomlProviderEntry>,
}

#[derive(Debug, Default, Deserialize)]
struct TomlProviderEntry {
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
}

// ── JSON config shape (backward-compat with zencode.json / opencode.json) ────

#[derive(Debug, Default, Deserialize)]
struct JsonConfig {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    small_model: Option<String>,
    #[serde(default, rename = "cloudflareAccountId")]
    cloudflare_account_id: Option<String>,
    #[serde(default, rename = "accountId")]
    account_id: Option<String>,
    #[serde(default, rename = "apiKey")]
    api_key_top: Option<String>,
    #[serde(default, rename = "cloudflareApiToken")]
    cloudflare_api_token: Option<String>,
    #[serde(default)]
    provider: Option<JsonProviderSection>,
    #[serde(default)]
    cloudflare: Option<JsonCloudflareBlock>,
    #[serde(default)]
    settings: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Default, Deserialize)]
struct JsonProviderSection {
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    cloudflare: Option<JsonProviderEntry>,
    #[serde(default)]
    xai: Option<JsonProviderEntry>,
    #[serde(default)]
    openai: Option<JsonProviderEntry>,
}

#[derive(Debug, Default, Deserialize)]
struct JsonProviderEntry {
    #[serde(default)]
    options: Option<JsonProviderOptions>,
    #[serde(default, rename = "accountId")]
    account_id: Option<String>,
    #[serde(default, rename = "apiKey")]
    api_key: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct JsonProviderOptions {
    #[serde(default, rename = "accountId")]
    account_id: Option<String>,
    #[serde(default, rename = "apiKey")]
    api_key: Option<String>,
    #[serde(default, rename = "baseURL")]
    base_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct JsonCloudflareBlock {
    #[serde(default, rename = "accountId")]
    account_id: Option<String>,
    #[serde(default, rename = "apiKey")]
    api_key: Option<String>,
}

// ── Unified intermediate used for merging ────────────────────────────────────

#[derive(Debug, Default)]
struct ParsedFile {
    model: Option<String>,
    small_model: Option<String>,
    yolo: Option<bool>,
    provider_default: Option<String>,
    cf_account_id: Option<String>,
    cf_api_key: Option<String>,
    cf_base_url: Option<String>,
    xai_api_key: Option<String>,
    xai_base_url: Option<String>,
    openai_api_key: Option<String>,
    openai_base_url: Option<String>,
    settings: HashMap<String, serde_json::Value>,
    path: Option<PathBuf>,
}

impl ParsedFile {
    fn from_toml(path: &Path) -> Option<Self> {
        let data = fs::read_to_string(path).ok()?;
        let t: TomlConfig = toml::from_str(&data).ok()?;

        let providers = t.provider.as_ref();
        let cf = providers.and_then(|p| p.cloudflare.as_ref());
        let xai = providers.and_then(|p| p.xai.as_ref());
        let openai = providers.and_then(|p| p.openai.as_ref());

        // Convert toml::Value settings to serde_json::Value
        let settings = t
            .settings
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| {
                let json_str = serde_json::to_string(&v).ok()?;
                let jv = serde_json::from_str(&json_str).ok()?;
                Some((k, jv))
            })
            .collect();

        Some(ParsedFile {
            model: t.model,
            small_model: t.small_model,
            yolo: t.yolo,
            provider_default: providers.and_then(|p| p.default.clone()),
            cf_account_id: cf.and_then(|c| c.account_id.clone()),
            cf_api_key: cf.and_then(|c| c.api_key.clone()),
            cf_base_url: cf.and_then(|c| c.base_url.clone()),
            xai_api_key: xai.and_then(|x| x.api_key.clone()),
            xai_base_url: xai.and_then(|x| x.base_url.clone()),
            openai_api_key: openai.and_then(|o| o.api_key.clone()),
            openai_base_url: openai.and_then(|o| o.base_url.clone()),
            settings,
            path: Some(path.to_path_buf()),
        })
    }

    fn from_json(path: &Path) -> Option<Self> {
        let data = fs::read_to_string(path).ok()?;
        let j: JsonConfig = serde_json::from_str(&data).ok()?;

        let providers = j.provider.as_ref();
        let cf_entry = providers.and_then(|p| p.cloudflare.as_ref());
        let xai_entry = providers.and_then(|p| p.xai.as_ref());
        let openai_entry = providers.and_then(|p| p.openai.as_ref());

        // Harvest CF account ID from multiple possible locations
        let cf_account_id = j
            .cloudflare_account_id
            .clone()
            .or_else(|| j.account_id.clone())
            .or_else(|| j.cloudflare.as_ref().and_then(|c| c.account_id.clone()))
            .or_else(|| {
                cf_entry.and_then(|c| {
                    c.options
                        .as_ref()
                        .and_then(|o| o.account_id.clone())
                        .or_else(|| c.account_id.clone())
                })
            });

        let cf_api_key = j
            .api_key_top
            .clone()
            .or_else(|| j.cloudflare_api_token.clone())
            .or_else(|| j.cloudflare.as_ref().and_then(|c| c.api_key.clone()))
            .or_else(|| {
                cf_entry.and_then(|c| {
                    c.options
                        .as_ref()
                        .and_then(|o| o.api_key.clone())
                        .or_else(|| c.api_key.clone())
                })
            });

        let cf_base_url = cf_entry
            .and_then(|c| c.options.as_ref())
            .and_then(|o| o.base_url.clone());

        let xai_api_key = xai_entry.and_then(|x| {
            x.options
                .as_ref()
                .and_then(|o| o.api_key.clone())
                .or_else(|| x.api_key.clone())
        });

        let xai_base_url = xai_entry
            .and_then(|x| x.options.as_ref())
            .and_then(|o| o.base_url.clone());

        let openai_api_key = openai_entry.and_then(|o| {
            o.options
                .as_ref()
                .and_then(|opts| opts.api_key.clone())
                .or_else(|| o.api_key.clone())
        });

        let openai_base_url = openai_entry
            .and_then(|o| o.options.as_ref())
            .and_then(|opts| opts.base_url.clone());

        Some(ParsedFile {
            model: j.model,
            small_model: j.small_model,
            yolo: None,
            provider_default: providers.and_then(|p| p.default.clone()),
            cf_account_id,
            cf_api_key,
            cf_base_url,
            xai_api_key,
            xai_base_url,
            openai_api_key,
            openai_base_url,
            settings: j.settings.unwrap_or_default(),
            path: Some(path.to_path_buf()),
        })
    }
}

/// Try to load a config file from a directory, preferring TOML (Codex-style).
/// Search order: `fusion.toml` → `fusion.json` → `zencode.json`.
fn try_load_from_dir(dir: &Path) -> Option<ParsedFile> {
    // TOML first (Codex-style, primary)
    let toml_path = dir.join("fusion.toml");
    if toml_path.exists() {
        if let Some(parsed) = ParsedFile::from_toml(&toml_path) {
            return Some(parsed);
        }
    }

    // JSON fallback
    for name in &["fusion.json", "zencode.json"] {
        let json_path = dir.join(name);
        if json_path.exists() {
            if let Some(parsed) = ParsedFile::from_json(&json_path) {
                return Some(parsed);
            }
        }
    }

    None
}

/// Load configuration with full precedence chain:
/// env vars (highest) > project config > global config > defaults.
///
/// Config file search (Codex-style TOML is primary):
///   - `fusion.toml` (preferred)
///   - `fusion.json` (fallback)
///   - `zencode.json` (backward compat)
pub fn load_config(cwd: &Path) -> Result<Config, FusionError> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    // 1. Global config (lowest file priority)
    let global_dirs = [
        home.join(".config").join("fusion"),
        home.join(".fusion"),
        home.join(".config").join("zencode"),
        home.join(".zencode"),
    ];
    let mut global_parsed = ParsedFile::default();
    for gd in &global_dirs {
        if let Some(p) = try_load_from_dir(gd) {
            global_parsed = p;
            break;
        }
    }

    // 2. Walk up from cwd to find project-level config
    let mut project_parsed = ParsedFile::default();
    let mut dir = cwd.to_path_buf();
    loop {
        if let Some(p) = try_load_from_dir(&dir) {
            project_parsed = p;
            break;
        }
        if !dir.pop() {
            break;
        }
    }

    // 3. Merge: project > global (project wins for each field)
    // Credential fields skip template placeholders like "YOUR_CLOUDFLARE_API_TOKEN"
    // so a project fusion.toml.example copy cannot override a real global key.
    let file_model = project_parsed.model.or(global_parsed.model);
    let file_small_model = project_parsed.small_model.or(global_parsed.small_model);
    let file_yolo = project_parsed.yolo.or(global_parsed.yolo);
    let file_provider_default = project_parsed
        .provider_default
        .or(global_parsed.provider_default);

    let cf_account_id = first_real_credential([
        env::var("CLOUDFLARE_ACCOUNT_ID").ok(),
        project_parsed.cf_account_id,
        global_parsed.cf_account_id,
    ]);

    let cf_token = first_real_credential([
        env::var("CLOUDFLARE_API_TOKEN").ok(),
        env::var("CLOUDFLARE_AI_TOKEN").ok(),
        project_parsed.cf_api_key,
        global_parsed.cf_api_key,
    ]);

    let xai_key = first_real_credential([
        env::var("XAI_API_KEY").ok(),
        project_parsed.xai_api_key,
        global_parsed.xai_api_key,
    ]);

    let openai_key = first_real_credential([
        env::var("OPENAI_API_KEY").ok(),
        project_parsed.openai_api_key,
        global_parsed.openai_api_key,
    ]);

    let generic_key = first_real_credential([
        env::var("FUSION_API_KEY").ok(),
        env::var("ZENCODE_API_KEY").ok(),
    ]);

    // Provider detection
    let env_provider = env::var("FUSION_PROVIDER")
        .or_else(|_| env::var("ZENCODE_PROVIDER"))
        .unwrap_or_default();
    let mut provider = match env_provider.to_lowercase().as_str() {
        "cloudflare" => Provider::Cloudflare,
        "xai" => Provider::Xai,
        "openai" => Provider::OpenAi,
        "faux" => Provider::Faux,
        _ => file_provider_default
            .as_deref()
            .map(|s| match s {
                "cloudflare" => Provider::Cloudflare,
                "xai" => Provider::Xai,
                "openai" => Provider::OpenAi,
                "faux" => Provider::Faux,
                _ => Provider::Auto,
            })
            .unwrap_or(Provider::Auto),
    };

    // Model
    let mut model = env::var("FUSION_MODEL")
        .or_else(|_| env::var("ZENCODE_MODEL"))
        .ok()
        .or(file_model)
        .unwrap_or_else(|| "@cf/moonshotai/kimi-k2.7-code".to_string());

    let small_model = env::var("FUSION_SMALL_MODEL")
        .or_else(|_| env::var("ZENCODE_SMALL_MODEL"))
        .ok()
        .or(file_small_model);

    // Auto-detect provider
    if provider == Provider::Auto {
        if !xai_key.is_empty() {
            provider = Provider::Xai;
        } else if !cf_account_id.is_empty() && !cf_token.is_empty() {
            provider = Provider::Cloudflare;
        } else if !openai_key.is_empty() {
            provider = Provider::OpenAi;
        } else {
            provider = Provider::Cloudflare;
        }
    }

    // Expand shorthand: "cloudflare/kimi" → "@cf/moonshotai/kimi-k2.7-code"
    model = expand_model_shorthand(&model);

    // Base URL
    let mut base_url = env::var("FUSION_BASE_URL")
        .or_else(|_| env::var("ZENCODE_BASE_URL"))
        .unwrap_or_default();

    if base_url.is_empty() {
        base_url = match provider {
            Provider::Xai => project_parsed
                .xai_base_url
                .or(global_parsed.xai_base_url)
                .or(project_parsed.cf_base_url)
                .or(global_parsed.cf_base_url)
                .unwrap_or_default(),
            Provider::OpenAi => project_parsed
                .openai_base_url
                .or(global_parsed.openai_base_url)
                .or(project_parsed.cf_base_url)
                .or(global_parsed.cf_base_url)
                .unwrap_or_default(),
            _ => project_parsed
                .cf_base_url
                .or(global_parsed.cf_base_url)
                .or(project_parsed.xai_base_url)
                .or(global_parsed.xai_base_url)
                .unwrap_or_default(),
        };
    }

    if provider == Provider::Xai && base_url.is_empty() {
        base_url = "https://api.x.ai/v1".to_string();
    }
    if base_url.is_empty() && provider != Provider::Cloudflare {
        base_url = "https://api.openai.com/v1".to_string();
    }

    // YOLO
    let yolo_env = env::var("FUSION_YOLO")
        .or_else(|_| env::var("ZENCODE_YOLO"))
        .unwrap_or_default();
    let yolo = yolo_env == "1" || yolo_env == "true" || file_yolo.unwrap_or(false);

    // Settings
    let mut settings = global_parsed.settings;
    settings.extend(project_parsed.settings);

    // Provider-aware key selection. Generic env keys (FUSION_API_KEY) still win
    // as an explicit override for any provider.
    let final_api_key = if !generic_key.is_empty() {
        generic_key
    } else {
        match provider {
            Provider::Xai => xai_key,
            Provider::OpenAi => openai_key,
            Provider::Cloudflare | Provider::Auto | Provider::Faux => cf_token,
        }
    };

    let config_path = project_parsed.path.or(global_parsed.path);

    Ok(Config {
        provider,
        model,
        small_model,
        api_key: final_api_key,
        base_url,
        cloudflare_account_id: if cf_account_id.is_empty() {
            None
        } else {
            Some(cf_account_id)
        },
        yolo,
        config_path,
        settings,
    })
}

/// True for empty strings and common config-template placeholders
/// (e.g. `YOUR_CLOUDFLARE_API_TOKEN`, `xai-YOUR_KEY`).
fn is_placeholder_value(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let upper = t.to_ascii_uppercase();
    upper.contains("YOUR_")
        || upper.contains("YOUR-")
        || upper.contains("<YOUR")
        || upper == "CHANGEME"
        || upper == "CHANGE_ME"
        || upper == "REPLACE_ME"
        || upper == "TODO"
        || upper == "XXX"
}

/// First non-placeholder credential in priority order.
fn first_real_credential<I>(candidates: I) -> String
where
    I: IntoIterator<Item = Option<String>>,
{
    candidates
        .into_iter()
        .flatten()
        .find(|v| !is_placeholder_value(v))
        .unwrap_or_default()
}

/// Save (upsert) an API key into the global config file at `~/.config/fusion/fusion.toml`.
/// Preserves all existing content — only updates or inserts the `api_key` field under
/// the matching `[provider.*]` section, and sets `[provider] default`.
///
/// For Cloudflare, pass `account_id` to also write `account_id` (required for Workers AI).
pub fn save_api_key(
    provider: Option<&str>,
    key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    save_provider_credentials(provider, key, None)
}

/// Save provider credentials (API key + optional Cloudflare account ID) to
/// `~/.config/fusion/fusion.toml`.
///
/// Creates `~/.config/fusion/` and `fusion.toml` if they do not exist yet.
pub fn save_provider_credentials(
    provider: Option<&str>,
    key: &str,
    account_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("cannot determine home directory")?;
    let config_dir = home.join(".config").join("fusion");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("fusion.toml");
    save_provider_credentials_to(&config_path, provider, key, account_id)
}

/// Write provider credentials to an explicit config path (used by tests and save).
///
/// Creates parent directories and the file when missing.
pub fn save_provider_credentials_to(
    config_path: &Path,
    provider: Option<&str>,
    key: &str,
    account_id: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Read existing content or start fresh (file may not exist yet)
    let existing = if config_path.exists() {
        fs::read_to_string(config_path)?
    } else {
        String::new()
    };

    let new_content = build_provider_credentials_toml(&existing, provider, key, account_id);
    fs::write(config_path, new_content)?;
    Ok(())
}

/// Build updated fusion.toml content from an existing file body (may be empty).
fn build_provider_credentials_toml(
    existing: &str,
    provider: Option<&str>,
    key: &str,
    account_id: Option<&str>,
) -> String {
    // Determine target section + default provider name
    let (section, default_name) = if let Some(p) = provider {
        match p.to_lowercase().as_str() {
            "xai" => ("[provider.xai]".to_string(), "xai"),
            "openai" => ("[provider.openai]".to_string(), "openai"),
            _ => ("[provider.cloudflare]".to_string(), "cloudflare"),
        }
    } else if key.starts_with("xai-") {
        ("[provider.xai]".to_string(), "xai")
    } else if key.starts_with("sk-") {
        ("[provider.openai]".to_string(), "openai")
    } else {
        ("[provider.cloudflare]".to_string(), "cloudflare")
    };

    let is_cf = section == "[provider.cloudflare]";
    let account_id = account_id
        .map(str::trim)
        .filter(|s| !s.is_empty() && !is_placeholder_value(s));

    let new_content = if existing.contains(&section) {
        // Replace the api_key (and account_id when provided) inside the existing section
        let mut result = String::new();
        let mut in_section = false;
        let mut key_written = false;
        let mut account_written = account_id.is_none(); // skip if not provided
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed == section {
                in_section = true;
                result.push_str(line);
                result.push('\n');
                continue;
            }
            if in_section {
                // Match active or commented account_id lines so we can uncomment/update them.
                let is_account_line = trimmed.starts_with("account_id")
                    || trimmed.starts_with("# account_id")
                    || trimmed.starts_with("#account_id");
                if is_account_line {
                    if let Some(id) = account_id {
                        result.push_str(&format!("account_id = \"{}\"\n", id));
                        account_written = true;
                    } else {
                        // Preserve existing account_id line when not updating it
                        result.push_str(line);
                        result.push('\n');
                    }
                    continue;
                }
                if trimmed.starts_with("api_key") {
                    result.push_str(&format!("api_key = \"{}\"\n", key));
                    key_written = true;
                    continue;
                }
                if trimmed.starts_with('[') && trimmed != section {
                    // Entering a new section — write missing fields first
                    if !account_written {
                        if let Some(id) = account_id {
                            result.push_str(&format!("account_id = \"{}\"\n", id));
                        }
                        account_written = true;
                    }
                    if !key_written {
                        result.push_str(&format!("api_key = \"{}\"\n", key));
                        key_written = true;
                    }
                    in_section = false;
                }
            }
            result.push_str(line);
            result.push('\n');
        }
        if in_section {
            if !account_written {
                if let Some(id) = account_id {
                    result.push_str(&format!("account_id = \"{}\"\n", id));
                }
            }
            if !key_written {
                result.push_str(&format!("api_key = \"{}\"\n", key));
            }
        }
        result
    } else {
        // Append the section (or create the whole file from empty)
        let mut result = existing.to_string();
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&section);
        result.push('\n');
        if is_cf {
            if let Some(id) = account_id {
                result.push_str(&format!("account_id = \"{}\"\n", id));
            } else {
                result.push_str("# account_id = \"your-cloudflare-account-id\"\n");
            }
        }
        result.push_str(&format!("api_key = \"{}\"\n", key));
        result
    };

    // Ensure `[provider] default = "..."` is set so the saved provider is used.
    upsert_provider_default(&new_content, default_name)
}

/// Insert or update `default = "<name>"` under the `[provider]` table.
fn upsert_provider_default(content: &str, default_name: &str) -> String {
    let mut result = String::new();
    let mut in_provider = false;
    let mut default_written = false;
    let mut saw_provider = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[provider]" {
            saw_provider = true;
            in_provider = true;
            result.push_str(line);
            result.push('\n');
            continue;
        }
        if in_provider {
            if trimmed.starts_with("default") {
                result.push_str(&format!("default = \"{}\"\n", default_name));
                default_written = true;
                continue;
            }
            // Nested tables like [provider.cloudflare] end the top-level [provider] section
            if trimmed.starts_with('[') {
                if !default_written {
                    result.push_str(&format!("default = \"{}\"\n", default_name));
                    default_written = true;
                }
                in_provider = false;
            }
        }
        result.push_str(line);
        result.push('\n');
    }

    if in_provider && !default_written {
        result.push_str(&format!("default = \"{}\"\n", default_name));
        default_written = true;
    }

    if !saw_provider {
        // Prepend a [provider] section so default is explicit.
        let mut with_provider = String::new();
        with_provider.push_str("[provider]\n");
        with_provider.push_str(&format!("default = \"{}\"\n\n", default_name));
        with_provider.push_str(&result);
        return with_provider;
    }

    let _ = default_written;
    result
}



/// Check if a model string refers to a Cloudflare Workers AI model.
pub fn is_cloudflare_model(model: &str) -> bool {
    model.starts_with("@cf/")
}

/// Detect if we're running inside Termux.
pub fn is_termux() -> bool {
    if env::var("TERMUX_VERSION").is_ok() {
        return true;
    }
    if let Ok(prefix) = env::var("PREFIX") {
        if prefix.contains("com.termux") {
            return true;
        }
    }
    false
}

/// Detect if we're running inside the iSH iOS emulator.
pub fn is_ish() -> bool {
    std::path::Path::new("/proc/ish").exists()
}

fn expand_model_shorthand(model: &str) -> String {
    if let Some(rest) = model.strip_prefix("cloudflare/") {
        let expanded = if rest.contains('/') {
            rest.to_string()
        } else {
            match rest {
                "kimi-k2.7-code" | "kimi-k2.7" | "kimi" => {
                    "moonshotai/kimi-k2.7-code".to_string()
                }
                "glm-4" | "glm-4-9b" => "zhipu-ai/glm-4".to_string(),
                "qwen2.5-coder" => "qwen/qwen2.5-coder-32b-instruct".to_string(),
                other => other.to_string(),
            }
        };
        format!("@cf/{}", expanded)
    } else {
        model.to_string()
    }
}

/// Load all available specialized skills from `.agents/skills/*/SKILL.md` (local)
/// and `~/.config/fusion/skills/*/SKILL.md` (global).
pub fn load_skills<P: AsRef<Path>>(cwd: P) -> Vec<(String, String)> {
    let mut skills = Vec::new();

    // 1. Local Workspace skills
    let workspace_skills = cwd.as_ref().join(".agents").join("skills");
    if workspace_skills.is_dir() {
        if let Ok(entries) = std::fs::read_dir(workspace_skills) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let skill_file = path.join("SKILL.md");
                    if skill_file.is_file() {
                        if let Ok(content) = std::fs::read_to_string(&skill_file) {
                            let name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string();
                            skills.push((name, content));
                        }
                    }
                }
            }
        }
    }

    // 2. Global user-level skills
    if let Some(home) = dirs::home_dir() {
        let global_skills = home.join(".config").join("fusion").join("skills");
        if global_skills.is_dir() {
            if let Ok(entries) = std::fs::read_dir(global_skills) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let skill_file = path.join("SKILL.md");
                        if skill_file.is_file() {
                            if let Ok(content) = std::fs::read_to_string(&skill_file) {
                                let name = path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                // Avoid overriding local skills with global ones if names clash
                                if !skills.iter().any(|(n, _)| n == &name) {
                                    skills.push((name, content));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    skills
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_expand_model_shorthand() {
        assert_eq!(
            expand_model_shorthand("cloudflare/kimi"),
            "@cf/moonshotai/kimi-k2.7-code"
        );
        assert_eq!(
            expand_model_shorthand("cloudflare/glm-4"),
            "@cf/zhipu-ai/glm-4"
        );
        assert_eq!(expand_model_shorthand("grok-3"), "grok-3");
    }

    #[test]
    fn test_is_cloudflare_model() {
        assert!(is_cloudflare_model("@cf/moonshotai/kimi-k2.7-code"));
        assert!(!is_cloudflare_model("grok-3"));
    }

    #[test]
    fn test_load_config_defaults() {
        let tmp = std::env::temp_dir().join("fusion-test-cfg-defaults");
        let _ = fs::create_dir_all(&tmp);

        env::remove_var("XAI_API_KEY");
        env::remove_var("CLOUDFLARE_ACCOUNT_ID");
        env::remove_var("FUSION_MODEL");
        env::remove_var("ZENCODE_MODEL");
        env::remove_var("FUSION_PROVIDER");
        env::remove_var("ZENCODE_PROVIDER");

        let cfg = load_config(&tmp).unwrap();
        assert_eq!(cfg.model, "@cf/moonshotai/kimi-k2.7-code");
        assert_eq!(cfg.provider, Provider::Cloudflare);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_load_from_toml() {
        let tmp = std::env::temp_dir().join("fusion-test-cfg-toml");
        let _ = fs::create_dir_all(&tmp);

        let toml_content = r#"
model = "grok-3"
yolo = true

[provider]
default = "xai"

[provider.xai]
api_key = "xai-test-123"
"#;
        fs::write(tmp.join("fusion.toml"), toml_content).unwrap();

        env::remove_var("XAI_API_KEY");
        env::remove_var("CLOUDFLARE_ACCOUNT_ID");
        env::remove_var("CLOUDFLARE_API_TOKEN");
        env::remove_var("CLOUDFLARE_AI_TOKEN");
        env::remove_var("FUSION_API_KEY");
        env::remove_var("ZENCODE_API_KEY");
        env::remove_var("FUSION_MODEL");
        env::remove_var("ZENCODE_MODEL");
        env::remove_var("FUSION_PROVIDER");
        env::remove_var("ZENCODE_PROVIDER");
        env::remove_var("FUSION_YOLO");
        env::remove_var("ZENCODE_YOLO");

        let cfg = load_config(&tmp).unwrap();
        assert_eq!(cfg.model, "grok-3");
        assert!(cfg.yolo);
        assert_eq!(cfg.provider, Provider::Xai);
        assert_eq!(cfg.api_key, "xai-test-123");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_placeholder_credentials_are_ignored() {
        assert!(is_placeholder_value("YOUR_CLOUDFLARE_API_TOKEN"));
        assert!(is_placeholder_value("YOUR_CLOUDFLARE_ACCOUNT_ID"));
        assert!(is_placeholder_value("xai-YOUR_KEY"));
        assert!(is_placeholder_value(""));
        assert!(!is_placeholder_value("cfat_real_token_value"));
        assert!(!is_placeholder_value("xai-abc123"));
    }

    #[test]
    fn test_first_real_credential_skips_placeholders() {
        assert_eq!(
            first_real_credential([
                Some("YOUR_CLOUDFLARE_API_TOKEN".into()),
                Some("cfat_real_token".into()),
            ]),
            "cfat_real_token"
        );
        assert_eq!(
            first_real_credential([
                Some("YOUR_CLOUDFLARE_ACCOUNT_ID".into()),
                None,
                Some("abc123account".into()),
            ]),
            "abc123account"
        );
        assert!(first_real_credential([
            Some("YOUR_CLOUDFLARE_API_TOKEN".into()),
            Some("xai-YOUR_KEY".into()),
            None,
        ])
        .is_empty());
    }

    #[test]
    fn test_project_placeholders_are_not_loaded_as_credentials() {
        // Project dir with only template placeholders. Global ~/.config may still
        // supply a real key — assert the placeholder strings themselves never win.
        let tmp = std::env::temp_dir().join("fusion-test-cfg-placeholders");
        let _ = fs::create_dir_all(&tmp);

        let toml_content = r#"
model = "@cf/moonshotai/kimi-k2.7-code"

[provider]
default = "cloudflare"

[provider.cloudflare]
account_id = "YOUR_CLOUDFLARE_ACCOUNT_ID"
api_key = "YOUR_CLOUDFLARE_API_TOKEN"
"#;
        fs::write(tmp.join("fusion.toml"), toml_content).unwrap();

        // Clear env so file credentials would be used if not filtered.
        env::remove_var("XAI_API_KEY");
        env::remove_var("CLOUDFLARE_ACCOUNT_ID");
        env::remove_var("CLOUDFLARE_API_TOKEN");
        env::remove_var("CLOUDFLARE_AI_TOKEN");
        env::remove_var("FUSION_API_KEY");
        env::remove_var("ZENCODE_API_KEY");
        env::remove_var("FUSION_MODEL");
        env::remove_var("ZENCODE_MODEL");
        env::remove_var("FUSION_PROVIDER");
        env::remove_var("ZENCODE_PROVIDER");

        let cfg = load_config(&tmp).unwrap();
        assert_ne!(cfg.api_key, "YOUR_CLOUDFLARE_API_TOKEN");
        assert!(!cfg.api_key.to_ascii_uppercase().contains("YOUR_"));
        if let Some(ref id) = cfg.cloudflare_account_id {
            assert_ne!(id, "YOUR_CLOUDFLARE_ACCOUNT_ID");
            assert!(!id.to_ascii_uppercase().contains("YOUR_"));
        }

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_upsert_provider_default() {
        let content = r#"[provider.cloudflare]
api_key = "cfat_test"
"#;
        let updated = upsert_provider_default(content, "cloudflare");
        assert!(updated.contains("[provider]"));
        assert!(updated.contains("default = \"cloudflare\""));
        assert!(updated.contains("api_key = \"cfat_test\""));

        let with_provider = r#"[provider]
default = "xai"

[provider.xai]
api_key = "xai-old"
"#;
        let updated2 = upsert_provider_default(with_provider, "cloudflare");
        assert!(updated2.contains("default = \"cloudflare\""));
        assert!(!updated2.contains("default = \"xai\""));
    }

    #[test]
    fn test_save_creates_toml_when_missing() {
        let tmp = std::env::temp_dir().join(format!(
            "fusion-test-save-toml-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("fusion.toml");

        // File must not exist yet
        assert!(!path.exists());

        save_provider_credentials_to(
            &path,
            Some("cloudflare"),
            "cfat_new_token_value",
            Some("acctid0123456789abcdef01234567"),
        )
        .unwrap();

        assert!(path.exists(), "save must create fusion.toml when missing");
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("[provider]"));
        assert!(content.contains("default = \"cloudflare\""));
        assert!(content.contains("[provider.cloudflare]"));
        assert!(content.contains("api_key = \"cfat_new_token_value\""));
        assert!(content.contains("account_id = \"acctid0123456789abcdef01234567\""));

        // Upsert on second save must update in place, not lose the file
        save_provider_credentials_to(
            &path,
            Some("cloudflare"),
            "cfat_updated_token",
            Some("acctid0123456789abcdef01234567"),
        )
        .unwrap();
        let content2 = fs::read_to_string(&path).unwrap();
        assert!(content2.contains("api_key = \"cfat_updated_token\""));
        assert!(!content2.contains("cfat_new_token_value"));

        // xAI create-from-empty also works
        let path_xai = tmp.join("xai.toml");
        save_provider_credentials_to(&path_xai, Some("xai"), "xai-abc123", None).unwrap();
        let xai = fs::read_to_string(&path_xai).unwrap();
        assert!(xai.contains("[provider.xai]"));
        assert!(xai.contains("default = \"xai\""));
        assert!(xai.contains("api_key = \"xai-abc123\""));
        assert!(!xai.contains("account_id"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_build_credentials_toml_from_empty() {
        let content =
            build_provider_credentials_toml("", Some("cloudflare"), "cfat_k", Some("acct_1"));
        assert!(content.starts_with("[provider]\n"));
        assert!(content.contains("default = \"cloudflare\""));
        assert!(content.contains("[provider.cloudflare]"));
        assert!(content.contains("account_id = \"acct_1\""));
        assert!(content.contains("api_key = \"cfat_k\""));
    }
}
