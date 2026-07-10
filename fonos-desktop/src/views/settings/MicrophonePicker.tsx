// Shared microphone selector — device list + "Auto Detect" + refresh.
//
// Extracted verbatim from GeneralTab so the wizard and Settings pick a mic the
// same way. Selecting a device calls onSelect(name); "auto" means Auto Detect.

import { useState, useEffect } from "react";
import { listAudioInputs } from "../../lib/api";
import { t, useT } from "../../lib/i18n";

export default function MicrophonePicker({
  value,
  onSelect,
}: {
  /** Currently selected device name; empty / "auto" means Auto Detect. */
  value: string;
  onSelect: (name: string) => void;
}) {
  useT();
  const [audioInputs, setAudioInputs] = useState<string[]>([]);

  useEffect(() => {
    listAudioInputs().then(setAudioInputs).catch(() => {});
  }, []);

  return (
    <div className="flex flex-col gap-2.5">
      <div>
        <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">{t("mic.title")}</div>
        <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
          {t("mic.desc")}
        </div>
      </div>

      <div className="flex flex-col gap-1.5">
        {audioInputs.map((name) => {
          const selected = (value || "auto") === name;
          const isAuto = name === "auto";
          return (
            <button
              key={name}
              onClick={() => onSelect(name)}
              className={[
                "flex items-center gap-3 px-3 py-2 rounded-lg text-left transition-all text-[11px]",
                selected
                  ? "bg-[rgba(242,184,75,0.08)] border border-[rgba(242,184,75,0.15)]"
                  : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)]",
              ].join(" ")}
            >
              <div className={[
                "w-1.5 h-1.5 rounded-full flex-shrink-0",
                selected ? "bg-[var(--accent)]" : "bg-[rgba(255,255,255,0.1)]",
              ].join(" ")} />
              <span className={selected ? "text-[var(--accent)]" : "text-[rgba(255,255,255,0.45)]"}>
                {isAuto ? t("mic.auto") : name}
              </span>
              {isAuto && (
                <span className="text-[8px] text-[rgba(255,255,255,0.15)] ml-auto">
                  {t("mic.prefers-external")}
                </span>
              )}
            </button>
          );
        })}
      </div>

      <button
        onClick={() => listAudioInputs().then(setAudioInputs).catch(() => {})}
        className="text-[9px] text-[rgba(242,184,75,0.4)] hover:text-[var(--accent)] transition-colors self-start"
      >
        {t("mic.refresh")}
      </button>
    </div>
  );
}
