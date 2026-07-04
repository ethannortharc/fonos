// Models tab — default service dropdowns + the shared model-profile editor.

import { useState, useEffect, useRef } from "react";
import { t, useT } from "../../lib/i18n";
import type { AppConfig, ModelProfile } from "../../types";
import ModelProfileEditor from "./ModelProfileEditor";

// ─── Default Service Card Dropdown ───────────────────────────────────────────

function ServiceCardDropdown({
  capKey,
  label,
  currentId,
  profiles,
  onSelect,
}: {
  capKey: string;
  label: string;
  currentId: string;
  profiles: ModelProfile[];
  onSelect: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const filtered = profiles.filter(
    (p) => p.capabilities && p.capabilities.includes(capKey)
  );
  const current = profiles.find((p) => p.id === currentId);

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="w-full rounded-lg bg-[rgba(255,255,255,0.025)] border border-[rgba(255,255,255,0.06)] p-3 text-left hover:border-[rgba(255,255,255,0.12)] transition-colors cursor-pointer h-[68px] flex flex-col justify-between"
      >
        <div className="text-[9px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
          {label}
        </div>
        <div>
          <div className={["text-[11px] truncate", current ? "text-[#fafaf9] font-medium" : "text-[rgba(255,255,255,0.2)] italic"].join(" ")}>
            {current ? current.name : t("models.notset")}
          </div>
          <div className="text-[9px] text-[rgba(255,255,255,0.2)] truncate mt-0.5 h-[14px]">
            {current?.model ?? ""}
          </div>
        </div>
      </button>

      {open && (
        <div className="absolute z-50 top-full left-0 right-0 mt-1 rounded-lg bg-[#252420] border border-[rgba(255,255,255,0.1)] shadow-xl overflow-hidden">
          <button
            onClick={() => { onSelect(""); setOpen(false); }}
            className={[
              "w-full px-3 py-2 text-left text-[11px] transition-colors",
              !currentId
                ? "bg-[rgba(245,158,11,0.1)] text-[#fbbf24]"
                : "text-[rgba(255,255,255,0.4)] hover:bg-[rgba(255,255,255,0.04)]",
            ].join(" ")}
          >
            {t("models.notconfigured")}
          </button>
          {filtered.map((p) => (
            <button
              key={p.id}
              onClick={() => { onSelect(p.id); setOpen(false); }}
              className={[
                "w-full px-3 py-2 text-left transition-colors",
                currentId === p.id
                  ? "bg-[rgba(245,158,11,0.1)]"
                  : "hover:bg-[rgba(255,255,255,0.04)]",
              ].join(" ")}
            >
              <div className={[
                "text-[11px]",
                currentId === p.id ? "text-[#fbbf24] font-medium" : "text-[rgba(255,255,255,0.6)]",
              ].join(" ")}>
                {p.name}
              </div>
              <div className="text-[9px] text-[rgba(255,255,255,0.2)]">
                {p.model}
              </div>
            </button>
          ))}
          {filtered.length === 0 && (
            <div className="px-3 py-2 text-[10px] text-[rgba(255,255,255,0.2)]">
              {t("models.nocap").replace("{cap}", capKey.toUpperCase())}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── ModelsTab ───────────────────────────────────────────────────────────────

export default function ModelsTab({
  config,
  onSave,
  setError,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
  setError: (e: string) => void;
}) {
  useT();
  return (
    <div className="flex flex-col gap-4">
      {/* ── Section 1: Default Services ── */}
      <div className="flex flex-col gap-2">
        <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
          {t("models.defaults")}
        </div>
        <div className="grid grid-cols-3 gap-2">
          <ServiceCardDropdown
            capKey="stt"
            label="STT"
            currentId={config.stt_profile}
            profiles={config.model_profiles}
            onSelect={(id) => onSave({ stt_profile: id })}
          />
          <ServiceCardDropdown
            capKey="tts"
            label="TTS"
            currentId={config.tts_profile}
            profiles={config.model_profiles}
            onSelect={(id) => onSave({ tts_profile: id })}
          />
          <ServiceCardDropdown
            capKey="llm"
            label="LLM"
            currentId={config.llm_profile}
            profiles={config.model_profiles}
            onSelect={(id) => onSave({ llm_profile: id })}
          />
        </div>
      </div>

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Section 2: Registered Models (shared editor) ── */}
      <ModelProfileEditor config={config} onSave={onSave} setError={setError} />
    </div>
  );
}
