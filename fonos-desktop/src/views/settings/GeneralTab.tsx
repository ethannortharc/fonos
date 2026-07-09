// General settings — microphone selection + per-app text-insertion overrides.
// STT language, translate target, and the default insertion strategy used to
// live here as globals; they moved onto per-widget props in P2 (see the stt
// and insert cases in WidgetForm.tsx) — only per-app overrides remain here.

import { useState, useEffect } from "react";
import { useT, setLocale, resolveLocale } from "../../lib/i18n";
import type { AppConfig, InjectionAppOverride } from "../../types";
import MicrophonePicker from "./MicrophonePicker";
import DoctorCard from "./DoctorCard";

const INJECTION_STRATEGIES = [
  { value: "paste", labelKey: "general.insert.paste", shortKey: "general.insert.paste.short" },
  { value: "type", labelKey: "general.insert.type", shortKey: "general.insert.type.short" },
] as const;

export default function GeneralTab({
  config,
  onSave,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  const t = useT();
  const [overrides, setOverrides] = useState<InjectionAppOverride[]>(config.injection_app_overrides ?? []);

  useEffect(() => {
    // Sync from config but keep unsaved local rows (blank app names are
    // filtered out on save) so an in-progress override isn't wiped mid-edit.
    setOverrides((prev) => {
      const saved = config.injection_app_overrides ?? [];
      const blanks = prev.filter((r) => r.app.trim() === "");
      return [...saved, ...blanks];
    });
  }, [config.injection_app_overrides]);

  const persistOverrides = (rows: InjectionAppOverride[]) => {
    setOverrides(rows);
    onSave({ injection_app_overrides: rows.filter((r) => r.app.trim() !== "") });
  };

  const updateOverride = (i: number, patch: Partial<InjectionAppOverride>) => {
    persistOverrides(overrides.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
  };

  // Typing in the app field updates local state only; persistence happens on
  // blur so config.json isn't rewritten on every keystroke.
  const updateOverrideLocal = (i: number, patch: Partial<InjectionAppOverride>) => {
    setOverrides(overrides.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
  };

  const removeOverride = (i: number) => {
    persistOverrides(overrides.filter((_, idx) => idx !== i));
  };

  const addOverride = () => {
    setOverrides([...overrides, { app: "", strategy: "paste" }]);
  };

  return (
    <div className="flex flex-col gap-5">
      {/* ── Setup Doctor (resident config-health card) ── */}
      <DoctorCard />

      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Interface language ── */}
      <div className="flex flex-col gap-2">
        <div className="text-[12px] font-medium text-[#fafaf9]">{t("general.language")}</div>
        <div className="flex gap-1.5">
          {([["auto", t("general.language.auto")], ["en", t("general.language.en")], ["zh", t("general.language.zh")]] as const).map(([val, label]) => (
            <button
              key={val}
              onClick={() => {
                onSave({ ui_language: val });
                setLocale(resolveLocale(val));
              }}
              className={[
                "px-3 py-1.5 rounded-lg text-[10.5px] border transition-all",
                (config.ui_language ?? "auto") === val
                  ? "bg-[rgba(245,158,11,0.12)] border-[rgba(245,158,11,0.3)] text-[#fbbf24]"
                  : "bg-[rgba(255,255,255,0.02)] border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.7)]",
              ].join(" ")}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Microphone ── */}
      <MicrophonePicker
        value={config.audio_input_device}
        onSelect={(name) => onSave({ audio_input_device: name })}
      />

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Model warm-up ── */}
      <div className="flex items-center justify-between gap-4">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">{t("general.warmup.title")}</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            {t("general.warmup.desc")}
          </div>
        </div>
        <button
          onClick={() => onSave({ warmup_enabled: !(config.warmup_enabled ?? true) })}
          className={[
            "px-2.5 py-1.5 rounded-lg text-[10px] transition-all flex-shrink-0",
            (config.warmup_enabled ?? true)
              ? "bg-[rgba(74,222,128,0.1)] border border-[rgba(74,222,128,0.2)] text-[#4ade80]"
              : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.3)]",
          ].join(" ")}
        >
          {(config.warmup_enabled ?? true) ? t("common.enabled") : t("common.disabled")}
        </button>
      </div>

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Text insertion ── */}
      <div className="flex flex-col gap-2.5">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">{t("general.insert.title")}</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            {t("general.insert.desc")}
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          {overrides.map((row, i) => (
            <div key={i} className="flex items-center gap-1.5">
              <input
                type="text"
                value={row.app}
                onChange={(e) => updateOverrideLocal(i, { app: e.target.value })}
                onBlur={() => persistOverrides(overrides)}
                placeholder={t("general.insert.appplaceholder")}
                className="flex-1 min-w-0 px-2.5 py-2 rounded-lg text-[11px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.7)] placeholder:text-[rgba(255,255,255,0.2)] focus:outline-none focus:border-[rgba(245,158,11,0.25)]"
              />
              {INJECTION_STRATEGIES.map((opt) => {
                const selected = (row.strategy || "paste") === opt.value;
                return (
                  <button
                    key={opt.value}
                    onClick={() => updateOverride(i, { strategy: opt.value })}
                    className={[
                      "px-2.5 py-2 rounded-lg text-[10px] transition-all flex-shrink-0",
                      selected
                        ? "bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.25)] text-[#fbbf24] font-medium"
                        : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:border-[rgba(255,255,255,0.08)]",
                    ].join(" ")}
                  >
                    {t(opt.shortKey)}
                  </button>
                );
              })}
              <button
                onClick={() => removeOverride(i)}
                className="px-2 py-2 rounded-lg text-[13px] leading-none text-[rgba(255,255,255,0.3)] hover:text-[#fbbf24] transition-colors flex-shrink-0"
              >
                ×
              </button>
            </div>
          ))}

          <button
            onClick={addOverride}
            className="text-[10px] text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24] transition-colors self-start"
          >
            {t("general.insert.addoverride")}
          </button>
        </div>

        <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
          {t("general.insert.overridehint")}
        </div>
      </div>
    </div>
  );
}
