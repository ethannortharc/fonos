// Models tab — default service dropdowns + the shared model-profile editor.
// Saved configuration bundles and the setup templates now live in the dedicated
// Settings › Scenarios tab.

import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
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
                ? "bg-[rgba(242,184,75,0.1)] text-[var(--accent)]"
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
                  ? "bg-[rgba(242,184,75,0.1)]"
                  : "hover:bg-[rgba(255,255,255,0.04)]",
              ].join(" ")}
            >
              <div className={[
                "text-[11px]",
                currentId === p.id ? "text-[var(--accent)] font-medium" : "text-[rgba(255,255,255,0.6)]",
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
        <div className="flex items-center">
          <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
            {t("models.defaults")}
          </div>
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

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Section 3: Local models (on-device, e.g. meeting diarization) ── */}
      <LocalModelsSection config={config} onSave={onSave} />
    </div>
  );
}

// ─── Local models (on-device downloads) ──────────────────────────────────────
// Currently just the meeting speaker-separation (diarization) CoreML model
// from T4's fonos-diarize sidecar — download button + progress + an optional
// HuggingFace mirror endpoint, the settings-side surface for the diarize_check/
// diarize_download_models commands and the "diarize:download" progress event
// (all frozen by Task 4). State machine modeled on GeneralTab's UpdatesSection.

type DiarDl =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "ready" }
  | { kind: "downloading"; pct: number }
  | { kind: "error"; message: string };

function LocalModelsSection({
  config, onSave,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  const t = useT();
  const [st, setSt] = useState<DiarDl>({ kind: "checking" });
  const [mirror, setMirror] = useState(config.diarization_hf_endpoint ?? "");

  // Subscribe to download progress + probe current status on mount.
  // StrictMode double-mount safe (mirrors TestRunSection's bench:event
  // subscription): a `disposed` guard drops a `listen()` that resolves after
  // this effect's cleanup already ran, so no listener leaks.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const un = await listen<string>("diarize:download", (e) => {
        try {
          const p = JSON.parse(e.payload);
          if (p.kind === "progress") setSt({ kind: "downloading", pct: p.pct });
          else if (p.kind === "done") setSt({ kind: "ready" });
          else if (p.kind === "error") setSt({ kind: "error", message: p.message });
        } catch {
          // malformed payload — ignore
        }
      });
      if (disposed) { un(); return; }
      unlisten = un;
    })();

    invoke<{ available: boolean; models_present: boolean }>("diarize_check")
      .then((s) => setSt(s.models_present ? { kind: "ready" } : { kind: "idle" }))
      .catch(() => setSt({ kind: "idle" }));

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const startDownload = () => {
    setSt({ kind: "downloading", pct: 0 });
    invoke("diarize_download_models").catch((e) => setSt({ kind: "error", message: String(e) }));
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
        {t("models.local.title")}
      </div>
      <div className="flex items-center gap-3">
        <div className="text-[12px] text-[rgba(255,255,255,0.6)] flex-1">
          {t("models.local.diarize.name")}
        </div>
        {st.kind === "ready" && (
          <div className="text-[11px] text-[rgba(134,239,172,0.8)]">{t("models.local.ready")}</div>
        )}
        {st.kind === "downloading" && (
          <div className="text-[11px] text-[rgba(255,255,255,0.5)]">
            {t("models.local.downloading").replace("{n}", String(st.pct))}
          </div>
        )}
        {st.kind === "error" && <div className="text-[11px] text-red-400/80">{st.message}</div>}
        {(st.kind === "idle" || st.kind === "error") && (
          <button
            onClick={startDownload}
            className="px-2.5 py-1.5 rounded-lg text-[11px] bg-[rgba(242,184,75,0.1)] text-[var(--accent)] border border-[rgba(242,184,75,0.2)] hover:bg-[rgba(242,184,75,0.16)]"
          >
            {t("models.local.download")}
          </button>
        )}
      </div>
      <input
        type="text"
        value={mirror}
        onChange={(e) => setMirror(e.target.value)}
        onBlur={() => onSave({ diarization_hf_endpoint: mirror })}
        placeholder={t("models.local.mirror-ph")}
        className="w-full px-2.5 py-2 rounded-lg text-[11px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.7)] placeholder:text-[rgba(255,255,255,0.2)] focus:outline-none focus:border-[rgba(242,184,75,0.25)]"
      />
    </div>
  );
}
