// Overview — the app's new top-level default landing page (Workbench P1,
// Task 15). Two things live here today: the floating pill's card (its global
// hotkey + capture mode + which recipe it currently follows) and a read-only
// trigger cheat-sheet (reused from the Workbench's Recipes segment, where it
// used to live behind a toolbar toggle — UsageOverview is unchanged, just
// promoted to a shared component). Design intent (per spec §3b): this page
// grows into an at-a-glance dashboard over time.

import { useEffect, useState } from "react";
import { useT } from "../lib/i18n";
import { useAppConfig } from "../lib/useAppConfig";
import { listWorkflows } from "../lib/api";
import type { WorkflowRow } from "../types";
import { workflowLabel } from "../lib/builtinLabels";
import { HotkeyInput } from "../components/HotkeyInput";
import { selectClass } from "./settings/constants";
import UsageOverview from "./workbench/UsageOverview";

const captureSelectClass = selectClass.replace("w-full ", "") + " w-auto";

export default function OverviewPage({ onJumpToRecipe }: { onJumpToRecipe: (id: string) => void }) {
  const t = useT();
  const { config, save } = useAppConfig();
  const [rows, setRows] = useState<WorkflowRow[]>([]);

  useEffect(() => {
    listWorkflows().then(setRows).catch(() => { /* no backend / ignore */ });
  }, []);

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

  return (
    <div className="h-full flex flex-col">
      <div className="px-[26px] pt-5 flex-shrink-0">
        <div className="fonos-eyebrow">OVERVIEW</div>
        <h1 className="fonos-page-title mt-[3px]">{t("nav.overview")}</h1>
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
        </div>

        {/* Trigger cheat-sheet */}
        <UsageOverview rows={rows} config={config} onJump={onJumpToRecipe} />
      </div>
    </div>
  );
}
