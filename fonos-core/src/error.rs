//! Unified error type for fonos-core.

use serde::{Deserialize, Serialize};

/// All errors returned by fonos-core functions.
#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
pub enum Error {
    /// Configuration error (load, save, parse).
    #[error("config: {0}")]
    Config(String),

    /// Database error (SQLite operations).
    #[error("db: {0}")]
    Database(String),

    /// LLM API error (HTTP call, parse, provider-specific).
    #[error("llm: {0}")]
    Llm(String),

    /// Mode error (not found, invalid template).
    #[error("mode: {0}")]
    Mode(String),

    /// Voice store error (clone, delete, load).
    #[error("voice: {0}")]
    Voice(String),

    /// Audio error (capture, playback, WAV encoding).
    #[error("audio: {0}")]
    Audio(String),

    /// HTTP / network error.
    #[error("http: {0}")]
    Http(String),

    /// Generic I/O error.
    #[error("io: {0}")]
    Io(String),

    /// Agent error (skill not found, skill disabled, execution failure).
    #[error("agent: {0}")]
    Agent(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Config(e.to_string())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Database(e.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e.to_string())
    }
}
