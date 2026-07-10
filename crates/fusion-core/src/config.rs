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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    xai_api_key: Option<String>,
    xai_base_url: Option<String>,
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
    let file_model = project_parsed.model.or(global_parsed.model);
    let file_small_model = project_parsed.small_model.or(global_parsed.small_model);
    let file_yolo = project_parsed.yolo.or(global_parsed.yolo);
    let file_provider_default = project_parsed
        .provider_default
        .or(global_parsed.provider_default);

    let mut cf_account_id = env::var("CLOUDFLARE_ACCOUNT_ID").unwrap_or_default();
    if cf_account_id.is_empty() {
        cf_account_id = project_parsed
            .cf_account_id
            .or(global_parsed.cf_account_id)
            .unwrap_or_default();
    }

    let mut cf_token = env::var("CLOUDFLARE_API_TOKEN")
        .or_else(|_| env::var("CLOUDFLARE_AI_TOKEN"))
        .unwrap_or_default();
    if cf_token.is_empty() {
        cf_token = project_parsed
            .cf_api_key
            .or(global_parsed.cf_api_key)
            .unwrap_or_default();
    }

    let xai_key = env::var("XAI_API_KEY").unwrap_or_default();

    let generic_key = env::var("FUSION_API_KEY")
        .or_else(|_| env::var("ZENCODE_API_KEY"))
        .unwrap_or_else(|_| {
            if !xai_key.is_empty() {
                xai_key.clone()
            } else {
                cf_token.clone()
            }
        });

    // Provider detection
    let env_provider = env::var("FUSION_PROVIDER")
        .or_else(|_| env::var("ZENCODE_PROVIDER"))
        .unwrap_or_default();
    let mut provider = match env_provider.to_lowercase().as_str() {
        "cloudflare" => Provider::Cloudflare,
        "xai" => Provider::Xai,
        "openai" => Provider::OpenAi,
        _ => file_provider_default
            .as_deref()
            .map(|s| match s {
                "cloudflare" => Provider::Cloudflare,
                "xai" => Provider::Xai,
                "openai" => Provider::OpenAi,
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
        } else if !cf_account_id.is_empty() && (!cf_token.is_empty() || !generic_key.is_empty()) {
            provider = Provider::Cloudflare;
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
        base_url = project_parsed
            .cf_base_url
            .or(project_parsed.xai_base_url)
            .or(global_parsed.cf_base_url)
            .or(global_parsed.xai_base_url)
            .unwrap_or_default();
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

    let final_api_key = if generic_key.is_empty() {
        cf_token.clone()
    } else {
        generic_key
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

/// Save (upsert) an API key into the global config file at `~/.config/fusion/fusion.toml`.
/// Preserves all existing content — only updates or inserts the `api_key` field under
/// `[provider.cloudflare]` (and `[provider.xai]` if key looks like an xAI key).
pub fn save_api_key(key: &str) -> Result<(), Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("cannot determine home directory")?;
    let config_dir = home.join(".config").join("fusion");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("fusion.toml");

    // Read existing content or start fresh
    let existing = if config_path.exists() {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };

    // Determine which provider the key belongs to based on prefix heuristics
    let is_xai = key.starts_with("xai-");
    let is_cf   = key.starts_with("cfat_") || key.starts_with("cf_");

    // Parse as TOML document using raw string manipulation to preserve formatting.
    // Strategy: if the relevant section already exists, replace the api_key line.
    // Otherwise, append the whole section.
    let section = if is_xai { "[provider.xai]" } else { "[provider.cloudflare]" };

    let new_content = if existing.contains(section) {
        // Replace the api_key inside the existing section
        let mut result = String::new();
        let mut in_section = false;
        let mut key_written = false;
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed == section {
                in_section = true;
                result.push_str(line);
                result.push('\n');
                continue;
            }
            if in_section && trimmed.starts_with("api_key") {
                result.push_str(&format!("api_key = \"{}\"\n", key));
                key_written = true;
                continue;
            }
            if in_section && trimmed.starts_with('[') && trimmed != section {
                // Entering a new section — write key if not yet written
                if !key_written {
                    result.push_str(&format!("api_key = \"{}\"\n", key));
                    key_written = true;
                }
                in_section = false;
            }
            result.push_str(line);
            result.push('\n');
        }
        if in_section && !key_written {
            result.push_str(&format!("api_key = \"{}\"\n", key));
        }
        result
    } else {
        // Append the section
        let mut result = existing.clone();
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push('\n');
        result.push_str(section);
        result.push('\n');
        // Add account_id placeholder for Cloudflare
        if is_cf || (!is_xai && !is_cf) {
            result.push_str("# account_id = \"your-cloudflare-account-id\"\n");
        }
        result.push_str(&format!("api_key = \"{}\"\n", key));
        result
    };

    fs::write(&config_path, new_content)?;
    Ok(())
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
        env::remove_var("FUSION_MODEL");
        env::remove_var("ZENCODE_MODEL");
        env::remove_var("FUSION_PROVIDER");
        env::remove_var("ZENCODE_PROVIDER");
        env::remove_var("FUSION_YOLO");
        env::remove_var("ZENCODE_YOLO");

        let cfg = load_config(&tmp).unwrap();
        assert_eq!(cfg.model, "grok-3");
        assert!(cfg.yolo);

        let _ = fs::remove_dir_all(&tmp);
    }
}
