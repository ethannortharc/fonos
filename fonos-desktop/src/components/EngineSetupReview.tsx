// Pre-execution review card (onboarding P3): every default laid out as an
// editable/inspectable row, disk verdict, one confirm — then a single
// progress line (bar + stage text) driven by engine:setup events until
// done/error/manual. Editing a default is cheap; that's the whole point of
// defaults.
//
// Reconciled against the shipped backend (commands::engine_setup): every
// error carries `failed_stage` (incl. "busy" for the re-entrancy rejection),
// and a terminal "manual" stage tells the user to install by hand. A smaller
// model only helps a *pull* or disk shortfall, so downgrade is offered there
// and nowhere else.

import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { engineSetup } from "../lib/api";
import type { EngineSetupEvent } from "../types";
import { suggestDowngrade, type BuiltPlan, type Tier } from "../lib/engineSetup";
import { t, td, useT } from "../lib/i18n";

const STAGE_KEY: Record<string, string> = {
  install: "engine.review.stage.install",
  start: "engine.review.stage.start",
  wait: "engine.review.stage.wait",
  pull: "engine.review.stage.pull",
};

/** Error stages a smaller model can't remedy — no downgrade offered. */
const NON_DOWNGRADE_STAGES: ReadonlySet<string> = new Set([
  "busy",
  "install",
  "start",
  "wait",
]);

export default function EngineSetupReview({
  built,
  engineName,
  tier,
  onRetier,
  onCancel,
  onDone,
}: {
  built: BuiltPlan;
  engineName: string;
  tier: Tier;
  onRetier: (t: Tier) => void;
  onCancel: () => void;
  onDone: () => void;
}) {
  useT();
  const [running, setRunning] = useState(false);
  const [stage, setStage] = useState<EngineSetupEvent | null>(null);
  const [error, setError] = useState("");
  const [failedStage, setFailedStage] = useState<EngineSetupEvent["failed_stage"]>();
  const [manualMsg, setManualMsg] = useState("");
  const doneRef = useRef(false);

  // Subscribe to engine:setup progress. Disposed-guard idiom (mirrors
  // ModelsTab's diarize:download subscription): a `disposed` flag drops a
  // listen() that resolves after this effect's cleanup already ran, so no
  // listener leaks under StrictMode's double-mount.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<string>("engine:setup", (e) => {
      let ev: EngineSetupEvent;
      try {
        ev = JSON.parse(e.payload) as EngineSetupEvent;
      } catch {
        return;
      }
      // Only process events for the current engine; stray events from
      // previously-abandoned setups must not affect this card.
      if (ev.engine && ev.engine !== built.plan.engine) return;
      if (ev.stage === "error") {
        setError(ev.message ?? "setup failed");
        setFailedStage(ev.failed_stage);
        setRunning(false);
        return;
      }
      if (ev.stage === "manual") {
        // Terminal: the engine has no automated install. Show the backend's
        // human-readable steps; no downgrade (installing is the blocker).
        setManualMsg(ev.message ?? "manual setup required");
        setRunning(false);
        return;
      }
      if (ev.stage === "done") {
        if (!doneRef.current) {
          doneRef.current = true;
          setRunning(false);
          onDone();
        }
        return;
      }
      setStage(ev);
    }).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const confirm = async () => {
    setError("");
    setFailedStage(undefined);
    setManualMsg("");
    setStage(null);
    setRunning(true);
    try {
      await engineSetup(built.plan);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setRunning(false);
    }
  };

  const rowLabel = (kind: string): string => {
    switch (kind) {
      case "install":
        return td("engine.review.install", [engineName]);
      case "start":
        return td("engine.review.start", [engineName]);
      case "note":
        return t("engine.review.ondemand");
      case "manual":
        return td("engine.review.manual", [engineName]);
      default:
        return "";
    }
  };

  const busy = failedStage === "busy";
  // A smaller model only remedies a pull/disk shortfall (or an unclassified
  // failure the legacy backend left `failed_stage`-less). Never for busy or a
  // failed install/start/wait.
  const errDowngrade =
    error && !NON_DOWNGRADE_STAGES.has(failedStage ?? "") ? suggestDowngrade(tier) : null;
  const hasPct = running && stage && typeof stage.pct === "number";

  return (
    <div className="mt-3 rounded-xl border border-[rgba(242,184,75,0.25)] bg-[rgba(242,184,75,0.03)] p-4 flex flex-col gap-3">
      <div className="text-[12px] font-semibold text-[#fafaf9]">{t("engine.review.title")}</div>

      <div className="rounded-lg border border-[rgba(255,255,255,0.06)] divide-y divide-[rgba(255,255,255,0.04)]">
        {built.rows.map((r, i) => (
          <div
            key={i}
            data-testid={`review-row-${r.kind}`}
            className="flex items-center gap-2 px-3 py-2 text-[11px] text-[rgba(255,255,255,0.75)]"
          >
            {r.kind === "pull" ? (
              <>
                <span>⬇️</span>
                <span className="font-mono">{r.model}</span>
                <span className="ml-auto text-[10px] text-[rgba(255,255,255,0.4)] tabular-nums">
                  {td("engine.review.size", [String(r.sizeGb ?? 0)])}
                </span>
              </>
            ) : (
              <span>{rowLabel(r.kind)}</span>
            )}
          </div>
        ))}
      </div>

      {!built.diskOk && (
        <div className="text-[11px] text-[#f2b84b] flex items-center gap-2 flex-wrap">
          <span>{td("engine.review.disk.low", [String(built.requiredGb)])}</span>
          {built.downgrade && (
            <button
              data-testid="review-downgrade"
              onClick={() => onRetier(built.downgrade!)}
              className="underline underline-offset-2 hover:text-[#fafaf9] transition-colors"
            >
              {t("engine.review.downgrade")}
            </button>
          )}
        </div>
      )}

      {error && (
        <div
          data-testid="review-error"
          className={`text-[11px] flex items-center gap-2 flex-wrap ${busy ? "text-[#f2b84b]" : "text-[#f87171]"}`}
        >
          <span>{busy ? t("engine.review.busy") : error}</span>
          {errDowngrade && (
            <button
              data-testid="review-error-downgrade"
              onClick={() => onRetier(errDowngrade)}
              className="underline underline-offset-2 text-[#f2b84b] hover:text-[#fafaf9] transition-colors"
            >
              {t("engine.review.downgrade")}
            </button>
          )}
        </div>
      )}

      {manualMsg && (
        <div data-testid="review-manual" className="text-[11px] text-[#f2b84b] flex items-center gap-2 flex-wrap">
          <span>{manualMsg}</span>
        </div>
      )}

      {running && stage && (
        <div className="flex flex-col gap-1.5">
          <div className="h-1 rounded-full bg-[rgba(255,255,255,0.06)] overflow-hidden">
            <div
              className={
                "h-full rounded-full bg-[var(--accent)] transition-[width] duration-300 ease-out motion-reduce:transition-none " +
                (hasPct ? "" : "w-1/3 animate-pulse motion-reduce:animate-none")
              }
              style={hasPct ? { width: `${stage.pct}%` } : undefined}
            />
          </div>
          <div
            data-testid="review-progress"
            className="text-[11px] text-[rgba(255,255,255,0.6)] tabular-nums"
          >
            {t((STAGE_KEY[stage.stage] ?? "engine.review.stage.wait") as Parameters<typeof t>[0])}
            {stage.stage === "pull" && stage.model ? ` ${stage.model}` : ""}
            {typeof stage.pct === "number" ? ` — ${stage.pct}%` : ""}
          </div>
        </div>
      )}

      <div className="flex items-center gap-2">
        <button
          data-testid="review-confirm"
          onClick={confirm}
          disabled={!built.diskOk || running}
          className="px-5 py-2 rounded-lg bg-[rgba(242,184,75,0.14)] border border-[rgba(242,184,75,0.35)] text-[var(--accent)] text-[12px] font-semibold hover:bg-[rgba(242,184,75,0.2)] transition-colors disabled:opacity-40"
        >
          {running ? t("engine.review.running") : t("engine.review.confirm")}
        </button>
        <button
          onClick={onCancel}
          disabled={running}
          className="px-4 py-2 rounded-lg border border-[rgba(255,255,255,0.08)] text-[rgba(255,255,255,0.5)] text-[11px] hover:text-[rgba(255,255,255,0.8)] transition-colors disabled:opacity-40"
        >
          {t("common.cancel")}
        </button>
      </div>
    </div>
  );
}
