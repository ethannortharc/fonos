// Workbench — the workbench-centered IA's home page (P1 skeleton). Three
// segments: Recipes (widgets assembled + triggers), Widgets (tuned/named
// instances of each type), Test Run (staged bench for stepping data through).
// This task renders placeholder segment bodies; Tasks 9-11 fill them in.

import { useState } from "react";
import { useT } from "../lib/i18n";
import { useAppConfig } from "../lib/useAppConfig";

export type WorkbenchSeg = "recipes" | "widgets" | "testrun";
export type BenchTarget = { kind: "recipe" | "widget"; id: string } | null;

export default function Workbench() {
  const t = useT();
  const { config, save } = useAppConfig();
  const [seg, setSeg] = useState<WorkbenchSeg>("recipes");
  const [benchTarget, setBenchTarget] = useState<BenchTarget>(null);
  const openBench = (target: BenchTarget) => {
    setBenchTarget(target);
    setSeg("testrun");
  };
  const SUB: Record<WorkbenchSeg, string> = {
    recipes: t("wb.sub.recipes"),
    widgets: t("wb.sub.widgets"),
    testrun: t("wb.sub.testrun"),
  };
  if (!config) return null;
  // Wired to real section props in Tasks 9-11 — placeholder bodies below
  // don't consume them yet.
  void save;
  void benchTarget;
  void openBench;
  return (
    <div className="h-full flex flex-col">
      <div className="px-[26px] pt-5 flex-shrink-0">
        <div className="fonos-eyebrow">WORKBENCH</div>
        <div className="flex items-baseline gap-2.5">
          <h1 className="fonos-page-title mt-[3px]">{t("wb.title")}</h1>
          <span className="text-[11px] text-[rgba(255,255,255,0.43)]">{SUB[seg]}</span>
        </div>
        <div className="inline-flex gap-0.5 mt-3.5 rounded-[10px] bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] p-[3px]">
          {(["recipes", "widgets", "testrun"] as const).map((k) => (
            <button
              key={k}
              onClick={() => setSeg(k)}
              className={[
                "rounded-[7px] px-4 py-[5px] text-[11.5px] font-medium transition-colors",
                seg === k
                  ? "bg-[rgba(240,173,50,0.13)] text-[var(--accent)]"
                  : "text-[rgba(255,255,255,0.43)] hover:text-[rgba(255,255,255,0.62)]",
              ].join(" ")}
            >
              {t(`wb.seg.${k}` as const)}
            </button>
          ))}
        </div>
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto px-[26px] py-4">
        {seg === "recipes" && <div /* Task 10: <RecipesSection config={config} onSave={save} onOpenBench={openBench} …/> */ />}
        {seg === "widgets" && <div /* Task 9: <WidgetsSection config={config} onSave={save} onOpenBench={openBench} …/> */ />}
        {seg === "testrun" && <div /* Task 11: <TestRunSection benchTarget={benchTarget} config={config} …/> */ />}
      </div>
    </div>
  );
}
