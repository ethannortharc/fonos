// First-run timeline card (onboarding P4): renders the six-node local funnel
// recorded by P1. Read-only diagnostics — the data never leaves the device.
// Self-fetching (precedent: settings/DoctorCard) so Stats.tsx only mounts it.

import { useEffect, useState } from "react";
import { getOnboardingEvents, type OnboardingEvent } from "../../lib/api";
import { t, useT } from "../../lib/i18n";

export const FUNNEL_STEPS = [
  "launch",
  "mic_granted",
  "first_transcript",
  "ax_granted",
  "first_insert",
  "first_command",
] as const;
export type FunnelStep = (typeof FUNNEL_STEPS)[number];

/** Activation targets (seconds since launch) — only these two steps carry a badge. */
const TARGETS: Partial<Record<FunnelStep, number>> = {
  first_transcript: 60,
  first_insert: 120,
};

export interface TimelineRow {
  step: FunnelStep;
  /** Seconds since launch (0 for launch itself); null = not reached yet. */
  elapsedSecs: number | null;
  /** Target verdict for target-bearing steps once reached; null otherwise. */
  targetMet: boolean | null;
}

/** Fold raw events into the fixed six-row model. No launch row ⇒ null (empty state). */
export function buildFirstRunTimeline(events: OnboardingEvent[]): TimelineRow[] | null {
  const byStep = new Map(events.map((e) => [e.step, e.created_at]));
  const launchAt = byStep.get("launch");
  if (launchAt == null) return null;
  const t0 = new Date(launchAt).getTime();
  return FUNNEL_STEPS.map((step) => {
    const at = byStep.get(step);
    if (at == null) return { step, elapsedSecs: null, targetMet: null };
    const elapsedSecs = Math.max(0, Math.round((new Date(at).getTime() - t0) / 1000));
    const target = TARGETS[step];
    return { step, elapsedSecs, targetMet: target != null ? elapsedSecs <= target : null };
  });
}

/** 47 → "0:47", 115 → "1:55". */
export function formatElapsed(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

export default function FirstRunTimeline() {
  useT();
  const [events, setEvents] = useState<OnboardingEvent[] | null>(null);

  useEffect(() => {
    getOnboardingEvents()
      .then(setEvents)
      .catch(() => setEvents([]));
  }, []);

  if (events === null) return null; // still loading — the card pops in with data

  const rows = buildFirstRunTimeline(events);

  return (
    <div
      data-testid="firstrun-card"
      className="bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] rounded-[10px] p-3.5"
    >
      <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] mb-2.5 block">
        {t("stats.firstrun.title")}
      </span>

      {rows === null ? (
        <div data-testid="firstrun-empty" className="text-[11px] text-[rgba(255,255,255,0.2)] py-1">
          {t("stats.firstrun.empty")}
        </div>
      ) : (
        <div className="flex flex-col">
          {rows.map((r) => {
            const done = r.elapsedSecs != null;
            return (
              <div
                key={r.step}
                data-testid={`firstrun-row-${r.step}`}
                data-state={done ? "done" : "pending"}
                className="flex items-center gap-2.5 py-1"
              >
                <span
                  className={
                    done
                      ? "w-2.5 h-2.5 rounded-full bg-[#16a34a] flex-shrink-0"
                      : "w-2.5 h-2.5 rounded-full border-2 border-[rgba(255,255,255,0.15)] flex-shrink-0"
                  }
                />
                <span
                  className={
                    done
                      ? "text-[11px] text-[rgba(255,255,255,0.6)] flex-1"
                      : "text-[11px] text-[rgba(255,255,255,0.3)] flex-1"
                  }
                >
                  {t(("stats.firstrun." + r.step) as any)}
                </span>
                {r.targetMet === true && (
                  <span className="text-[9px] text-[#16a34a]">
                    {t("stats.firstrun.target-met").replace(
                      "{secs}",
                      String(TARGETS[r.step])
                    )}
                  </span>
                )}
                <span className="text-[10px] font-mono tabular-nums text-[rgba(255,255,255,0.45)]">
                  {done ? formatElapsed(r.elapsedSecs!) : "—"}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
