// Setup Doctor (issue #30) — resident config-health card at the top of
// Settings › General. Auto-runs on mount, surfaces silent-failure findings with
// one-click fixes, and collapses to a single green row when everything passes.

import { useCallback, useEffect, useState } from "react";
import { useT, td } from "../../lib/i18n";
import { runDoctor, applyDoctorFix, openSettingsPane } from "../../lib/api";
import type { DoctorFinding, DoctorFix } from "../../types";

// Status circle visuals per severity, mirroring the approved mockup (§②).
const STATUS = {
  pass: { glyph: "✓", cls: "bg-[rgba(74,222,128,0.12)] text-[#4ade80]" },
  warn: { glyph: "!", cls: "bg-[rgba(251,191,36,0.13)] text-[#fbbf24]" },
  advise: { glyph: "↯", cls: "bg-[rgba(248,113,113,0.13)] text-[#f87171]" },
} as const;

/** Fix button label key by action kind. */
function fixLabelKey(fix: DoctorFix): string {
  switch (fix.kind) {
    case "attach_book_global":
      return "doctor.fix.attach";
    case "switch_tts_model":
      return "doctor.fix.switch";
    case "open_settings_pane":
      return "doctor.fix.opensettings";
    default:
      return "doctor.fix.reset";
  }
}

/** Endpoint rows carry a `host · latency` detail in params[1], shown as small text. */
function smallDetail(f: DoctorFinding): string | undefined {
  return f.id.startsWith("endpoint") ? f.message_params[1] : undefined;
}

export default function DoctorCard() {
  const t = useT();
  const [findings, setFindings] = useState<DoctorFinding[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>("");
  const [checkedAt, setCheckedAt] = useState<number>(0);
  const [expanded, setExpanded] = useState(false);
  const [fixing, setFixing] = useState<string | null>(null);
  const [, setTick] = useState(0); // drives the relative-time label

  const check = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const result = await runDoctor();
      setFindings(result);
      setCheckedAt(Date.now());
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  // Auto-run on tab mount.
  useEffect(() => {
    check();
  }, [check]);

  // Refresh the "checked Nm ago" label periodically.
  useEffect(() => {
    const id = setInterval(() => setTick((n) => n + 1), 30_000);
    return () => clearInterval(id);
  }, []);

  const onFix = useCallback(
    async (f: DoctorFinding) => {
      if (!f.fix) return;
      setFixing(f.id);
      try {
        if (f.fix.kind === "open_settings_pane") {
          await openSettingsPane(f.fix.pane);
        } else {
          await applyDoctorFix(f.fix);
          await check(); // re-running everything is acceptable for v1
        }
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setFixing(null);
      }
    },
    [check]
  );

  // ── First run: slim placeholder ─────────────────────────────────────────────
  if (findings === null) {
    return (
      <div className="rounded-xl bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.07)]">
        <div className="flex items-center gap-2.5 px-3.5 py-3">
          <span className="text-[14px]">🩺</span>
          <span className="text-[12px] font-medium text-[#fafaf9]">{t("doctor.title")}</span>
          <span className="ml-auto text-[10.5px] text-[rgba(255,255,255,0.32)]">
            {error ? td("doctor.error", [error]) : t("doctor.checking")}
          </span>
        </div>
      </div>
    );
  }

  const passed = findings.filter((f) => f.severity === "pass").length;
  const warnings = findings.filter((f) => f.severity === "warn").length;
  const suggestions = findings.filter((f) => f.severity === "advise").length;
  const allPass = warnings === 0 && suggestions === 0 && findings.length > 0;

  const minsAgo = Math.floor((Date.now() - checkedAt) / 60_000);
  const checkedLabel = loading
    ? t("doctor.checking")
    : minsAgo < 1
    ? t("doctor.checked.justnow")
    : td("doctor.checked.min", [String(minsAgo)]);

  const chip = (n: number, key: string, cls: string) =>
    n > 0 ? (
      <span className={["text-[9px] px-2 py-0.5 rounded-full", cls].join(" ")}>
        {td(key, [String(n)])}
      </span>
    ) : null;

  const recheckBtn = (
    <button
      onClick={check}
      disabled={loading}
      className="text-[10px] px-2.5 py-1 rounded-lg border border-[rgba(255,255,255,0.07)] bg-[rgba(255,255,255,0.03)] text-[rgba(255,255,255,0.55)] hover:text-[rgba(255,255,255,0.8)] transition-colors disabled:opacity-50"
    >
      {t("common.recheck")}
    </button>
  );

  // ── Collapsed all-pass state: a single slim green row ───────────────────────
  if (allPass && !expanded) {
    return (
      <div className="rounded-xl bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.07)]">
        <button
          onClick={() => setExpanded(true)}
          className="w-full flex items-center gap-2.5 px-3.5 py-3 text-left"
        >
          <span
            className={[
              "w-[17px] h-[17px] rounded-full flex items-center justify-center text-[10px] font-bold flex-none",
              STATUS.pass.cls,
            ].join(" ")}
          >
            ✓
          </span>
          <span className="text-[11.5px] text-[#4ade80]">
            {td("doctor.allpassed", [String(findings.length)])}
          </span>
          <span className="ml-auto text-[10.5px] text-[rgba(255,255,255,0.32)]">{checkedLabel}</span>
        </button>
      </div>
    );
  }

  // ── Full card ───────────────────────────────────────────────────────────────
  return (
    <div className="rounded-xl bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.07)]">
      {/* Header */}
      <div className="flex items-center gap-2.5 px-3.5 py-3 border-b border-[rgba(255,255,255,0.05)] flex-wrap">
        <span className="text-[14px]">🩺</span>
        <span className="text-[12px] font-medium text-[#fafaf9]">{t("doctor.title")}</span>
        {chip(passed, "doctor.chip.passed", "bg-[rgba(74,222,128,0.1)] text-[#4ade80]")}
        {chip(warnings, "doctor.chip.warnings", "bg-[rgba(251,191,36,0.12)] text-[#fbbf24]")}
        {chip(suggestions, "doctor.chip.suggestions", "bg-[rgba(248,113,113,0.1)] text-[#f87171]")}
        <span className="ml-auto text-[10.5px] text-[rgba(255,255,255,0.32)]">{checkedLabel}</span>
        {recheckBtn}
      </div>

      {error && (
        <div className="px-3.5 py-2 text-[10.5px] text-[#f87171] border-b border-[rgba(255,255,255,0.05)]">
          {td("doctor.error", [error])}
        </div>
      )}

      {/* Rows */}
      {findings.map((f) => {
        const status = STATUS[f.severity];
        const small = smallDetail(f);
        return (
          <div
            key={f.id}
            className="flex items-center gap-2.5 px-3.5 py-2.5 border-b border-[rgba(255,255,255,0.04)] last:border-b-0"
          >
            <span
              className={[
                "w-[17px] h-[17px] rounded-full flex items-center justify-center text-[10px] font-bold flex-none",
                status.cls,
              ].join(" ")}
            >
              {status.glyph}
            </span>
            <span className="flex-1 min-w-0 text-[11.5px] text-[rgba(255,255,255,0.55)]">
              {td(f.message_key, f.message_params)}
            </span>
            {small && (
              <span className="text-[10px] text-[rgba(255,255,255,0.32)] tabular-nums flex-none">
                {small}
              </span>
            )}
            {f.fix && (
              <button
                onClick={() => onFix(f)}
                disabled={fixing === f.id}
                className="flex-none text-[10px] px-2.5 py-1 rounded-lg border border-[rgba(251,191,36,0.25)] bg-[rgba(251,191,36,0.1)] text-[#fbbf24] hover:bg-[rgba(251,191,36,0.16)] transition-colors disabled:opacity-50"
              >
                {fixing === f.id ? t("doctor.fixing") : td(fixLabelKey(f.fix))}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
