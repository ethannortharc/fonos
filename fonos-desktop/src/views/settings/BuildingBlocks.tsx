// BuildingBlocks.tsx — the "Components" catalog: one descriptive card per
// widget TYPE (type_tag), grouped into the three role sections
// (Sources / Processors / Outputs). It documents the component vocabulary of
// the workflow engine — the kinds of building block a flow can be made of —
// rather than the concrete widget INSTANCES a user has configured (those are
// created and edited inside FlowsTab's in-place node editor).
//
// Each card shows the type's role-colored icon, its localized name, a
// two-line description, and a count of how many configured instances of that
// type currently exist. Cards are informational only (no click / detail view).
// Sections stack in role order (Sources → Processors → Outputs), cards flow in
// a responsive wrap grid. TYPE_TAGS is exported unchanged for FlowsTab's slot
// picker to consume.

import { useState, useEffect } from "react";
import { t, useT } from "../../lib/i18n";
import type { TKey } from "../../lib/i18n";
import type { WidgetDef, WidgetRole } from "../../types";
import { listWidgets } from "../../lib/api";
import { WidgetIcon, roleColor } from "../../components/WidgetIcon";

// ─── Shared class recipes (canonical: constants.ts; match WidgetForm/WorkflowsTab) ──

const headingClass =
  "text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] font-semibold";

// The type_tags each role can instantiate — mirrors the desktop registry
// (workflow_widgets.rs build_registry). v1 hardcoded map, ported from
// WidgetsTab.tsx. Exported so other WidgetForm callers (e.g. FlowsTab's
// in-place node editor, Task 4) can share one source of truth for the
// allowed-types picker instead of duplicating this map.
export const TYPE_TAGS: Record<WidgetRole, string[]> = {
  source: ["microphone", "selection"],
  processor: ["stt", "llm"],
  output: ["insert", "replace", "clipboard", "notebook", "speak", "panel", "dialog"],
};

const ROLES: { role: WidgetRole; label: TKey }[] = [
  { role: "source", label: "widgets.section.sources" },
  { role: "processor", label: "widgets.section.processors" },
  { role: "output", label: "widgets.section.outputs" },
];

// type_tag → its localized name/description i18n keys. A static typed map (no
// dynamic key construction) so every reference stays TKey-checked. Covers every
// tag across all three TYPE_TAGS role lists.
const TYPE_META: Record<string, { name: TKey; desc: TKey }> = {
  microphone: { name: "widgets.type.microphone.name", desc: "widgets.type.microphone.desc" },
  selection: { name: "widgets.type.selection.name", desc: "widgets.type.selection.desc" },
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

// ─── Type card (informational — one per type_tag) ───────────────────────────────

function TypeCard({ role, tag, count }: { role: WidgetRole; tag: string; count: number }) {
  const rc = roleColor(role);
  const meta = TYPE_META[tag];
  const name = meta ? t(meta.name) : tag;
  return (
    <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] flex flex-col gap-2 px-3.5 py-3">
      <div className="flex items-center gap-2.5">
        <span
          className="flex-shrink-0 w-6 h-6 flex items-center justify-center rounded-md"
          style={{
            background: `rgba(${rc.rgb},0.08)`,
            border: `1px solid rgba(${rc.rgb},0.22)`,
            color: `rgba(${rc.rgb},0.95)`,
          }}
        >
          <WidgetIcon typeTag={tag} size={13} />
        </span>
        <span className="flex-1 min-w-0 text-[#fafaf9] text-[12px] font-medium truncate" title={name}>{name}</span>
        <span className="text-[9px] text-[rgba(255,255,255,0.15)] flex-shrink-0">({count})</span>
      </div>
      {meta && (
        <p className="m-0 text-[11px] leading-[1.5] text-[rgba(255,255,255,0.42)] line-clamp-2">{t(meta.desc)}</p>
      )}
    </div>
  );
}

// ─── Main BuildingBlocks ───────────────────────────────────────────────────────

export default function BuildingBlocks() {
  useT();
  const [widgets, setWidgets] = useState<WidgetDef[]>([]);

  const load = async () => {
    try {
      setWidgets(await listWidgets());
    } catch (e) {
      console.error("list_widgets:", e);
    }
  };

  useEffect(() => { load(); }, []);

  // ── Catalog: stacked role sections, one card per type_tag in a wrap grid ───
  // Each section is its heading row (role-colored, from ab2549f) followed by an
  // auto-fill grid of type cards. The per-card count chip reports how many
  // configured instances of that type currently exist. Sections stack in role
  // order (ROLES).
  return (
    <div className="flex flex-col gap-5">
      {ROLES.map(({ role, label }) => {
        const tags = TYPE_TAGS[role];
        const rc = roleColor(role);
        return (
          <div key={role} className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <span className={headingClass} style={{ color: `rgba(${rc.rgb},0.75)` }}>{t(label)}</span>
              <span className="text-[9px] text-[rgba(255,255,255,0.15)]">({tags.length})</span>
            </div>
            <div className="grid gap-2.5 grid-cols-[repeat(auto-fill,minmax(240px,1fr))]">
              {tags.map((tag) => (
                <TypeCard
                  key={tag}
                  role={role}
                  tag={tag}
                  count={widgets.filter((w) => w.type_tag === tag).length}
                />
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}
