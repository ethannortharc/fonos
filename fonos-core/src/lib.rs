//! Fonos Core — platform-independent business logic for the Fonos voice assistant.
//!
//! This crate contains all business logic that is shared across platforms:
//! configuration, modes, stats, LLM clients, voice management, and audio utilities.
//! It has zero dependency on Tauri or any macOS-specific frameworks.

#![deny(missing_docs)]

pub mod agent;
pub mod audio;
pub mod config;
pub mod error;
pub mod llm;
pub mod modes;
pub mod model_caps;
pub mod stats;
pub mod voice_store;

pub use error::Error;

/// Convenience type alias for Results using the fonos-core error type.
pub type Result<T> = std::result::Result<T, Error>;
