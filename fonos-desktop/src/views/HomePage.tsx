// HomePage — 主页/Home (Workbench P1, Task 15 → Task 16 revision). Two
// things live here: the floating pill's own card (its global hotkey +
// capture mode + which recipe it currently follows, plus a plain-text
// preview of the pill roller's order) and the trigger table — now the
// SINGLE place every recipe's triggers get edited ("the Workbench builds
// what, Home pulls the trigger"). Recipe cards in the Workbench show the
// same chips read-only with a hint pointing back here. Data model is
// unchanged: triggers[] still lives on WorkflowDef and still saves through
// save_workflow — only where the editing UI lives has moved.
// Design intent (per spec §3b): this page grows into an at-a-glance
// dashboard over time.

import { useEffect, useState } from "react";
import { useT } from "../lib/i18n";
import { useAppConfig } from "../lib/useAppConfig";
import { listWorkflows, listWidgets, saveWorkflow } from "../lib/api";
import type { Trigger, WidgetDef, WorkflowRow } from "../types";
import { workflowLabel } from "../lib/builtinLabels";
import { HotkeyInput } from "../components/HotkeyInput";
import { selectClass } from "./settings/constants";
import { WidgetIcon, roleColor } from "../components/WidgetIcon";
import TriggerChips from "../components/TriggerChips";
import { pillWorkflows } from "../lib/triggers";
import { rowToDef } from "./workbench/RecipesSection";

const captureSelectClass = selectClass.replace("w-full ", "") + " w-auto";

export default function HomePage({ onJumpToRecipe }: { onJumpToRecipe: (id: string) => void }) {
  const t = useT();
  const { config, save } = useAppConfig();
  const [rows, setRows] = useState<WorkflowRow[]>([]);
  const [widgets, setWidgets] = useState<WidgetDef[]>([]);
  // In-flight guard for trigger-chip saves (moved from RecipesSection, Task
  // 16): holds the id of the workflow currently persisting a chip edit, so a
  // second rapid edit on the same row can't fire before the first save's
  // reload lands (which would otherwise compute its patch from stale
  // wf.triggers).
  const [chipsSaving, setChipsSaving] = useState<string | null>(null);

  const load = () => {
    listWorkflows().then(setRows).catch(() => { /* no backend / ignore */ });
    listWidgets().then(setWidgets).catch(() => { /* no backend / ignore */ });
  };
  useEffect(load, []);

  if (!config) return null;

  // Follow-row resolution: active_voice_workflow's row if it matches one,
  // else wf.dictation's row, else the raw id (dangling active_voice_workflow,
  // or workflows not loaded yet).
  const activeId = config.active_voice_workflow;
  const activeRow = activeId ? rows.find((r) => r.id === activeId) : undefined;
  const dictationRow = rows.find((r) => r.id === "wf.dictation");
  const currentName = activeRow
    ? workflowLabel(activeRow)
    : dictationRow
      ? workflowLabel(dictationRow)
      : activeId || "wf.dictation";

  const widgetById = (id: string): WidgetDef | undefined => widgets.find((w) => w.id === id);
  const flowIcon = (wf: WorkflowRow) => {
    const src = widgetById(wf.source);
    const rc = roleColor("source");
    return (
      <span
        className="w-[26px] h-[26px] rounded-[7px] flex items-center justify-center flex-shrink-0"
        style={{ background: `rgba(${rc.rgb},0.12)`, color: `rgba(${rc.rgb},0.95)` }}
      >
        <WidgetIcon typeTag={src?.type_tag ?? "panel"} size={14} />
      </span>
    );
  };

  // Next `order` for a manually-added pill-slot chip: past the highest pill
  // order across ALL workflows (not just the row being edited), so a new
  // pill chip never ties with another workflow's migration-assigned slot.
  // (Moved from RecipesSection, Task 16 — trigger editing now lives here.)
  const nextPillOrder =
    Math.max(999, ...rows.flatMap((w) => (w.triggers ?? []).flatMap((tr) => (tr.kind === "pill_slot" ? [tr.order ?? 0] : [])))) + 10;

  /** Persist a trigger-chip edit (moved from RecipesSection's saveFlow). */
  const saveTriggers = async (wf: WorkflowRow, triggers: Trigger[]) => {
    if (chipsSaving) return;
    setChipsSaving(wf.id);
    try {
      await saveWorkflow({ ...rowToDef(wf), triggers });
      load();
    } finally {
      setChipsSaving(null);
    }
  };

  // Builtin group first, custom after — same order as the Workbench's
  // Recipes segment.
  const presets = rows.filter((w) => w.builtin);
  const customs = rows.filter((w) => !w.builtin);
  const pill = pillWorkflows(rows);
  // The legacy read-only hotkey rows are gone entirely: agent (Task 6 Fix
  // Round 1), meeting (Task 7), and finally STS (Task 9 —
  // migrate_legacy_call_triggers folds config.hotkey_sts into wf.call's own
  // Hotkey chip and clears the field) each migrated into recipe trigger
  // chips, so every trigger now renders through the table above.

  return (
    <div className="h-full flex flex-col">
      <div className="px-[26px] pt-5 flex-shrink-0">
        <div className="fonos-eyebrow">HOME</div>
        <h1 className="fonos-page-title mt-[3px]">{t("nav.home")}</h1>
        <div className="text-[11px] text-[rgba(255,255,255,0.4)] mt-1.5 max-w-[560px] leading-relaxed">
          {t("home.note")}
        </div>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto px-[26px] py-4">
        {/* Floating pill card */}
        <div className="mb-4 p-4 rounded-[14px] fonos-surface">
          <div className="text-[12px] font-semibold text-[#fafaf9]">{t("ov.pill.title")}</div>
          <div className="text-[10.5px] text-[rgba(255,255,255,0.35)] mt-1 leading-relaxed max-w-[520px]">
            {t("ov.pill.hint")}
          </div>
          <div className="flex items-center gap-2.5 mt-3.5 flex-wrap">
            <HotkeyInput
              value={config.pill_hotkey ?? ""}
              onChange={(v) => save({ pill_hotkey: v })}
            />
            <select
              value={config.pill_hotkey_capture ?? "hold"}
              onChange={(e) => save({ pill_hotkey_capture: e.target.value as "hold" | "toggle" })}
              className={captureSelectClass}
            >
              <option value="hold">{t("widgets.field.capture.hold")}</option>
              <option value="toggle">{t("widgets.field.capture.toggle")}</option>
            </select>
          </div>
          <div className="mt-3 text-[10.5px] text-[rgba(255,255,255,0.32)]">
            {t("ov.pill.follows").replace("{0}", currentName)}
          </div>
          {pill.length > 0 && (
            <div className="mt-1.5 text-[10.5px] text-[rgba(255,255,255,0.32)]">
              {t("home.pill.roller")}:{" "}
              {pill.map((w, i) => (
                <span key={w.id}>
                  {i > 0 && <span className="text-[rgba(255,255,255,0.28)]"> · </span>}
                  {workflowLabel(w)}
                </span>
              ))}
            </div>
          )}
        </div>

        {/* Trigger table — one row per recipe; the single editing surface
            for triggers. Recipe cards in the Workbench render these same
            chips read-only. */}
        <div className="mb-3.5 rounded-[12px] border border-[rgba(255,255,255,0.075)] bg-[rgba(255,255,255,0.02)] p-4">
          <div className="flex flex-col">
            {[...presets, ...customs].map((wf) => (
              <div
                key={wf.id}
                className={
                  "flex items-center gap-2.5 py-2 border-t border-[rgba(255,255,255,0.045)] first:border-t-0" +
                  (chipsSaving === wf.id ? " opacity-60 pointer-events-none" : "")
                }
              >
                {flowIcon(wf)}
                <a
                  onClick={() => onJumpToRecipe(wf.id)}
                  className="cursor-pointer text-[11.5px] text-[rgba(255,255,255,0.75)] border-b border-dotted border-[rgba(255,255,255,0.25)] hover:text-[var(--accent)] flex-shrink-0 w-[160px] truncate"
                >
                  {workflowLabel(wf)}
                </a>
                <div className="flex-1 min-w-0">
                  <TriggerChips
                    wf={wf}
                    isMic={wf.source_type_tag === "microphone"}
                    nextPillOrder={nextPillOrder}
                    onChange={(triggers) => saveTriggers(wf, triggers)}
                  />
                </div>
              </div>
            ))}
          </div>

        </div>
      </div>
    </div>
  );
}
