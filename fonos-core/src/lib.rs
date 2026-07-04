//! Fonos Core — platform-independent business logic for the Fonos voice assistant.
//!
//! This crate contains all business logic that is shared across platforms:
//! configuration, modes, stats, LLM clients, voice management, and audio utilities.
//! It has zero dependency on Tauri or any macOS-specific frameworks.

#![deny(missing_docs)]

pub mod agent;
pub mod audio;
pub mod config;
pub mod doctor;
pub mod error;
pub mod hotkey;
pub mod listen;
pub mod llm;
pub mod meetings;
pub mod error_class;
pub mod modes;
pub mod pipeline;
pub mod model_caps;
pub mod services;
pub mod stats;
pub mod stt;
pub mod storage;
pub mod sts;
pub mod tts;
pub mod vocab;
pub mod voice_store;

pub use error::Error;

/// Convenience type alias for Results using the fonos-core error type.
pub type Result<T> = std::result::Result<T, Error>;
