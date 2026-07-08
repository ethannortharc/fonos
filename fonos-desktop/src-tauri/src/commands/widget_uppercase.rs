//! Workflow P1 Task 16 — extensibility acceptance fixture.
//!
//! This file exists to *prove* the workflow engine's extensibility claim: a
//! brand-new component can be added by touching only (1) this file, (2) one
//! registration line in `workflow_widgets.rs`'s `build_registry`, (3) one
//! form-mapping entry in `WidgetsTab.tsx`, and (4) a test — with zero changes
//! to the engine, the data model, the `Registry`/`Processor` trait, or any
//! other component adapter. [`UppercaseProcessor`] is deliberately trivial
//! (Text → Text, `.to_uppercase()`) so the diff proves the extension story
//! rather than any interesting behavior.
//!
//! See `tests/uppercase_acceptance.rs` for the end-to-end proof that this
//! type runs through the real [`fonos_core::workflow::engine::run`].

use fonos_core::workflow::model::{Data, DataKind};
use fonos_core::workflow::registry::{Processor, RunCtx};

/// Trivial Text → Text processor: uppercases its input via `.to_uppercase()`.
/// Its only purpose is Task 16's extensibility acceptance test — see the
/// module doc comment above.
pub struct UppercaseProcessor;

#[async_trait::async_trait]
impl Processor for UppercaseProcessor {
    fn input_kind(&self) -> DataKind {
        DataKind::Text
    }

    fn output_kind(&self) -> DataKind {
        DataKind::Text
    }

    async fn process(&self, input: Data, _ctx: &RunCtx) -> Result<Data, String> {
        Ok(Data::Text(input.into_text()?.to_uppercase()))
    }
}
