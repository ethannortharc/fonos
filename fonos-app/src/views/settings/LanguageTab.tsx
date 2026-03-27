// Language tab — multi-select STT languages, single-select translate target.

import { useState } from "react";
import type { AppConfig } from "../../types";
import { LANGUAGES, TARGET_LANGUAGES } from "./constants";

const FREQUENT_CODES = ["auto", "Chinese", "English", "Japanese", "Korean", "Cantonese", "French", "Spanish"];

export default function LanguageTab({
  config,
  onSave,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  const [showAllStt, setShowAllStt] = useState(false);
  const [showAllTranslate, setShowAllTranslate] = useState(false);

  // Parse multi-select STT languages (comma-separated in config)
  const sttSelected = config.stt_language
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  const toggleSttLang = (code: string) => {
    if (code === "auto") {
      onSave({ stt_language: "auto" });
      return;
    }
    let next = sttSelected.filter((c) => c !== "auto");
    if (next.includes(code)) {
      next = next.filter((c) => c !== code);
    } else {
      next.push(code);
    }
    if (next.length === 0) next = ["auto"];
    onSave({ stt_language: next.join(",") });
  };

  // Sort: selected first, then frequent, then rest
  const sttFrequent = LANGUAGES.filter((l) => FREQUENT_CODES.includes(l.code));
  const sttRest = LANGUAGES.filter((l) => !FREQUENT_CODES.includes(l.code));
  const sttVisible = showAllStt ? [...sttFrequent, ...sttRest] : sttFrequent;

  // Move selected to front (except auto which stays first)
  const selectedLangs = sttVisible.filter(
    (l) => sttSelected.includes(l.code) && l.code !== "auto"
  );
  const unselectedLangs = sttVisible.filter(
    (l) => !sttSelected.includes(l.code) && l.code !== "auto"
  );
  const autoLang = LANGUAGES.find((l) => l.code === "auto")!;
  const sttOrdered = [autoLang, ...selectedLangs, ...unselectedLangs];

  return (
    <div className="flex flex-col gap-5">
      {/* STT Languages -- multi-select */}
      <div className="flex flex-col gap-2.5">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">
            Speech Recognition
          </div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            Select one or more languages you speak. Multiple selections enable auto-switching.
          </div>
        </div>

        {/* Selected tags */}
        {!sttSelected.includes("auto") && sttSelected.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {sttSelected.map((code) => {
              const lang = LANGUAGES.find((l) => l.code === code);
              if (!lang) return null;
              return (
                <span
                  key={code}
                  className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-[rgba(245,158,11,0.12)] text-[#fbbf24] text-[11px] font-medium"
                >
                  {lang.flag} {lang.label}
                  <button
                    onClick={() => toggleSttLang(code)}
                    className="ml-0.5 text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24]"
                  >
                    {"\u2715"}
                  </button>
                </span>
              );
            })}
          </div>
        )}

        {/* Language grid */}
        <div className="grid grid-cols-3 gap-1.5">
          {sttOrdered.map((lang) => {
            const selected = sttSelected.includes(lang.code);
            return (
              <button
                key={lang.code}
                onClick={() => toggleSttLang(lang.code)}
                className={[
                  "flex items-center gap-2 px-2.5 py-2 rounded-lg text-left transition-all",
                  selected
                    ? "bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.25)]"
                    : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)]",
                ].join(" ")}
              >
                <span className="text-[13px]">{lang.flag}</span>
                <span
                  className={[
                    "text-[10px] truncate",
                    selected ? "text-[#fbbf24] font-medium" : "text-[rgba(255,255,255,0.45)]",
                  ].join(" ")}
                >
                  {lang.label}
                </span>
              </button>
            );
          })}
        </div>

        {/* Show more / less */}
        {sttRest.length > 0 && (
          <button
            onClick={() => setShowAllStt(!showAllStt)}
            className="text-[10px] text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24] transition-colors self-start"
          >
            {showAllStt
              ? "Show less"
              : `Show ${sttRest.length} more languages...`}
          </button>
        )}
      </div>

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* Translation Target -- selected shown separately, grid stays in original order */}
      <div className="flex flex-col gap-2.5">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">
            Translate To
          </div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            Target language for Translate mode. Click to switch.
          </div>
        </div>

        {/* Current selection -- displayed separately */}
        {(() => {
          const current = TARGET_LANGUAGES.find(
            (l) => l.code === config.translate_target
          ) ?? TARGET_LANGUAGES[0];
          return (
            <div className="flex items-center gap-3 px-3.5 py-2.5 rounded-[10px] bg-[rgba(245,158,11,0.1)] border border-[rgba(245,158,11,0.2)]">
              <span className="text-[18px]">{current.flag}</span>
              <div className="flex-1">
                <div className="text-[13px] font-medium text-[#fbbf24]">
                  {current.label}
                </div>
              </div>
              <span className="text-[10px] text-[rgba(251,191,36,0.35)]">
                Selected
              </span>
            </div>
          );
        })()}

        {/* Language grid -- original order, no reordering */}
        {(() => {
          const translateFrequent = TARGET_LANGUAGES.filter((l) =>
            FREQUENT_CODES.includes(l.code)
          );
          const translateRest = TARGET_LANGUAGES.filter(
            (l) => !FREQUENT_CODES.includes(l.code)
          );
          const visible = showAllTranslate
            ? [...translateFrequent, ...translateRest]
            : translateFrequent;

          return (
            <>
              <div className="grid grid-cols-3 gap-1.5">
                {visible.map((lang) => {
                  const isSelected = config.translate_target === lang.code;
                  return (
                    <button
                      key={lang.code}
                      onClick={() => onSave({ translate_target: lang.code })}
                      className={[
                        "flex items-center gap-2 px-2.5 py-2 rounded-lg text-left transition-all",
                        isSelected
                          ? "bg-[rgba(245,158,11,0.06)] border border-[rgba(245,158,11,0.15)] opacity-60"
                          : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)]",
                      ].join(" ")}
                    >
                      <span className="text-[13px]">{lang.flag}</span>
                      <span
                        className={[
                          "text-[10px] truncate",
                          isSelected
                            ? "text-[rgba(251,191,36,0.5)]"
                            : "text-[rgba(255,255,255,0.45)]",
                        ].join(" ")}
                      >
                        {lang.label}
                      </span>
                    </button>
                  );
                })}
              </div>
              {translateRest.length > 0 && (
                <button
                  onClick={() => setShowAllTranslate(!showAllTranslate)}
                  className="text-[10px] text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24] transition-colors self-start"
                >
                  {showAllTranslate
                    ? "Show less"
                    : `Show ${translateRest.length} more languages...`}
                </button>
              )}
            </>
          );
        })()}
      </div>
    </div>
  );
}
