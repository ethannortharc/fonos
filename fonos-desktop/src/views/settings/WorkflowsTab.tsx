// Workflows tab (Workflow P1, Task 15) — lists/edits pipelines that wire one
// source widget through an ordered processor chain to one or more outputs.
//
// Preset (built-in) workflows are editable but never deletable, so they show no
// delete button. Each row carries an inline HotkeyInput bound directly to the
// workflow: changing it calls saveWorkflow immediately, which makes the backend
// re-validate the chain and emit hotkey:reload. The editor's Save also goes
// through saveWorkflow; the backend is the final validator and any Err (invalid
// chain) is surfaced verbatim in red.

import { useState, useEffect } from "react";
import { t, useT } from "../../lib/i18n";
import type { WidgetDef, WorkflowDef, WorkflowRow } from "../../types";
import { listWorkflows, listWidgets, saveWorkflow, deleteWorkflow } from "../../lib/api";
import { HotkeyInput } from "./HotkeysTab";

// ─── Shared class recipes (match WidgetsTab) ──────────────────────────────────

const inputClass =
  "w-full bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]";
const selectClass = inputClass + " cursor-pointer appearance-none";
const labelClass = "text-[10px] text-[rgba(255,255,255,0.35)]";
const headingClass =
  "text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] font-semibold";

// ─── Editing form model ────────────────────────────────────────────────────────

interface WorkflowForm {
  id: string;
  name: string;
  icon: string;
  hotkey: string;
  source: string;
  processors: string[];
  outputs: string[];
  builtin: boolean;
  /** New (unsaved) workflow → shows the "new" title and no delete button. */
  isNew: boolean;
}

function rowToForm(w: WorkflowRow): WorkflowForm {
  return {
    id: w.id, name: w.name, icon: w.icon ?? "", hotkey: w.hotkey ?? "",
    source: w.source, processors: [...(w.processors ?? [])], outputs: [...w.outputs],
    builtin: !!w.builtin, isNew: false,
  };
}

/** Strip the WorkflowRow-only `source_type_tag` before handing to saveWorkflow,
 *  which takes a bare WorkflowDef. */
function rowToDef(w: WorkflowRow): WorkflowDef {
  const { source_type_tag: _drop, ...def } = w;
  return def;
}

// ─── Small building blocks ─────────────────────────────────────────────────────

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <label className={labelClass}>{label}</label>
      {children}
    </div>
  );
}

/** Pipeline summary — source → processors → outputs as widget names, rendered
 *  with a leading dot per step and a muted arrow between (the pipeline dot/line
 *  visual language, laid out inline). */
function PipelineSummary({ names }: { names: string[] }) {
  if (names.length === 0) return null;
  return (
    <div className="flex items-center flex-wrap gap-x-1.5 gap-y-0.5 text-[9px] text-[rgba(255,255,255,0.3)]">
      {names.map((n, i) => (
        <span key={i} className="inline-flex items-center gap-1.5">
          {i > 0 && <span className="text-[rgba(255,255,255,0.15)]">{"─▸"}</span>}
          <span className="inline-flex items-center gap-1">
            <span className="w-1 h-1 rounded-full bg-[rgba(255,255,255,0.2)] flex-shrink-0" />
            {n}
          </span>
        </span>
      ))}
    </div>
  );
}

// ─── Workflow row (list) ───────────────────────────────────────────────────────

function WorkflowRowCard({
  wf, names, onHotkey, onEdit, onDelete,
}: {
  wf: WorkflowRow;
  names: string[];
  onHotkey: (v: string) => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)] transition-colors flex items-center gap-2.5 px-3.5 py-2.5">
      <span className="flex-shrink-0 text-[15px] leading-none">{wf.icon || "⚙️"}</span>
      <div className="flex-1 min-w-0 flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <span className="text-[#fafaf9] text-[12px] font-medium truncate">{wf.name}</span>
          {wf.builtin && (
            <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded flex-shrink-0">{t("common.builtin")}</span>
          )}
        </div>
        <PipelineSummary names={names} />
      </div>
      {/* Inline hotkey — changing it saves the workflow (backend emits hotkey:reload). */}
      <HotkeyInput value={wf.hotkey ?? ""} onChange={onHotkey} />
      <button
        onClick={onEdit}
        className="text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)] text-[10px] px-1.5 transition-colors flex-shrink-0"
      >
        {t("common.edit")}
      </button>
      {/* Preset (built-in) workflows can't be deleted — hide the button. */}
      {!wf.builtin && (
        <button
          onClick={onDelete}
          className="text-[rgba(255,255,255,0.12)] hover:text-[#ef4444] text-[10px] px-1 transition-colors flex-shrink-0"
          title={t("common.delete")}
        >{"✕"}</button>
      )}
    </div>
  );
}

// ─── Main WorkflowsTab ─────────────────────────────────────────────────────────

export default function WorkflowsTab() {
  useT();
  const [workflows, setWorkflows] = useState<WorkflowRow[]>([]);
  const [widgets, setWidgets] = useState<WidgetDef[]>([]);
  const [editing, setEditing] = useState<WorkflowForm | null>(null);
  const [error, setError] = useState<string>("");

  const load = async () => {
    try {
      const [wfs, wgs] = await Promise.all([listWorkflows(), listWidgets()]);
      setWorkflows(wfs);
      setWidgets(wgs);
    } catch (e) {
      console.error("list_workflows/list_widgets:", e);
    }
  };

  useEffect(() => { load(); }, []);

  const widgetById = (id: string): WidgetDef | undefined => widgets.find((w) => w.id === id);
  const widgetName = (id: string): string => widgetById(id)?.name ?? id;
  const sourceWidgets = widgets.filter((w) => w.role === "source");
  const processorWidgets = widgets.filter((w) => w.role === "processor");
  const outputWidgets = widgets.filter((w) => w.role === "output");

  /** source → processors → outputs, as widget names, for the row summary. */
  const pipelineNames = (wf: WorkflowRow): string[] => [
    widgetName(wf.source),
    ...(wf.processors ?? []).map(widgetName),
    ...wf.outputs.map(widgetName),
  ];

  // ── Inline row hotkey: save immediately so the backend reloads triggers ──────
  const onRowHotkey = async (row: WorkflowRow, hotkey: string) => {
    setError("");
    try {
      await saveWorkflow({ ...rowToDef(row), hotkey });
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const openNew = () => {
    setError("");
    setEditing({
      id: `wf.custom-${Date.now()}`,
      name: "", icon: "⚙️", hotkey: "",
      source: "src.selection", processors: [], outputs: ["out.panel"],
      builtin: false, isNew: true,
    });
  };

  const openEdit = (w: WorkflowRow) => { setError(""); setEditing(rowToForm(w)); };

  const handleDeleteRow = async (w: WorkflowRow) => {
    setError("");
    try {
      await deleteWorkflow(w.id);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleSave = async () => {
    if (!editing) return;
    if (!editing.name.trim()) { setError(t("wf.err.name-required")); return; }
    if (!editing.source) { setError(t("wf.err.source-required")); return; }
    if (editing.outputs.length === 0) { setError(t("wf.err.outputs-required")); return; }
    setError("");
    try {
      await saveWorkflow({
        id: editing.id.trim(),
        name: editing.name.trim(),
        icon: editing.icon,
        hotkey: editing.hotkey,
        source: editing.source,
        processors: editing.processors,
        outputs: editing.outputs,
        builtin: editing.builtin,
      });
      setEditing(null);
      await load();
    } catch (e) {
      // Backend rejects an invalid chain with a descriptive message — show it.
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDeleteEditing = async () => {
    if (!editing) return;
    setError("");
    try {
      await deleteWorkflow(editing.id);
      setEditing(null);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  // ── Editor (inline, replaces the list — matches WidgetsTab) ───────────────────
  if (editing) {
    const setProc = (i: number, id: string) =>
      setEditing({ ...editing, processors: editing.processors.map((p, k) => (k === i ? id : p)) });
    const moveProc = (i: number, dir: -1 | 1) => {
      const j = i + dir;
      if (j < 0 || j >= editing.processors.length) return;
      const next = [...editing.processors];
      [next[i], next[j]] = [next[j], next[i]];
      setEditing({ ...editing, processors: next });
    };
    const removeProc = (i: number) =>
      setEditing({ ...editing, processors: editing.processors.filter((_, k) => k !== i) });
    const addProc = () => {
      const first = processorWidgets[0]?.id;
      if (!first) return;
      setEditing({ ...editing, processors: [...editing.processors, first] });
    };
    const toggleOutput = (id: string) =>
      setEditing({
        ...editing,
        outputs: editing.outputs.includes(id)
          ? editing.outputs.filter((x) => x !== id)
          : [...editing.outputs, id],
      });

    // Frontend pre-check: a microphone source must start with an stt processor.
    // (Final say belongs to the backend on save; this is just an early hint.)
    const sourceW = widgetById(editing.source);
    const firstProcW = editing.processors[0] ? widgetById(editing.processors[0]) : undefined;
    const micNeedsStt = sourceW?.type_tag === "microphone" && firstProcW?.type_tag !== "stt";

    return (
      <div className="flex flex-col gap-3">
        <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-4">
          <div className="text-[12px] font-medium text-[#fafaf9]">
            {editing.isNew ? t("wf.editor.new") : t("wf.editor.edit")}
          </div>

          {error && <div className="text-[11px] text-[#ef4444] leading-relaxed">{error}</div>}

          {/* Header: icon + name, hotkey */}
          <div className="flex flex-col gap-2">
            <div className="grid grid-cols-[48px_1fr] gap-2">
              <input
                type="text"
                value={editing.icon}
                onChange={(e) => setEditing({ ...editing, icon: e.target.value })}
                title={t("wf.field.icon")}
                className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2 py-2 text-center text-[16px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
              />
              <input
                type="text"
                value={editing.name}
                onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                placeholder={t("wf.ph.name")}
                className={inputClass}
              />
            </div>
            <div className="flex items-center gap-2">
              <span className={labelClass}>{t("wf.field.hotkey")}</span>
              <HotkeyInput value={editing.hotkey} onChange={(v) => setEditing({ ...editing, hotkey: v })} />
            </div>
          </div>

          {/* Source (single-select) */}
          <Field label={t("wf.field.source")}>
            <select
              value={editing.source}
              onChange={(e) => setEditing({ ...editing, source: e.target.value })}
              className={selectClass}
            >
              {!sourceWidgets.some((w) => w.id === editing.source) && (
                <option value={editing.source}>{editing.source}</option>
              )}
              {sourceWidgets.map((w) => (
                <option key={w.id} value={w.id}>{w.name}</option>
              ))}
            </select>
          </Field>

          {/* Processors (ordered) */}
          <div className="flex flex-col gap-2">
            <div className={headingClass}>{t("wf.field.processors")}</div>
            {editing.processors.map((pid, i) => (
              <div key={i} className="flex items-center gap-1.5">
                <select
                  value={pid}
                  onChange={(e) => setProc(i, e.target.value)}
                  className={selectClass}
                >
                  {!processorWidgets.some((w) => w.id === pid) && (
                    <option value={pid}>{pid}</option>
                  )}
                  {processorWidgets.map((w) => (
                    <option key={w.id} value={w.id}>{w.name}</option>
                  ))}
                </select>
                <button
                  onClick={() => moveProc(i, -1)}
                  disabled={i === 0}
                  title={t("wf.step.up")}
                  className="px-1.5 py-1 text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)] disabled:opacity-20 transition-colors"
                >{"↑"}</button>
                <button
                  onClick={() => moveProc(i, 1)}
                  disabled={i === editing.processors.length - 1}
                  title={t("wf.step.down")}
                  className="px-1.5 py-1 text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)] disabled:opacity-20 transition-colors"
                >{"↓"}</button>
                <button
                  onClick={() => removeProc(i)}
                  title={t("wf.step.remove")}
                  className="px-1.5 py-1 text-[rgba(255,255,255,0.15)] hover:text-[#ef4444] transition-colors"
                >{"✕"}</button>
              </div>
            ))}
            {micNeedsStt && (
              <div className="text-[10px] text-[#ef4444] leading-relaxed">{t("wf.hint.mic-needs-stt")}</div>
            )}
            {processorWidgets.length > 0 && (
              <button
                onClick={addProc}
                className="self-start text-[11px] text-[rgba(245,158,11,0.7)] hover:text-[#fbbf24] px-1 py-0.5 transition-colors"
              >
                {t("wf.add-step")}
              </button>
            )}
          </div>

          {/* Outputs (multi-select chips, ≥1 required) */}
          <div className="flex flex-col gap-2">
            <div className={headingClass}>{t("wf.field.outputs")}</div>
            <div className="flex flex-wrap gap-1.5">
              {outputWidgets.map((w) => {
                const on = editing.outputs.includes(w.id);
                return (
                  <button
                    key={w.id}
                    onClick={() => toggleOutput(w.id)}
                    className={[
                      "px-2.5 py-1 rounded-full text-[10px] transition-all",
                      on
                        ? "bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.3)] text-[#fbbf24]"
                        : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] hover:border-[rgba(255,255,255,0.12)]",
                    ].join(" ")}
                  >
                    {w.icon ? `${w.icon} ` : ""}{w.name}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Actions */}
          <div className="flex gap-2 pt-1 items-center">
            <button
              onClick={handleSave}
              className="flex-1 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-[#1a1917] text-[12px] font-semibold hover:opacity-90 transition-opacity"
            >
              {t("common.save")}
            </button>
            <button
              onClick={() => { setEditing(null); setError(""); }}
              className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
            >
              {t("common.cancel")}
            </button>
            {/* Preset workflows can't be deleted — hide the button entirely. */}
            {!editing.isNew && !editing.builtin && (
              <button
                onClick={handleDeleteEditing}
                className="px-3 py-2 rounded-lg bg-transparent border border-[rgba(239,68,68,0.1)] text-[rgba(239,68,68,0.6)] text-[12px] hover:text-[#ef4444] hover:border-[rgba(239,68,68,0.3)] transition-colors"
              >
                {t("common.delete")}
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }

  // ── List: Preset + Custom sections ────────────────────────────────────────────
  const presets = workflows.filter((w) => w.builtin);
  const customs = workflows.filter((w) => !w.builtin);

  return (
    <div className="flex flex-col gap-5">
      {error && <div className="text-[11px] text-[#ef4444] leading-relaxed">{error}</div>}

      {/* Preset (built-in) */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <span className={headingClass}>{t("wf.section.preset")}</span>
          <span className="text-[9px] text-[rgba(255,255,255,0.15)]">({presets.length})</span>
        </div>
        {presets.map((w) => (
          <WorkflowRowCard
            key={w.id}
            wf={w}
            names={pipelineNames(w)}
            onHotkey={(v) => onRowHotkey(w, v)}
            onEdit={() => openEdit(w)}
            onDelete={() => handleDeleteRow(w)}
          />
        ))}
      </div>

      {/* Custom */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <span className={headingClass}>{t("wf.section.custom")}</span>
          <span className="text-[9px] text-[rgba(255,255,255,0.15)]">({customs.length})</span>
        </div>
        {customs.length === 0 && (
          <div className="text-[11px] text-[rgba(255,255,255,0.25)] italic py-1">{t("wf.empty.custom")}</div>
        )}
        {customs.map((w) => (
          <WorkflowRowCard
            key={w.id}
            wf={w}
            names={pipelineNames(w)}
            onHotkey={(v) => onRowHotkey(w, v)}
            onEdit={() => openEdit(w)}
            onDelete={() => handleDeleteRow(w)}
          />
        ))}
        <button
          onClick={openNew}
          className="w-full py-2 rounded-[10px] border border-dashed border-[rgba(245,158,11,0.12)] text-[rgba(251,191,36,0.6)] text-[12px] hover:border-[rgba(245,158,11,0.25)] transition-colors"
        >
          {t("wf.new")}
        </button>
      </div>
    </div>
  );
}
