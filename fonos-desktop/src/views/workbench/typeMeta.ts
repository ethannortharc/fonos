// typeMeta.ts — the widget type vocabulary shared by the Workbench Widgets
// section and RecipesSection's slot pickers: which type_tags each role can
// instantiate, the shelf groups in display order, and each type_tag's
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
//
// Workbench P2 (Task 5): the "output" role now spans two presentation
// shelves — delivery (insert/replace/clipboard/notebook/speak/panel) and
// sessions (dialog/call/agent/meeting) — see GROUPS below. TYPE_TAGS stays
// role-keyed (not shelf-keyed) because it's the semantic source consumed by
// pickers/validation (RecipesSection's new-widget picker, WidgetForm's
// isNew type_tag <select>): those only ever need to know "what can this
// role instantiate", never which shelf a tag renders on. All four session
// composites (dialog/agent/meeting/call) now have registered backend
// factories (T4/T6/T7/T9), so every tag here is creatable.
export const TYPE_TAGS: Record<WidgetRole, string[]> = {
  source: ["microphone", "selection", "instant"],
  processor: ["stt", "llm"],
  output: ["insert", "replace", "clipboard", "notebook", "speak", "panel", "dialog", "call", "agent", "meeting"],
};

// Props that hold references to other widget instances, per type_tag —
// mirrors fonos-core's widget_ref_props(type_tag) (workflow/model.rs).
// Workbench P2's composite widgets (dialog/call/agent/meeting, built in
// T4/T6-T9) embed a capability widget's id directly as a string prop value
// instead of instantiating their own — e.g. a "call" widget's stt_widget prop
// names the "stt"-type widget it delegates to. usageCount (lib/triggers.ts)
// reads this table to count a widget still embedded inside a composite as
// "in use", even though no workflow's source/processors/outputs names it
// directly (pierced usage). All four composites now have PropsForm cases +
// registered factories (dialog T4, agent T6, meeting T7, call T9), so every
// row here is live; it mirrors fonos-core's widget_ref_props so the two
// sides (Rust/TS) can't drift.
export const WIDGET_REF_PROPS: Record<string, string[]> = {
  dialog: ["llm_widget"],
  call: ["stt_widget", "llm_widget"],
  agent: ["llm_widget"],
  meeting: ["stt_widget", "llm_widget"],
};

// The four widget shelves, in display order. Splits the "output" role into
// two presentation groups (delivery/sessions) while keeping `role` alongside
// each group — WidgetsSection still needs `role` for tinting (roleColor) and
// for the type_tag it hands new widgets (a "call" widget is still, at the
// model layer, an Output). `tags` here is a display-order slice of
// TYPE_TAGS[role] (sessions cherry-picks dialog/call/agent/meeting out of
// output's full list; delivery gets the rest) rather than a derived
// computation, so shelf order can diverge from TYPE_TAGS's role-internal
// order without the two fighting each other.
export const GROUPS: { key: "sources" | "processors" | "delivery" | "sessions"; role: WidgetRole; label: TKey; tags: string[] }[] = [
  { key: "sources", role: "source", label: "widgets.section.sources", tags: ["microphone", "selection", "instant"] },
  { key: "processors", role: "processor", label: "widgets.section.processors", tags: ["stt", "llm"] },
  { key: "delivery", role: "output", label: "widgets.section.delivery", tags: ["insert", "replace", "clipboard", "notebook", "speak", "panel"] },
  { key: "sessions", role: "output", label: "widgets.section.sessions", tags: ["dialog", "call", "agent", "meeting"] },
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
  call: { name: "widgets.type.call.name", desc: "widgets.type.call.desc" },
  agent: { name: "widgets.type.agent.name", desc: "widgets.type.agent.desc" },
  meeting: { name: "widgets.type.meeting.name", desc: "widgets.type.meeting.desc" },
};
