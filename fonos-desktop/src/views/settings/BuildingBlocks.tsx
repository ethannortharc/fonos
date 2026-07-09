// BuildingBlocks.tsx — three-column widget library (Inputs / Processors /
// Outputs) that supersedes WidgetsTab (Flows UI redesign, Task 3). Ports
// WidgetsTab's list/template-copy/delete-referrer shell verbatim, but
// delegates the per-type_tag property editor to the shared `WidgetForm`
// (Task 2) instead of a local PropsForm copy — the same component FlowsTab's
// in-place node editor (Task 4) is expected to use, so there is exactly one
// form implementation for both surfaces.
//
// Built-in widgets are editable (saveWidget overrides them by id) but never
// deletable. Custom widgets can be deleted; the backend rejects deletion of
// a widget still referenced by a workflow and returns the referrer list,
// which is surfaced verbatim in red below the editor.

import { useState, useEffect } from "react";
import { t, useT } from "../../lib/i18n";
import type { TKey } from "../../lib/i18n";
import type { AppConfig, WidgetDef, WidgetRole } from "../../types";
import { listWidgets, saveWidget, deleteWidget } from "../../lib/api";
import { listContainers } from "../../lib/storage-api";
import type { Container } from "../../lib/storage-api";
import { WidgetIcon, roleColor } from "../../components/WidgetIcon";
import { widgetLabel } from "../../lib/builtinLabels";
import WidgetForm, { widgetToForm } from "./WidgetForm";
import type { WidgetFormValue } from "./WidgetForm";

// ─── Shared class recipes (match WidgetForm/WorkflowsTab) ─────────────────────

const selectClass =
  "w-full bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] cursor-pointer appearance-none";
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
      <span className="flex-1 min-w-0 text-[#fafaf9] text-[12px] font-medium truncate">{widgetLabel(w)}</span>
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
  const [deleteErr, setDeleteErr] = useState<string>("");

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

  const openNew = (role: WidgetRole) => {
    setDeleteErr("");
    const type_tag = TYPE_TAGS[role][0];
    setEditing({
      id: `${type_tag}.custom-${Date.now()}`,
      role, type_tag, name: "", icon: "", props: {}, builtin: false, isNew: true,
    });
  };

  // Clone a builtin llm processor's props into a new custom processor.
  const openTemplate = (w: WidgetDef) => {
    setDeleteErr("");
    setEditing({
      id: `llm.custom-${Date.now()}`,
      role: "processor", type_tag: "llm",
      name: `${widgetLabel(w)}${t("widgets.copy-suffix")}`, icon: w.icon ?? "",
      props: { ...(w.props ?? {}) }, builtin: false, isNew: true,
    });
  };

  const openEdit = (w: WidgetDef) => {
    setDeleteErr("");
    setEditing(widgetToForm(w));
  };

  const handleSave = async (w: WidgetDef) => {
    // Validation + error surfacing happen inside WidgetForm itself — a
    // rejected saveWidget() here propagates back up through its onSave
    // await and is shown inline there. This just persists and reloads.
    await saveWidget(w);
    setEditing(null);
    await load();
  };

  const handleDelete = async () => {
    if (!editing) return;
    setDeleteErr("");
    try {
      await deleteWidget(editing.id);
      setEditing(null);
      await load();
    } catch (e) {
      // Backend rejects referenced widgets with the referrer list in the message.
      setDeleteErr(e instanceof Error ? e.message : String(e));
    }
  };

  // ── Editor (shared WidgetForm) ────────────────────────────────────────────
  if (editing) {
    return (
      <div className="flex flex-col gap-3">
        <WidgetForm
          value={editing}
          config={config}
          containers={containers}
          typeTags={TYPE_TAGS[editing.role]}
          onSave={handleSave}
          onCancel={() => { setEditing(null); setDeleteErr(""); }}
          onDelete={editing.builtin ? undefined : handleDelete}
          deleteError={deleteErr}
        />
      </div>
    );
  }

  // ── List: three role columns ────────────────────────────────────────────
  // Side-by-side grid (Inputs / Processors / Outputs), matching the mockup's
  // `.blocks{grid-template-columns:1fr 1fr 1fr;gap:14px}` — collapses to one
  // column on narrow widths, same breakpoint convention as Scenarios.tsx.
  return (
    <div className="grid grid-cols-1 sm:grid-cols-3 gap-3.5 items-start">
      {ROLES.map(({ role, label }) => {
        const items = widgets.filter((w) => w.role === role);
        const llmTemplates = widgets.filter((w) => w.role === "processor" && w.type_tag === "llm" && w.builtin);
        const rc = roleColor(role);
        return (
          <div key={role} className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <span className={headingClass} style={{ color: `rgba(${rc.rgb},0.75)` }}>{t(label)}</span>
              <span className="text-[9px] text-[rgba(255,255,255,0.15)]">({items.length})</span>
            </div>

            {items.map((w) => (
              <WidgetCard key={w.id} w={w} onClick={() => openEdit(w)} />
            ))}

            {/* Processors: copy an LLM template into a new custom processor. */}
            {role === "processor" && llmTemplates.length > 0 && (
              <select
                value=""
                onChange={(e) => {
                  const w = llmTemplates.find((x) => x.id === e.target.value);
                  if (w) openTemplate(w);
                }}
                className={selectClass + " text-[rgba(251,191,36,0.7)]"}
              >
                <option value="">{t("widgets.copy-template")}</option>
                {llmTemplates.map((w) => (
                  <option key={w.id} value={w.id}>{widgetLabel(w)}</option>
                ))}
              </select>
            )}

            <button
              onClick={() => openNew(role)}
              className="w-full py-2 rounded-[10px] border border-dashed border-[rgba(245,158,11,0.12)] text-[rgba(251,191,36,0.6)] text-[11px] hover:border-[rgba(245,158,11,0.25)] transition-colors"
            >
              {t("widgets.new")}
            </button>
          </div>
        );
      })}
    </div>
  );
}
