use thiserror::Error;

#[derive(Error, Debug)]
pub enum OlympusError {
    #[error("HTTP error: {status} {message}")]
    Api { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Authentication expired")]
    AuthExpired,
}

pub type Result<T> = std::result::Result<T, OlympusError>;
