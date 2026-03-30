// General settings — microphone selection + speech recognition language.

import { useState, useEffect } from "react";
import type { AppConfig } from "../../types";
import { listAudioInputs } from "../../lib/api";
import { LANGUAGES } from "./constants";

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

  useEffect(() => {
    listAudioInputs().then(setAudioInputs).catch(() => {});
  }, []);

  // Parse multi-select STT languages
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

  const sttFrequent = LANGUAGES.filter((l) => FREQUENT_CODES.includes(l.code));
  const sttRest = LANGUAGES.filter((l) => !FREQUENT_CODES.includes(l.code));
  const sttVisible = showAllLangs ? [...sttFrequent, ...sttRest] : sttFrequent;

  const autoLang = LANGUAGES.find((l) => l.code === "auto")!;
  const selectedLangs = sttVisible.filter((l) => sttSelected.includes(l.code) && l.code !== "auto");
  const unselectedLangs = sttVisible.filter((l) => !sttSelected.includes(l.code) && l.code !== "auto");
  const sttOrdered = [autoLang, ...selectedLangs, ...unselectedLangs];

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

        <div className="flex items-center gap-2">
          <select
            value={config.audio_input_device || "auto"}
            onChange={(e) => onSave({ audio_input_device: e.target.value })}
            className="flex-1 bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2.5 text-[12px] text-[#fafaf9] focus:outline-none focus:border-[rgba(245,158,11,0.3)] cursor-pointer"
          >
            {audioInputs.map((name) => (
              <option key={name} value={name}>
                {name === "auto" ? "Auto Detect" : name}
              </option>
            ))}
          </select>
          <button
            onClick={() => listAudioInputs().then(setAudioInputs).catch(() => {})}
            className="px-3 py-2.5 rounded-lg bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[rgba(255,255,255,0.4)] text-[10px] transition-colors flex-shrink-0"
          >
            Refresh
          </button>
        </div>

        {config.audio_input_device && config.audio_input_device !== "auto" && config.audio_input_device !== "default" && (
          <div className="text-[9px] text-[rgba(255,255,255,0.2)]">
            Locked to "{config.audio_input_device}". If disconnected, recording will show an error.
          </div>
        )}
      </div>

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Speech Recognition Language ── */}
      <div className="flex flex-col gap-2.5">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">Speech Recognition</div>
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
                <span key={code}
                  className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-[rgba(245,158,11,0.12)] text-[#fbbf24] text-[11px] font-medium">
                  {lang.flag} {lang.label}
                  <button onClick={() => toggleSttLang(code)}
                    className="ml-0.5 text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24]">{"\u2715"}</button>
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
              <button key={lang.code} onClick={() => toggleSttLang(lang.code)}
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
    </div>
  );
}
