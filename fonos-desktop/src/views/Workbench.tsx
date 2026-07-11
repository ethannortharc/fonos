// Workbench — the workbench-centered IA's home page (P1 skeleton). Three
// segments: Recipes (widgets assembled + triggers), Widgets (tuned/named
// instances of each type), Test Run (staged bench for stepping data through).
// Recipes (Task 10) and Widgets (Task 9) are wired; Test Run (Task 11) still
// renders a placeholder body.

import { useEffect, useState } from "react";
import { useT } from "../lib/i18n";
import { useAppConfig } from "../lib/useAppConfig";
import { listContainers } from "../lib/storage-api";
import type { Container } from "../lib/storage-api";
import WidgetsSection from "./workbench/WidgetsSection";
import RecipesSection from "./workbench/RecipesSection";

export type WorkbenchSeg = "recipes" | "widgets" | "testrun";
export type BenchTarget = { kind: "recipe" | "widget"; id: string } | null;

export default function Workbench() {
  const t = useT();
  const { config, save } = useAppConfig();
  const [seg, setSeg] = useState<WorkbenchSeg>("recipes");
  const [benchTarget, setBenchTarget] = useState<BenchTarget>(null);
  const [containers, setContainers] = useState<Container[]>([]);
  useEffect(() => {
    listContainers().then(setContainers).catch(() => { /* no backend / ignore */ });
  }, []);
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
  // Wired to real section props in Tasks 10-11 — placeholder bodies below
  // don't consume them yet.
  void save;
  void benchTarget;
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
        {seg === "recipes" && (
          <RecipesSection config={config} onBench={(id) => openBench({ kind: "recipe", id })} />
        )}
        {seg === "widgets" && (
          <WidgetsSection
            config={config}
            containers={containers}
            onTest={(id) => openBench({ kind: "widget", id })}
          />
        )}
        {seg === "testrun" && <div /* Task 11: <TestRunSection benchTarget={benchTarget} config={config} …/> */ />}
      </div>
    </div>
  );
}
