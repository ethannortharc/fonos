// Pure planning logic for the engine-setup review card (onboarding P3):
// detection + tier + disk → an executable SetupPlan plus display rows.
// Mirrors fonos_core::engine_setup's tier table — keep the two in sync.

import type { EngineDetection, SetupPlan } from "../types";

// Re-exported so consumers (this module's test, Task 8/9) can import the
// detection shape from here without a separate "../types" import.
export type { EngineDetection, SetupPlan };

export type EngineKey = "omlx" | "lmstudio" | "ollama" | "vllm";
export type Tier = "light" | "balanced" | "max";

/** Recommended Ollama pull per tier. sizeGb is a display/precheck estimate. */
export const TIER_PULLS: Record<Tier, { model: string; sizeGb: number }> = {
  max: { model: "qwen3:30b-a3b", sizeGb: 18.6 },
  balanced: { model: "qwen3:14b", sizeGb: 9.3 },
  light: { model: "qwen3:4b", sizeGb: 2.6 },
};

/** The next tier down for failure/disk downgrade suggestions. */
export function suggestDowngrade(tier: Tier): Tier | null {
  if (tier === "max") return "balanced";
  if (tier === "balanced") return "light";
  return null;
}

/** Engines fonos can install automatically. */
const AUTO_INSTALL: ReadonlySet<string> = new Set(["omlx", "ollama"]);

/** One review-card display row. */
export interface ReviewRow {
  kind: "install" | "start" | "pull" | "note" | "manual";
  /** The pull row's model (editable in the card). */
  model?: string;
  /** Estimated GB for pull rows. */
  sizeGb?: number;
}

export interface BuiltPlan {
  plan: SetupPlan;
  rows: ReviewRow[];
  /** Estimated pull volume fits available disk (with 10% headroom). */
  diskOk: boolean;
  requiredGb: number;
  downgrade: Tier | null;
}

/** Build the executable plan + review rows for one engine selection. */
export function buildSetupPlan(
  detection: EngineDetection,
  tier: Tier,
  diskAvailableKb: number,
  engine: EngineKey
): BuiltPlan {
  const rows: ReviewRow[] = [];
  const auto = AUTO_INSTALL.has(engine);
  const install = auto && !detection.installed;
  const start = auto && !detection.running;

  if (!auto && !detection.installed) rows.push({ kind: "manual" });
  if (install) rows.push({ kind: "install" });
  if (start) rows.push({ kind: "start" });

  let pulls: string[] = [];
  let requiredGb = 0;
  if (engine === "ollama") {
    const rec = TIER_PULLS[tier];
    pulls = [rec.model];
    requiredGb = rec.sizeGb;
    rows.push({ kind: "pull", model: rec.model, sizeGb: rec.sizeGb });
  }
  if (engine === "omlx") rows.push({ kind: "note" });

  const availGb = diskAvailableKb / 1_000_000;
  const diskOk = requiredGb === 0 || availGb > requiredGb * 1.1;

  return {
    plan: { engine, install, start, pulls, base_url: detection.url },
    rows,
    diskOk,
    requiredGb,
    downgrade: diskOk ? null : suggestDowngrade(tier),
  };
}
