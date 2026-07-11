// typeMeta.ts — the widget type vocabulary shared by the Workbench Widgets
// section and RecipesSection's slot pickers: which type_tags each role can
// instantiate, the three role sections in display order, and each type_tag's
// localized name/description keys. Moved here (unchanged) from the retired
// BuildingBlocks.tsx catalog (Task 9) — this is now the single source of
// truth for the component vocabulary of the workflow engine.

import type { TKey } from "../../lib/i18n";
import type { WidgetRole } from "../../types";

// The type_tags each role can instantiate — mirrors the desktop registry
// (workflow_widgets.rs build_registry). v1 hardcoded map, ported from
// WidgetsTab.tsx. Exported so other WidgetForm callers (e.g. RecipesSection's
// in-place node editor, the Workbench Widgets section) can share one source
// of truth for the allowed-types picker instead of duplicating this map.
export const TYPE_TAGS: Record<WidgetRole, string[]> = {
  source: ["microphone", "selection", "instant"],
  processor: ["stt", "llm"],
  output: ["insert", "replace", "clipboard", "notebook", "speak", "panel", "dialog"],
};

// Props that hold references to other widget instances, per type_tag —
// mirrors fonos-core's widget_ref_props(type_tag) (workflow/model.rs).
// Workbench P2's composite widgets (dialog/call/agent/meeting, built in
// T4/T6-T9) embed a capability widget's id directly as a string prop value
// instead of instantiating their own — e.g. a "call" widget's stt_widget prop
// names the "stt"-type widget it delegates to. usageCount (lib/triggers.ts)
// reads this table to count a widget still embedded inside a composite as
// "in use", even though no workflow's source/processors/outputs names it
// directly (pierced usage). None of dialog/call/agent/meeting is in
// TYPE_TAGS yet — no widget of these types can be created via the form until
// their PropsForm cases land — so this table currently has no effect on the
// running app; it exists now so the two sides (Rust/TS) can't drift once
// they do.
export const WIDGET_REF_PROPS: Record<string, string[]> = {
  dialog: ["llm_widget"],
  call: ["stt_widget", "llm_widget"],
  agent: ["llm_widget"],
  meeting: ["stt_widget", "llm_widget"],
};

export const ROLES: { role: WidgetRole; label: TKey }[] = [
  { role: "source", label: "widgets.section.sources" },
  { role: "processor", label: "widgets.section.processors" },
  { role: "output", label: "widgets.section.outputs" },
];

// type_tag → its localized name/description i18n keys. A static typed map (no
// dynamic key construction) so every reference stays TKey-checked. Covers every
// tag across all three TYPE_TAGS role lists.
export const TYPE_META: Record<string, { name: TKey; desc: TKey }> = {
  microphone: { name: "widgets.type.microphone.name", desc: "widgets.type.microphone.desc" },
  selection: { name: "widgets.type.selection.name", desc: "widgets.type.selection.desc" },
  instant: { name: "widgets.type.instant.name", desc: "widgets.type.instant.desc" },
  stt: { name: "widgets.type.stt.name", desc: "widgets.type.stt.desc" },
  llm: { name: "widgets.type.llm.name", desc: "widgets.type.llm.desc" },
  insert: { name: "widgets.type.insert.name", desc: "widgets.type.insert.desc" },
  replace: { name: "widgets.type.replace.name", desc: "widgets.type.replace.desc" },
  clipboard: { name: "widgets.type.clipboard.name", desc: "widgets.type.clipboard.desc" },
  notebook: { name: "widgets.type.notebook.name", desc: "widgets.type.notebook.desc" },
  speak: { name: "widgets.type.speak.name", desc: "widgets.type.speak.desc" },
  panel: { name: "widgets.type.panel.name", desc: "widgets.type.panel.desc" },
  dialog: { name: "widgets.type.dialog.name", desc: "widgets.type.dialog.desc" },
};
