// General settings — microphone selection + speech recognition language.

import { useState, useEffect } from "react";
import { useT, setLocale, resolveLocale } from "../../lib/i18n";
import type { AppConfig, InjectionAppOverride } from "../../types";
import { LANGUAGES, TARGET_LANGUAGES } from "./constants";
import MicrophonePicker from "./MicrophonePicker";

const FREQUENT_CODES = ["auto", "Chinese", "English", "Japanese", "Korean", "Cantonese", "French", "Spanish"];

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
  const [showAllLangs, setShowAllLangs] = useState(false);
  const [showAllTranslate, setShowAllTranslate] = useState(false);
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

  const sttCurrent = config.stt_language || "auto";
  const injectionStrategy = config.injection_strategy ?? "paste";

  const selectSttLang = (code: string) => {
    onSave({ stt_language: code });
  };

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

  const sttFrequent = LANGUAGES.filter((l) => FREQUENT_CODES.includes(l.code));
  const sttRest = LANGUAGES.filter((l) => !FREQUENT_CODES.includes(l.code));
  const sttVisible = showAllLangs ? [...sttFrequent, ...sttRest] : sttFrequent;

  return (
    <div className="flex flex-col gap-5">
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

      {/* ── Speech Recognition Language ── */}
      <div className="flex flex-col gap-2.5">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">{t("general.stt.title")}</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            {t("general.stt.desc")}
          </div>
        </div>

        {/* Language grid */}
        <div className="grid grid-cols-3 gap-1.5">
          {sttVisible.map((lang) => {
            const selected = sttCurrent === lang.code;
            return (
              <button key={lang.code} onClick={() => selectSttLang(lang.code)}
                className={[
                  "flex items-center gap-2 px-2.5 py-2 rounded-lg text-left transition-all",
                  selected
                    ? "bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.25)]"
                    : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)]",
                ].join(" ")}>
                <span className="text-[13px]">{lang.flag}</span>
                <span className={["text-[10px] truncate",
                  selected ? "text-[#fbbf24] font-medium" : "text-[rgba(255,255,255,0.45)]",
                ].join(" ")}>{lang.label}</span>
              </button>
            );
          })}
        </div>

        {sttRest.length > 0 && (
          <button onClick={() => setShowAllLangs(!showAllLangs)}
            className="text-[10px] text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24] transition-colors self-start">
            {showAllLangs ? t("general.showless") : t("general.showmore").replace("{n}", String(sttRest.length))}
          </button>
        )}
      </div>

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Translate Target Language ── */}
      <div className="flex flex-col gap-2.5">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">{t("general.translate.title")}</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            {t("general.translate.desc")}
          </div>
        </div>

        {(() => {
          const current =
            TARGET_LANGUAGES.find((l) => l.code === config.translate_target) ??
            TARGET_LANGUAGES[0];
          const translateFrequent = TARGET_LANGUAGES.filter((l) => FREQUENT_CODES.includes(l.code));
          const translateRest = TARGET_LANGUAGES.filter((l) => !FREQUENT_CODES.includes(l.code));
          const visible = showAllTranslate ? [...translateFrequent, ...translateRest] : translateFrequent;
          return (
            <>
              <div className="flex items-center gap-3 px-3.5 py-2.5 rounded-[10px] bg-[rgba(245,158,11,0.1)] border border-[rgba(245,158,11,0.2)]">
                <span className="text-[18px]">{current.flag}</span>
                <div className="flex-1">
                  <div className="text-[13px] font-medium text-[#fbbf24]">{current.label}</div>
                </div>
                <span className="text-[10px] text-[rgba(251,191,36,0.35)]">{t("general.selected")}</span>
              </div>

              <div className="grid grid-cols-3 gap-1.5">
                {visible.map((lang) => {
                  const isSelected = config.translate_target === lang.code;
                  return (
                    <button key={lang.code} onClick={() => onSave({ translate_target: lang.code })}
                      className={[
                        "flex items-center gap-2 px-2.5 py-2 rounded-lg text-left transition-all",
                        isSelected
                          ? "bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.25)]"
                          : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)]",
                      ].join(" ")}>
                      <span className="text-[13px]">{lang.flag}</span>
                      <span className={["text-[10px] truncate",
                        isSelected ? "text-[#fbbf24] font-medium" : "text-[rgba(255,255,255,0.45)]",
                      ].join(" ")}>{lang.label}</span>
                    </button>
                  );
                })}
              </div>

              {translateRest.length > 0 && (
                <button onClick={() => setShowAllTranslate(!showAllTranslate)}
                  className="text-[10px] text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24] transition-colors self-start">
                  {showAllTranslate ? t("general.showless") : t("general.showmore").replace("{n}", String(translateRest.length))}
                </button>
              )}
            </>
          );
        })()}
      </div>

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
          {INJECTION_STRATEGIES.map((opt) => {
            const selected = injectionStrategy === opt.value;
            return (
              <button
                key={opt.value}
                onClick={() => onSave({ injection_strategy: opt.value })}
                className={[
                  "flex items-center gap-3 px-3 py-2 rounded-lg text-left transition-all text-[11px]",
                  selected
                    ? "bg-[rgba(245,158,11,0.08)] border border-[rgba(245,158,11,0.15)]"
                    : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)]",
                ].join(" ")}
              >
                <div className={[
                  "w-1.5 h-1.5 rounded-full flex-shrink-0",
                  selected ? "bg-[#fbbf24]" : "bg-[rgba(255,255,255,0.1)]",
                ].join(" ")} />
                <span className={selected ? "text-[#fbbf24]" : "text-[rgba(255,255,255,0.45)]"}>
                  {t(opt.labelKey)}
                </span>
              </button>
            );
          })}
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
