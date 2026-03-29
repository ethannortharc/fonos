//! Meeting mode — continuous recording, chunking, session management, and export.
//!
//! Sub-modules:
//! - [`chunker`]: splits a continuous PCM stream into fixed-length audio chunks
//! - [`audio`]: stereo interleave/split helpers and WAV encoding
//! - [`session`]: meeting session container creation and entry insertion
//! - [`summary`]: LLM summary prompt construction
//! - [`openrouter`]: OpenRouter provider configuration
//! - [`export`]: Markdown and JSON export

pub mod audio;
pub mod chunker;
pub mod export;
pub mod openrouter;
pub mod session;
pub mod summary;
