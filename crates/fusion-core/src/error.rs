use thiserror::Error;

/// Central error type for Fusion.
#[derive(Error, Debug)]
pub enum FusionError {
    #[error("config error: {0}")]
    Config(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("tool error ({tool}): {message}")]
    Tool { tool: String, message: String },

    #[error("search_replace error: {0}")]
    SearchReplace(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

pub type FusionResult<T> = Result<T, FusionError>;
