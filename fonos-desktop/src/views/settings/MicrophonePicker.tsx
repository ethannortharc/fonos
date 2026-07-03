// Shared microphone selector — device list + "Auto Detect" + refresh.
//
// Extracted verbatim from GeneralTab so the wizard and Settings pick a mic the
// same way. Selecting a device calls onSelect(name); "auto" means Auto Detect.

import { useState, useEffect } from "react";
import { listAudioInputs } from "../../lib/api";

export default function MicrophonePicker({
  value,
  onSelect,
}: {
  /** Currently selected device name; empty / "auto" means Auto Detect. */
  value: string;
  onSelect: (name: string) => void;
}) {
  const [audioInputs, setAudioInputs] = useState<string[]>([]);

  useEffect(() => {
    listAudioInputs().then(setAudioInputs).catch(() => {});
  }, []);

  return (
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
          const selected = (value || "auto") === name;
          const isAuto = name === "auto";
          return (
            <button
              key={name}
              onClick={() => onSelect(name)}
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
  );
}
