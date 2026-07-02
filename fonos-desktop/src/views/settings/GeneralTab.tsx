// General settings — microphone selection + speech recognition language.

import { useState, useEffect } from "react";
import type { AppConfig } from "../../types";
import { listAudioInputs } from "../../lib/api";
import { LANGUAGES, TARGET_LANGUAGES } from "./constants";

const FREQUENT_CODES = ["auto", "Chinese", "English", "Japanese", "Korean", "Cantonese", "French", "Spanish"];

export default function GeneralTab({
  config,
  onSave,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  const [audioInputs, setAudioInputs] = useState<string[]>([]);
  const [showAllLangs, setShowAllLangs] = useState(false);
  const [showAllTranslate, setShowAllTranslate] = useState(false);

  useEffect(() => {
    listAudioInputs().then(setAudioInputs).catch(() => {});
  }, []);

  const sttCurrent = config.stt_language || "auto";

  const selectSttLang = (code: string) => {
    onSave({ stt_language: code });
  };

  const sttFrequent = LANGUAGES.filter((l) => FREQUENT_CODES.includes(l.code));
  const sttRest = LANGUAGES.filter((l) => !FREQUENT_CODES.includes(l.code));
  const sttVisible = showAllLangs ? [...sttFrequent, ...sttRest] : sttFrequent;

  return (
    <div className="flex flex-col gap-5">
      {/* ── Microphone ── */}
      <div className="flex flex-col gap-2.5">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">Microphone</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            Auto Detect finds the best available mic (prefers external over built-in).
            Select a specific device to lock to it.
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          {audioInputs.map((name) => {
            const selected = (config.audio_input_device || "auto") === name;
            const isAuto = name === "auto";
            return (
              <button
                key={name}
                onClick={() => onSave({ audio_input_device: name })}
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
                  {isAuto ? "Auto Detect" : name}
                </span>
                {isAuto && (
                  <span className="text-[8px] text-[rgba(255,255,255,0.15)] ml-auto">
                    prefers external
                  </span>
                )}
              </button>
            );
          })}
        </div>

        <button
          onClick={() => listAudioInputs().then(setAudioInputs).catch(() => {})}
          className="text-[9px] text-[rgba(251,191,36,0.4)] hover:text-[#fbbf24] transition-colors self-start"
        >
          Refresh Devices
        </button>
      </div>

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Speech Recognition Language ── */}
      <div className="flex flex-col gap-2.5">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">Speech Recognition</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            Select your spoken language. Use Auto for mixed-language speech.
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
            {showAllLangs ? "Show less" : `Show ${sttRest.length} more languages...`}
          </button>
        )}
      </div>

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Translate Target Language ── */}
      <div className="flex flex-col gap-2.5">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">Translate To</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            Target language for the Translate mode. Click to switch.
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
                <span className="text-[10px] text-[rgba(251,191,36,0.35)]">Selected</span>
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
                  {showAllTranslate ? "Show less" : `Show ${translateRest.length} more languages...`}
                </button>
              )}
            </>
          );
        })()}
      </div>
    </div>
  );
}
