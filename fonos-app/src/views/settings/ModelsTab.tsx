// Models tab — default service dropdowns, model registry list, add/edit form.

import { useState, useEffect, useRef, useCallback } from "react";
import { probeModel } from "../../lib/api";
import type { AppConfig, ModelCaps, ModelProfile } from "../../types";
import { PROVIDERS, CAP_BADGE, EMPTY_MODEL } from "./constants";
import type { ModelForm } from "./constants";

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
            {current ? current.name : "Not set"}
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
            Not configured
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
              No models with {capKey.toUpperCase()} capability
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
  const [editingProfile, setEditingProfile] = useState<ModelForm | null>(null);
  const [addingNew, setAddingNew] = useState<boolean>(false);
  const [providerPicked, setProviderPicked] = useState<boolean>(false);
  const [modelCaps, setModelCaps] = useState<ModelCaps | null>(null);
  const [probingModel, setProbingModel] = useState<boolean>(false);
  const [saving, setSaving] = useState<boolean>(false);

  const handleProbeModel = useCallback(async () => {
    setProbingModel(true);
    setError("");
    try {
      const caps = await probeModel();
      setModelCaps(caps);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setProbingModel(false);
    }
  }, [setError]);

  const startAddModel = () => {
    setEditingProfile({ ...EMPTY_MODEL });
    setAddingNew(true);
    setProviderPicked(false);
  };

  const startEditModel = (p: ModelProfile) => {
    setEditingProfile({
      id: p.id,
      name: p.name,
      provider: p.provider,
      model: p.model,
      api_key: p.api_key ?? "",
      base_url: p.base_url ?? "",
      capabilities: p.capabilities ? [...p.capabilities] : [],
      stt_api: p.stt_api ?? "whisper",
    });
    setAddingNew(false);
    setProviderPicked(true);
  };

  const pickProvider = (providerId: string) => {
    if (!editingProfile) return;
    const prov = PROVIDERS.find((p) => p.id === providerId);
    setEditingProfile({
      ...editingProfile,
      provider: providerId,
      base_url: prov?.url ?? "",
    });
    setProviderPicked(true);
  };

  const toggleCapability = (cap: string) => {
    if (!editingProfile) return;
    const caps = editingProfile.capabilities.includes(cap)
      ? editingProfile.capabilities.filter((c) => c !== cap)
      : [...editingProfile.capabilities, cap];
    setEditingProfile({ ...editingProfile, capabilities: caps });
  };

  const handleSaveProfile = async () => {
    if (!editingProfile) return;
    if (!editingProfile.name || !editingProfile.model) {
      setError("Name and Model ID are required");
      return;
    }
    setError("");
    setSaving(true);
    try {
      let profiles: ModelProfile[];
      const stt_api_val = editingProfile.capabilities.includes("stt") && editingProfile.stt_api === "chat" ? "chat" as const : undefined;
      if (addingNew) {
        const newProfile: ModelProfile = {
          id: `${editingProfile.provider}-${Date.now()}`,
          name: editingProfile.name,
          provider: editingProfile.provider,
          model: editingProfile.model,
          api_key: editingProfile.api_key || undefined,
          base_url: editingProfile.base_url || undefined,
          capabilities: editingProfile.capabilities.length > 0 ? editingProfile.capabilities : undefined,
          stt_api: stt_api_val,
        };
        profiles = [...config.model_profiles, newProfile];
      } else {
        profiles = config.model_profiles.map((p) =>
          p.id === editingProfile.id
            ? {
                id: editingProfile.id,
                name: editingProfile.name,
                provider: editingProfile.provider,
                model: editingProfile.model,
                api_key: editingProfile.api_key || undefined,
                base_url: editingProfile.base_url || undefined,
                capabilities: editingProfile.capabilities.length > 0 ? editingProfile.capabilities : undefined,
                stt_api: stt_api_val,
              }
            : p
        );
      }
      onSave({ model_profiles: profiles });
      setEditingProfile(null);
      setAddingNew(false);
      setProviderPicked(false);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleDeleteProfile = async (profileId: string) => {
    setError("");
    const profiles = config.model_profiles.filter((p) => p.id !== profileId);
    onSave({ model_profiles: profiles });
  };

  const cancelModelEdit = () => {
    setEditingProfile(null);
    setAddingNew(false);
    setProviderPicked(false);
  };

  return (
    <div className="flex flex-col gap-4">
      {/* ── Section 1: Default Services ── */}
      <div className="flex flex-col gap-2">
        <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
          Default Services
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

      {/* ── Section 2: Registered Models ── */}
      <div className="flex flex-col gap-2">
        <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
          Registered Models
        </div>

        {editingProfile ? (
          /* ── Add / Edit form ── */
          <div className="flex flex-col gap-3">
            <span className="text-[rgba(255,255,255,0.4)] text-xs uppercase tracking-wider">
              {addingNew ? "Add Model Profile" : "Edit Model Profile"}
            </span>

            {/* Provider picker (shown first when adding, before provider is picked) */}
            {addingNew && !providerPicked && (
              <div className="grid grid-cols-3 gap-2">
                {PROVIDERS.map((prov) => (
                  <button
                    key={prov.id}
                    onClick={() => pickProvider(prov.id)}
                    className="px-3 py-2 rounded-lg bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] text-[12px] text-[rgba(255,255,255,0.5)] hover:border-[rgba(255,255,255,0.1)] cursor-pointer transition-colors"
                  >
                    {prov.label}
                  </button>
                ))}
              </div>
            )}

            {/* Form fields (after provider picked, or always when editing) */}
            {providerPicked && (
              <div className="flex flex-col gap-3">
                <div className="flex flex-col gap-2">
                  <label className="text-[rgba(255,255,255,0.4)] text-[11px]">Name</label>
                  <input
                    type="text"
                    value={editingProfile.name}
                    onChange={(e) =>
                      setEditingProfile({ ...editingProfile, name: e.target.value })
                    }
                    placeholder="e.g. GPT-4o"
                    className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                  />
                </div>

                <div className="flex flex-col gap-2">
                  <label className="text-[rgba(255,255,255,0.4)] text-[11px]">Model ID</label>
                  <input
                    type="text"
                    value={editingProfile.model}
                    onChange={(e) =>
                      setEditingProfile({ ...editingProfile, model: e.target.value })
                    }
                    placeholder="e.g. gpt-4o"
                    className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono"
                  />
                </div>

                <div className="flex flex-col gap-2">
                  <label className="text-[rgba(255,255,255,0.4)] text-[11px]">API Key</label>
                  <input
                    type="password"
                    value={editingProfile.api_key}
                    onChange={(e) =>
                      setEditingProfile({ ...editingProfile, api_key: e.target.value })
                    }
                    placeholder="sk-..."
                    className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono"
                  />
                </div>

                <div className="flex flex-col gap-2">
                  <label className="text-[rgba(255,255,255,0.4)] text-[11px]">Base URL</label>
                  <input
                    type="text"
                    value={editingProfile.base_url}
                    onChange={(e) =>
                      setEditingProfile({ ...editingProfile, base_url: e.target.value })
                    }
                    placeholder="https://api.example.com"
                    className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono"
                  />
                </div>

                <div className="flex flex-col gap-2">
                  <label className="text-[rgba(255,255,255,0.4)] text-[11px]">Capabilities</label>
                  <div className="flex gap-4 text-[12px] text-[rgba(255,255,255,0.5)]">
                    {(["stt", "tts", "llm"] as const).map((cap) => (
                      <label key={cap} className="flex items-center gap-1.5">
                        <input
                          type="checkbox"
                          checked={editingProfile.capabilities.includes(cap)}
                          onChange={() => toggleCapability(cap)}
                          className="accent-[#fbbf24]"
                        />
                        {cap.toUpperCase()}
                      </label>
                    ))}
                  </div>
                </div>

                {/* STT API path selector — shown when STT capability is enabled */}
                {editingProfile.capabilities.includes("stt") && (
                  <div className="flex flex-col gap-2">
                    <label className="text-[rgba(255,255,255,0.4)] text-[11px]">STT API</label>
                    <div className="flex gap-2">
                      {([["whisper", "Whisper (multipart)"], ["chat", "Chat Completions (base64)"]] as const).map(([val, label]) => (
                        <button
                          key={val}
                          onClick={() => setEditingProfile({ ...editingProfile, stt_api: val })}
                          className={[
                            "flex-1 px-3 py-1.5 rounded-lg text-[11px] border transition-colors",
                            editingProfile.stt_api === val
                              ? "bg-[rgba(245,158,11,0.1)] border-[rgba(245,158,11,0.3)] text-[#fbbf24]"
                              : "bg-[rgba(255,255,255,0.02)] border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.35)] hover:border-[rgba(255,255,255,0.1)]",
                          ].join(" ")}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                    <div className="text-[9px] text-[rgba(255,255,255,0.2)]">
                      {editingProfile.stt_api === "whisper"
                        ? "Standard Whisper API (/v1/audio/transcriptions) — OpenAI, local Whisper servers"
                        : "Send base64 audio via chat completions — OpenRouter, Gemini, Voxtral"}
                    </div>
                  </div>
                )}

                <div className="flex gap-2 mt-1">
                  <button
                    onClick={handleSaveProfile}
                    className="flex-1 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-white text-[12px] font-medium hover:opacity-90 transition-opacity"
                  >
                    {addingNew ? "Add Profile" : "Save Changes"}
                  </button>
                  <button
                    onClick={cancelModelEdit}
                    className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}

            {/* Cancel from provider pick step */}
            {addingNew && !providerPicked && (
              <button
                onClick={cancelModelEdit}
                className="mt-1 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
              >
                Cancel
              </button>
            )}
          </div>
        ) : (
          /* ── List view ── */
          <div className="flex flex-col gap-1.5">
            {config.model_profiles.length === 0 ? (
              <div className="py-6 text-center text-[rgba(255,255,255,0.25)] text-[12px]">
                No models configured
              </div>
            ) : (
              config.model_profiles.map((p) => {
                // Determine if this profile is a current default
                const defaultBadges: string[] = [];
                if (p.id === config.stt_profile) defaultBadges.push("Default STT");
                if (p.id === config.tts_profile) defaultBadges.push("Default TTS");
                if (p.id === config.llm_profile) defaultBadges.push("Default LLM");

                return (
                  <div
                    key={p.id}
                    className="flex items-center justify-between px-3 py-2.5 rounded-lg bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)] transition-colors"
                  >
                    <div className="flex flex-col gap-0.5">
                      <span className="text-[12px] font-medium text-[#fafaf9]">
                        {p.name}
                      </span>
                      <span className="text-[10px] text-[rgba(255,255,255,0.25)]">
                        {p.provider}{p.model ? ` \u00B7 ${p.model}` : ""}{p.base_url ? ` \u00B7 ${p.base_url}` : ""}
                      </span>
                    </div>
                    <div className="flex items-center gap-1.5">
                      {defaultBadges.map((badge) => (
                        <span
                          key={badge}
                          className="px-1.5 py-0.5 rounded text-[9px] font-medium bg-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)]"
                        >
                          {badge}
                        </span>
                      ))}
                      {p.capabilities?.map((cap) => (
                        <span
                          key={cap}
                          className={`px-1.5 py-0.5 rounded text-[9px] font-medium uppercase tracking-wide ${CAP_BADGE[cap] ?? "bg-[rgba(255,255,255,0.05)] text-[rgba(255,255,255,0.4)]"}`}
                        >
                          {cap}
                        </span>
                      ))}
                      <button
                        onClick={() => startEditModel(p)}
                        className="text-[rgba(255,255,255,0.25)] hover:text-[rgba(255,255,255,0.5)] text-[11px] px-1.5 transition-colors"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => handleDeleteProfile(p.id)}
                        className="text-[rgba(255,255,255,0.15)] hover:text-red-400 text-[11px] px-1 transition-colors"
                      >
                        {"\u2715"}
                      </button>
                    </div>
                  </div>
                );
              })
            )}

            {/* Model caps probe */}
            <div className="flex items-center gap-2 mt-1">
              <button
                onClick={handleProbeModel}
                disabled={probingModel}
                className="px-3 py-1.5 bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[rgba(255,255,255,0.4)] text-[11px] rounded-lg transition-colors disabled:opacity-40"
              >
                {probingModel ? "Probing..." : "Probe Capabilities"}
              </button>
              {modelCaps && (
                <span className="text-[rgba(255,255,255,0.25)] text-[10px]">
                  sys:{modelCaps.follows_system_prompt ? "\u2713" : "\u2717"} lang:
                  {modelCaps.preserves_language ? "\u2713" : "\u2717"}
                </span>
              )}
            </div>

            {/* Add button */}
            <button
              onClick={startAddModel}
              className="w-full mt-1 py-2 rounded-lg bg-transparent border border-dashed border-[rgba(245,158,11,0.12)] text-[rgba(251,191,36,0.6)] text-[12px] hover:border-[rgba(245,158,11,0.25)] transition-colors"
            >
              + Add Model
            </button>
          </div>
        )}
      </div>

      {saving && (
        <div className="text-center text-[rgba(255,255,255,0.25)] text-[10px] mt-1">
          Saving...
        </div>
      )}
    </div>
  );
}
