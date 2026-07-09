// General settings — microphone selection + per-app text-insertion overrides.
// STT language, translate target, and the default insertion strategy used to
// live here as globals; they moved onto per-widget props in P2 (see the stt
// and insert cases in WidgetForm.tsx) — only per-app overrides remain here.

import { useT, setLocale, resolveLocale } from "../../lib/i18n";
import type { AppConfig } from "../../types";
import MicrophonePicker from "./MicrophonePicker";
import DoctorCard from "./DoctorCard";

export default function GeneralTab({
  config,
  onSave,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  const t = useT();

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
    </div>
  );
}
