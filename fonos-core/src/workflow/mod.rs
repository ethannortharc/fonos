//! Generic Source→Processor→Output workflow engine (Workflow P1).
//!
//! Replaces the fixed dictation/note/listen/text-actions pipelines with a
//! data-driven component model: [`model::WidgetDef`] instances (source,
//! processor, output) are wired into a [`model::WorkflowDef`] and executed by
//! a linear engine. This module is platform-independent; adapters live in
//! `fonos-desktop`.
//!
//! Submodules land incrementally across the Workflow P1 task series:
//! `engine` (linear runner), `builtin` (built-in widget/workflow definitions),
//! and `migrate` (one-time migration of legacy config into workflow defs).
//!
//! ## Adding a new workflow component
//!
//! A new Source/Processor/Output needs no changes to this engine or its data
//! model — see `fonos-desktop/src-tauri/src/commands/widget_uppercase.rs` for
//! a worked, test-covered example. Touch points:
//!
//! 1. Implement [`registry::Source`], [`registry::Processor`], or
//!    [`registry::Output`] in a new adapter file (platform crate, e.g.
//!    `fonos-desktop/src-tauri/src/commands/widget_<name>.rs`).
//! 2. Register its `type_tag` in `build_registry`
//!    (`fonos-desktop/src-tauri/src/commands/workflow_widgets.rs`).
//! 3. Add a `case "<type_tag>":` to the `PropsForm` switch in
//!    `fonos-desktop/src/views/settings/WidgetsTab.tsx` so its props are editable.
//! 4. If it should be user-creatable (not just editable once referenced), add
//!    the `type_tag` to `WidgetsTab.tsx`'s `TYPE_TAGS` map.
//! 5. Add a test — ideally an end-to-end run through [`engine::run`], as in
//!    `fonos-desktop/src-tauri/tests/uppercase_acceptance.rs`.

pub mod builtin;
pub mod engine;
pub mod llm_step;
pub mod migrate;
pub mod model;
pub mod registry;
