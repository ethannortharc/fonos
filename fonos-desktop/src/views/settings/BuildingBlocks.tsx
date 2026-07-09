// BuildingBlocks.tsx — read-only reference catalog of the widget library
// (Sources / Processors / Outputs). Widget creating/editing/deleting no longer
// lives here: all of that now happens in FlowsTab's in-place node editor (it
// calls the global saveWidget and supports minting a new widget straight into
// a flow slot). This surface is a glanceable catalog only — clicking a card
// opens the shared WidgetForm in read-only mode (every field disabled,
// Close-only footer) as a detail view, delegating per-type_tag rendering to
// the same one form implementation FlowsTab uses.
//
// Cards flow in a responsive wrap grid within each role section, and the
// sections stack in role order (Sources → Processors → Outputs). TYPE_TAGS is
// exported unchanged for FlowsTab's slot picker to consume.

import { useState, useEffect } from "react";
import { t, useT } from "../../lib/i18n";
import type { TKey } from "../../lib/i18n";
import type { AppConfig, WidgetDef, WidgetRole } from "../../types";
import { listWidgets } from "../../lib/api";
import { listContainers } from "../../lib/storage-api";
import type { Container } from "../../lib/storage-api";
import { WidgetIcon, roleColor } from "../../components/WidgetIcon";
import { widgetLabel } from "../../lib/builtinLabels";
import WidgetForm, { widgetToForm } from "./WidgetForm";
import type { WidgetFormValue } from "./WidgetForm";

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

// ─── Widget card (list row) ────────────────────────────────────────────────────

function WidgetCard({ w, onClick }: { w: WidgetDef; onClick: () => void }) {
  const rc = roleColor(w.role);
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onClick(); }
      }}
      className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)] transition-colors cursor-pointer flex items-center gap-2.5 px-3.5 py-2.5"
    >
      <span
        className="flex-shrink-0 w-6 h-6 flex items-center justify-center rounded-md"
        style={{
          background: `rgba(${rc.rgb},0.08)`,
          border: `1px solid rgba(${rc.rgb},0.22)`,
          color: `rgba(${rc.rgb},0.95)`,
        }}
      >
        <WidgetIcon typeTag={w.type_tag} size={13} />
      </span>
      <span className="flex-1 min-w-0 text-[#fafaf9] text-[12px] font-medium truncate" title={widgetLabel(w)}>{widgetLabel(w)}</span>
      <span className="text-[9px] text-[rgba(255,255,255,0.3)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded font-mono flex-shrink-0">{w.type_tag}</span>
      {w.builtin && (
        <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded flex-shrink-0">{t("common.builtin")}</span>
      )}
    </div>
  );
}

// ─── Main BuildingBlocks ───────────────────────────────────────────────────────

export default function BuildingBlocks({ config }: { config: AppConfig }) {
  useT();
  const [widgets, setWidgets] = useState<WidgetDef[]>([]);
  const [containers, setContainers] = useState<Container[]>([]);
  const [editing, setEditing] = useState<WidgetFormValue | null>(null);

  const load = async () => {
    try {
      setWidgets(await listWidgets());
    } catch (e) {
      console.error("list_widgets:", e);
    }
  };

  useEffect(() => { load(); }, []);
  useEffect(() => {
    listContainers().then(setContainers).catch(() => { /* no backend / ignore */ });
  }, []);

  // ── Detail view (shared WidgetForm, read-only) ────────────────────────────
  if (editing) {
    return (
      <div className="flex flex-col gap-3">
        <WidgetForm
          value={editing}
          config={config}
          containers={containers}
          readOnly
          onCancel={() => setEditing(null)}
        />
      </div>
    );
  }

  // ── Catalog: stacked role sections, cards in a responsive wrap grid ────────
  // Each section is its heading row followed by an auto-fill grid so cards flow
  // multiple-per-row by available width (replacing the old fixed 3-column
  // side-by-side layout). Sections stack in role order (ROLES).
  return (
    <div className="flex flex-col gap-5">
      {ROLES.map(({ role, label }) => {
        const items = widgets.filter((w) => w.role === role);
        const rc = roleColor(role);
        return (
          <div key={role} className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <span className={headingClass} style={{ color: `rgba(${rc.rgb},0.75)` }}>{t(label)}</span>
              <span className="text-[9px] text-[rgba(255,255,255,0.15)]">({items.length})</span>
            </div>
            <div className="grid gap-2.5 grid-cols-[repeat(auto-fill,minmax(210px,1fr))]">
              {items.map((w) => (
                <WidgetCard key={w.id} w={w} onClick={() => setEditing(widgetToForm(w))} />
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}
