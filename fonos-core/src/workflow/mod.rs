//! Generic Source→Processor→Output workflow engine (Workflow P1).
//!
//! Replaces the fixed dictation/note/listen/text-actions pipelines with a
//! data-driven component model: [`model::WidgetDef`] instances (source,
//! processor, output) are wired into a [`model::WorkflowDef`] and executed by
//! a linear engine. This module is platform-independent; adapters live in
//! `fonos-desktop`.
//!
//! Submodules land incrementally across the Workflow P1 task series:
//! `registry` (component traits + factories), `engine` (linear runner),
//! `builtin` (built-in widget/workflow definitions), and `migrate`
//! (one-time migration of legacy config into workflow defs) are not yet
//! present — this task only introduces the data model.

pub mod model;
