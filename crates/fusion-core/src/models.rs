/// Model catalog — known models with metadata (context size, max_tokens tiers, etc.)

use serde::{Deserialize, Serialize};

/// Token output level — controls max_tokens sent to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenLevel {
    Normal, // model default
    High,   // extended output
    Max,    // maximum available
}

impl Default for TokenLevel {
    fn default() -> Self {
        TokenLevel::Normal
    }
}

impl std::fmt::Display for TokenLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenLevel::Normal => write!(f, "normal"),
            TokenLevel::High => write!(f, "high"),
            TokenLevel::Max => write!(f, "max"),
        }
    }
}

/// Metadata about a known model.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub shorthand: &'static str,
    pub full_id: &'static str,
    pub display_name: &'static str,
    pub context_window: u32,
    pub max_tokens_normal: Option<u32>,
    pub max_tokens_high: Option<u32>,
    pub max_tokens_max: Option<u32>,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
    pub category: &'static str, // "coding", "general", "fast"
}

impl ModelInfo {
    /// Get max_tokens for a given level.
    pub fn max_tokens_for(&self, level: TokenLevel) -> Option<u32> {
        match level {
            TokenLevel::Normal => self.max_tokens_normal,
            TokenLevel::High => self.max_tokens_high,
            TokenLevel::Max => self.max_tokens_max,
        }
    }

    /// Whether this model supports the given token level.
    pub fn supports_level(&self, level: TokenLevel) -> bool {
        self.max_tokens_for(level).is_some()
    }
}

/// All known Cloudflare Workers AI models.
pub static CLOUDFLARE_MODELS: &[ModelInfo] = &[
    // ── Coding models ────────────────────────────────────────────────────────
    ModelInfo {
        shorthand: "kimi",
        full_id: "@cf/moonshotai/kimi-k2.7-code",
        display_name: "Kimi K2.7 Code",
        context_window: 131072,
        max_tokens_normal: Some(4096),
        max_tokens_high: Some(16384),
        max_tokens_max: Some(32768),
        supports_tools: true,
        supports_reasoning: true,
        category: "coding",
    },
    ModelInfo {
        shorthand: "glm",
        full_id: "@cf/zai-org/glm-4.7-flash",
        display_name: "GLM 4.7 Flash",
        context_window: 131072,
        max_tokens_normal: Some(4096),
        max_tokens_high: Some(8192),
        max_tokens_max: Some(16384),
        supports_tools: true,
        supports_reasoning: false,
        category: "coding",
    },
    ModelInfo {
        shorthand: "glm5",
        full_id: "@cf/zai-org/glm-5.2",
        display_name: "GLM 5.2 Coder",
        context_window: 262144,
        max_tokens_normal: Some(4096),
        max_tokens_high: Some(16384),
        max_tokens_max: Some(32768),
        supports_tools: true,
        supports_reasoning: true,
        category: "coding",
    },
    ModelInfo {
        shorthand: "qwen3",
        full_id: "@cf/qwen/qwen3-30b-a3b-fp8",
        display_name: "Qwen3 30B-A3B",
        context_window: 32768,
        max_tokens_normal: Some(4096),
        max_tokens_high: Some(8192),
        max_tokens_max: Some(16384),
        supports_tools: true,
        supports_reasoning: true,
        category: "coding",
    },
    ModelInfo {
        shorthand: "qwen-coder",
        full_id: "@cf/qwen/qwen2.5-coder-32b-instruct",
        display_name: "Qwen 2.5 Coder 32B",
        context_window: 32768,
        max_tokens_normal: Some(4096),
        max_tokens_high: Some(8192),
        max_tokens_max: Some(16384),
        supports_tools: true,
        supports_reasoning: false,
        category: "coding",
    },
    ModelInfo {
        shorthand: "gemma4",
        full_id: "@cf/google/gemma-3-27b-it",
        display_name: "Gemma 3 27B",
        context_window: 131072,
        max_tokens_normal: Some(4096),
        max_tokens_high: Some(8192),
        max_tokens_max: Some(16384),
        supports_tools: true,
        supports_reasoning: false,
        category: "general",
    },
    // ── General / fast models ────────────────────────────────────────────────────────────
    ModelInfo {
        shorthand: "llama3",
        full_id: "@cf/meta/llama-3.3-70b-instruct-fp8-fast",
        display_name: "Llama 3.3 70B",
        context_window: 8192,
        max_tokens_normal: Some(2048),
        max_tokens_high: Some(4096),
        max_tokens_max: None,
        supports_tools: true,
        supports_reasoning: false,
        category: "general",
    },
    ModelInfo {
        shorthand: "deepseek",
        full_id: "@cf/deepseek-ai/deepseek-r1-0528-qwen3-8b",
        display_name: "DeepSeek R1 8B",
        context_window: 16384,
        max_tokens_normal: Some(4096),
        max_tokens_high: Some(8192),
        max_tokens_max: Some(16384),
        supports_tools: false,
        supports_reasoning: true,
        category: "coding",
    },
    // ── Non-Cloudflare well-known models (no shorthand expansion needed) ────
    ModelInfo {
        shorthand: "grok-3",
        full_id: "grok-3",
        display_name: "Grok 3",
        context_window: 131072,
        max_tokens_normal: Some(4096),
        max_tokens_high: Some(16384),
        max_tokens_max: Some(32768),
        supports_tools: true,
        supports_reasoning: true,
        category: "coding",
    },
    ModelInfo {
        shorthand: "grok-3-mini",
        full_id: "grok-3-mini",
        display_name: "Grok 3 Mini",
        context_window: 131072,
        max_tokens_normal: Some(4096),
        max_tokens_high: Some(16384),
        max_tokens_max: Some(32768),
        supports_tools: true,
        supports_reasoning: true,
        category: "fast",
    },
];

/// Look up a model by shorthand, full ID, or partial match.
pub fn lookup_model(query: &str) -> Option<&'static ModelInfo> {
    let q = query.to_lowercase();

    // Exact shorthand match
    if let Some(m) = CLOUDFLARE_MODELS.iter().find(|m| m.shorthand == q) {
        return Some(m);
    }

    // Exact full ID match
    if let Some(m) = CLOUDFLARE_MODELS.iter().find(|m| m.full_id == query) {
        return Some(m);
    }

    // Partial match on display name or shorthand
    CLOUDFLARE_MODELS
        .iter()
        .find(|m| m.display_name.to_lowercase().contains(&q) || m.shorthand.contains(&q))
}

/// List all models, optionally filtered by category.
pub fn list_models(category: Option<&str>) -> Vec<&'static ModelInfo> {
    match category {
        Some(cat) => CLOUDFLARE_MODELS
            .iter()
            .filter(|m| m.category == cat)
            .collect(),
        None => CLOUDFLARE_MODELS.iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_by_shorthand() {
        let m = lookup_model("kimi").unwrap();
        assert_eq!(m.full_id, "@cf/moonshotai/kimi-k2.7-code");
    }

    #[test]
    fn test_lookup_by_full_id() {
        let m = lookup_model("@cf/zai-org/glm-4.7-flash").unwrap();
        assert_eq!(m.shorthand, "glm");
    }

    #[test]
    fn test_token_levels() {
        let m = lookup_model("kimi").unwrap();
        assert_eq!(m.max_tokens_for(TokenLevel::Normal), Some(4096));
        assert_eq!(m.max_tokens_for(TokenLevel::High), Some(16384));
        assert_eq!(m.max_tokens_for(TokenLevel::Max), Some(32768));
    }

    #[test]
    fn test_list_coding_models() {
        let coding = list_models(Some("coding"));
        assert!(coding.len() >= 4);
    }
}
