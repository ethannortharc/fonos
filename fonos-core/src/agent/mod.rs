//! Agent module — multi-step reasoning with skill execution.
//!
//! This module contains the conversation context, skill definitions,
//! skill registry, command safety filtering, fast-path matching,
//! and the agent processor that orchestrates them all.

pub mod context;
pub mod custom_loader;
pub mod fast_path;
pub mod processor;
pub mod registry;
pub mod safety;
pub mod skill;
