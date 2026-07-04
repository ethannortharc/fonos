// Shared "Registered Models" editor — the add / edit / probe / batch-add flow.
//
// Extracted verbatim from ModelsTab so the model-profile shape and logic stay
// in ONE place and can't drift. Used by:
//   • ModelsTab  — the Settings › Models "Registered Models" section.
//   • Onboarding — the first-run wizard's backend step.
//
// Behaviour is identical to the original ModelsTab implementation. Two optional
// props layer on wizard-only conveniences without changing Settings behaviour:
//   • onProfilesAdded — fired with the newly-saved profile(s) so a caller can
//                       assign default services (wizard sets stt_profile/llm_profile).
//   • startInAddMode  — auto-open the "Add Model" provider picker on mount when
//                       no profiles exist yet (wizard first-run).

import { useState, useEffect, useRef, useCallback } from "react";
import { listProviderModels, testStt } from "../../lib/api";
import { t, useT } from "../../lib/i18n";
import type { AppConfig, ModelProfile } from "../../types";
import { PROVIDERS, CAP_BADGE, EMPTY_MODEL } from "./constants";
import type { ModelForm } from "./constants";

export default function ModelProfileEditor({
  config,
  onSave,
  setError,
  onProfilesAdded,
  startInAddMode,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
  setError: (e: string) => void;
  /** Fired with the profile(s) just added/saved (wizard uses it to set defaults). */
  onProfilesAdded?: (added: ModelProfile[]) => void;
  /** Auto-open the add-model provider picker on mount when the list is empty. */
  startInAddMode?: boolean;
}) {
  useT();
  const [editingProfile, setEditingProfile] = useState<ModelForm | null>(null);
  const [addingNew, setAddingNew] = useState<boolean>(false);
  const [providerPicked, setProviderPicked] = useState<boolean>(false);
  const [probedModels, setProbedModels] = useState<{ id: string; owned_by: string; caps: string[]; checked: boolean; stt_api: "whisper" | "chat" }[]>([]);
  const [probingModels, setProbingModels] = useState<boolean>(false);
  const [saving, setSaving] = useState<boolean>(false);
  const [sttTest, setSttTest] = useState<{ id: string; status: "testing" | "ok" | "err"; msg: string } | null>(null);

  // Send a silent probe clip to a model's STT endpoint to confirm it works.
  const handleTestStt = async (id: string) => {
    setSttTest({ id, status: "testing", msg: "" });
    try {
      const msg = await testStt(id);
      setSttTest({ id, status: "ok", msg });
    } catch (e: unknown) {
      setSttTest({ id, status: "err", msg: e instanceof Error ? e.message : String(e) });
    }
  };

  // Auto-detect capabilities from model name
  const guessCaps = (id: string): string[] => {
    const l = id.toLowerCase();
    if (/asr|whisper|transcri|stt|voxtral/.test(l)) return ["stt"];
    if (/tts|speech|voice|audio-out/.test(l)) return ["tts"];
    return ["llm"];
  };
  const guessSttApi = (_id: string, provider: string): "whisper" | "chat" => {
    if (provider === "openrouter") return "chat";
    return "whisper";
  };

  const handleProbeModels = useCallback(async () => {
    if (!editingProfile) return;
    setProbingModels(true);
    setError("");
    try {
      const models = await listProviderModels(editingProfile.base_url, editingProfile.api_key);
      // Filter out models already registered
      const existingIds = new Set(config.model_profiles.map((p) => p.model));
      setProbedModels(models.map((m) => ({
        ...m,
        caps: guessCaps(m.id),
        checked: !existingIds.has(m.id),
        stt_api: guessSttApi(m.id, editingProfile.provider),
      })));
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
      setProbedModels([]);
    } finally {
      setProbingModels(false);
    }
  }, [editingProfile, config.model_profiles, setError]);

  const startAddModel = () => {
    setEditingProfile({ ...EMPTY_MODEL });
    setAddingNew(true);
    setProviderPicked(false);
    setProbedModels([]);
  };

  // Wizard convenience: jump straight to the provider picker on first run when
  // nothing is configured yet. Opt-in via startInAddMode; ModelsTab never sets it.
  const autoStarted = useRef(false);
  useEffect(() => {
    if (startInAddMode && !autoStarted.current && config.model_profiles.length === 0) {
      autoStarted.current = true;
      startAddModel();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [startInAddMode, config.model_profiles.length]);

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
      setError(t("mprof.err-name-model"));
      return;
    }
    setError("");
    setSaving(true);
    try {
      let profiles: ModelProfile[];
      let saved: ModelProfile;
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
        saved = newProfile;
        profiles = [...config.model_profiles, newProfile];
      } else {
        const updated: ModelProfile = {
          id: editingProfile.id,
          name: editingProfile.name,
          provider: editingProfile.provider,
          model: editingProfile.model,
          api_key: editingProfile.api_key || undefined,
          base_url: editingProfile.base_url || undefined,
          capabilities: editingProfile.capabilities.length > 0 ? editingProfile.capabilities : undefined,
          stt_api: stt_api_val,
        };
        saved = updated;
        profiles = config.model_profiles.map((p) =>
          p.id === editingProfile.id ? updated : p
        );
      }
      onSave({ model_profiles: profiles });
      onProfilesAdded?.([saved]);
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

  const handleBatchAdd = () => {
    if (!editingProfile) return;
    const selected = probedModels.filter((m) => m.checked);
    if (selected.length === 0) return;
    const newProfiles: ModelProfile[] = selected.map((m) => ({
      id: `${editingProfile.provider}-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      name: m.id,
      provider: editingProfile.provider,
      model: m.id,
      api_key: editingProfile.api_key || undefined,
      base_url: editingProfile.base_url || undefined,
      capabilities: m.caps.length > 0 ? m.caps : undefined,
      stt_api: m.caps.includes("stt") && m.stt_api === "chat" ? "chat" as const : undefined,
    }));
    onSave({ model_profiles: [...config.model_profiles, ...newProfiles] });
    onProfilesAdded?.(newProfiles);
    setEditingProfile(null);
    setAddingNew(false);
    setProviderPicked(false);
    setProbedModels([]);
  };

  const cancelModelEdit = () => {
    setEditingProfile(null);
    setAddingNew(false);
    setProviderPicked(false);
    setProbedModels([]);
  };

  return (
    <>
      {/* ── Registered Models ── */}
      <div className="flex flex-col gap-2">
        <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
          {t("mprof.registered")}
        </div>

        {editingProfile ? (
          /* ── Add / Edit form ── */
          <div className="flex flex-col gap-3">
            <span className="text-[rgba(255,255,255,0.4)] text-xs uppercase tracking-wider">
              {addingNew ? t("mprof.add-profile") : t("mprof.edit-profile")}
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

            {/* Form: after provider picked when ADDING */}
            {addingNew && providerPicked && (
              <div className="flex flex-col gap-3">
                {/* Connection fields */}
                <div className="grid grid-cols-2 gap-2">
                  <div className="flex flex-col gap-1">
                    <label className="text-[rgba(255,255,255,0.4)] text-[9px]">API Key</label>
                    <input type="password" value={editingProfile.api_key}
                      onChange={(e) => setEditingProfile({ ...editingProfile, api_key: e.target.value })}
                      placeholder="sk-..." className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-1.5 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono" />
                  </div>
                  <div className="flex flex-col gap-1">
                    <label className="text-[rgba(255,255,255,0.4)] text-[9px]">{t("mprof.base-url")}</label>
                    <input type="text" value={editingProfile.base_url}
                      onChange={(e) => setEditingProfile({ ...editingProfile, base_url: e.target.value })}
                      placeholder="https://..." className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-1.5 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono" />
                  </div>
                </div>

                {/* Probe button */}
                <button onClick={handleProbeModels} disabled={probingModels || !editingProfile.base_url}
                  className="w-full py-2 rounded-lg bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[rgba(255,255,255,0.5)] text-[11px] transition-colors disabled:opacity-30">
                  {probingModels ? t("mprof.probing") : t("mprof.probe")}
                </button>

                {/* Model list with checkboxes */}
                {probedModels.length > 0 && (
                  <div className="flex flex-col gap-1.5">
                    {/* Select all / Unselect all */}
                    <div className="flex items-center justify-between">
                      <span className="text-[10px] text-[rgba(255,255,255,0.25)]">{t("mprof.models-found").replace("{n}", String(probedModels.length))}</span>
                      <button onClick={() => {
                        const allChecked = probedModels.every((m) => m.checked);
                        setProbedModels(probedModels.map((m) => ({ ...m, checked: !allChecked })));
                      }}
                        className="text-[9px] text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24] transition-colors">
                        {probedModels.every((m) => m.checked) ? t("mprof.unselect-all") : t("mprof.select-all")}
                      </button>
                    </div>
                  <div className="flex flex-col gap-1 max-h-[240px] overflow-y-auto">
                    {probedModels.map((m, i) => {
                      const existingIds = new Set(config.model_profiles.map((p) => p.model));
                      const alreadyAdded = existingIds.has(m.id);
                      return (
                        <div key={m.id} className={[
                          "flex items-center gap-2 px-2.5 py-2 rounded-lg transition-colors",
                          m.checked && !alreadyAdded ? "bg-[rgba(245,158,11,0.05)] border border-[rgba(245,158,11,0.1)]" : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.03)]",
                        ].join(" ")}>
                          <input type="checkbox" checked={m.checked && !alreadyAdded} disabled={alreadyAdded}
                            onChange={() => {
                              const updated = [...probedModels];
                              updated[i] = { ...m, checked: !m.checked };
                              setProbedModels(updated);
                            }}
                            className="accent-[#fbbf24] flex-shrink-0" />
                          <span className={["text-[11px] font-mono flex-1 truncate", alreadyAdded ? "text-[rgba(255,255,255,0.15)] line-through" : "text-[rgba(255,255,255,0.6)]"].join(" ")}>{m.id}</span>
                          {/* Capability toggles */}
                          <div className="flex gap-1 flex-shrink-0">
                            {(["stt", "tts", "llm"] as const).map((cap) => (
                              <button key={cap} onClick={() => {
                                const updated = [...probedModels];
                                const caps = m.caps.includes(cap) ? m.caps.filter((c) => c !== cap) : [...m.caps, cap];
                                updated[i] = { ...m, caps };
                                setProbedModels(updated);
                              }}
                                className={["px-1.5 py-0.5 rounded text-[8px] font-medium uppercase transition-colors",
                                  m.caps.includes(cap)
                                    ? CAP_BADGE[cap] ?? "bg-[rgba(255,255,255,0.08)] text-[rgba(255,255,255,0.5)]"
                                    : "bg-transparent text-[rgba(255,255,255,0.1)] hover:text-[rgba(255,255,255,0.3)]",
                                ].join(" ")}>{cap}</button>
                            ))}
                          </div>
                          {alreadyAdded && <span className="text-[8px] text-[rgba(255,255,255,0.15)]">{t("mprof.added")}</span>}
                        </div>
                      );
                    })}
                  </div>
                  </div>
                )}

                {/* Action buttons */}
                <div className="flex gap-2">
                  {probedModels.filter((m) => m.checked && !config.model_profiles.some((p) => p.model === m.id)).length > 0 && (
                    <button onClick={handleBatchAdd}
                      className="flex-1 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-white text-[12px] font-medium hover:opacity-90 transition-opacity">
                      {(() => {
                        const n = probedModels.filter((m) => m.checked && !config.model_profiles.some((p) => p.model === m.id)).length;
                        return (n > 1 ? t("mprof.add-n-models") : t("mprof.add-n-model")).replace("{n}", String(n));
                      })()}
                    </button>
                  )}
                  <button onClick={cancelModelEdit}
                    className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors">
                    {t("common.cancel")}
                  </button>
                </div>

                {/* Manual add fallback */}
                {probedModels.length === 0 && !probingModels && (
                  <div className="flex flex-col gap-2 border-t border-[rgba(255,255,255,0.04)] pt-3">
                    <span className="text-[9px] text-[rgba(255,255,255,0.2)]">{t("mprof.or-manual")}</span>
                    <div className="grid grid-cols-2 gap-2">
                      <input type="text" value={editingProfile.name}
                        onChange={(e) => setEditingProfile({ ...editingProfile, name: e.target.value })}
                        placeholder={t("mprof.name")} className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-1.5 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]" />
                      <input type="text" value={editingProfile.model}
                        onChange={(e) => setEditingProfile({ ...editingProfile, model: e.target.value })}
                        placeholder={t("mprof.model-id")} className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-1.5 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono" />
                    </div>
                    <div className="flex gap-4 text-[11px] text-[rgba(255,255,255,0.5)]">
                      {(["stt", "tts", "llm"] as const).map((cap) => (
                        <label key={cap} className="flex items-center gap-1.5">
                          <input type="checkbox" checked={editingProfile.capabilities.includes(cap)} onChange={() => toggleCapability(cap)} className="accent-[#fbbf24]" />
                          {cap.toUpperCase()}
                        </label>
                      ))}
                    </div>
                    {editingProfile.capabilities.includes("stt") && (
                      <div className="flex gap-2">
                        {([["whisper", "Whisper"], ["chat", "Chat Completions"]] as const).map(([val, label]) => (
                          <button key={val} onClick={() => setEditingProfile({ ...editingProfile, stt_api: val })}
                            className={["flex-1 px-2 py-1 rounded-lg text-[10px] border transition-colors",
                              editingProfile.stt_api === val ? "bg-[rgba(245,158,11,0.1)] border-[rgba(245,158,11,0.3)] text-[#fbbf24]" : "bg-[rgba(255,255,255,0.02)] border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.3)]",
                            ].join(" ")}>{label}</button>
                        ))}
                      </div>
                    )}
                    <button onClick={handleSaveProfile}
                      className="py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-white text-[12px] font-medium hover:opacity-90">
                      {t("mprof.add-profile-btn")}
                    </button>
                  </div>
                )}
              </div>
            )}

            {/* Form: EDITING existing profile */}
            {!addingNew && providerPicked && (
              <div className="flex flex-col gap-3">
                <div className="flex flex-col gap-2">
                  <label className="text-[rgba(255,255,255,0.4)] text-[11px]">{t("mprof.name")}</label>
                  <input type="text" value={editingProfile.name}
                    onChange={(e) => setEditingProfile({ ...editingProfile, name: e.target.value })}
                    placeholder={t("mprof.ph.name-eg")} className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]" />
                </div>
                <div className="flex flex-col gap-2">
                  <label className="text-[rgba(255,255,255,0.4)] text-[11px]">{t("mprof.model-id")}</label>
                  <input type="text" value={editingProfile.model}
                    onChange={(e) => setEditingProfile({ ...editingProfile, model: e.target.value })}
                    placeholder={t("mprof.ph.model-eg")} className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono" />
                </div>
                <div className="flex flex-col gap-2">
                  <label className="text-[rgba(255,255,255,0.4)] text-[11px]">API Key</label>
                  <input type="password" value={editingProfile.api_key}
                    onChange={(e) => setEditingProfile({ ...editingProfile, api_key: e.target.value })}
                    placeholder="sk-..." className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono" />
                </div>
                <div className="flex flex-col gap-2">
                  <label className="text-[rgba(255,255,255,0.4)] text-[11px]">{t("mprof.base-url")}</label>
                  <input type="text" value={editingProfile.base_url}
                    onChange={(e) => setEditingProfile({ ...editingProfile, base_url: e.target.value })}
                    placeholder="https://..." className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono" />
                </div>
                <div className="flex gap-4 text-[12px] text-[rgba(255,255,255,0.5)]">
                  {(["stt", "tts", "llm"] as const).map((cap) => (
                    <label key={cap} className="flex items-center gap-1.5">
                      <input type="checkbox" checked={editingProfile.capabilities.includes(cap)} onChange={() => toggleCapability(cap)} className="accent-[#fbbf24]" />
                      {cap.toUpperCase()}
                    </label>
                  ))}
                </div>
                {editingProfile.capabilities.includes("stt") && (
                  <div className="flex gap-2">
                    {([["whisper", "Whisper (multipart)"], ["chat", "Chat Completions (base64)"]] as const).map(([val, label]) => (
                      <button key={val} onClick={() => setEditingProfile({ ...editingProfile, stt_api: val })}
                        className={["flex-1 px-3 py-1.5 rounded-lg text-[11px] border transition-colors",
                          editingProfile.stt_api === val ? "bg-[rgba(245,158,11,0.1)] border-[rgba(245,158,11,0.3)] text-[#fbbf24]" : "bg-[rgba(255,255,255,0.02)] border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.35)]",
                        ].join(" ")}>{label}</button>
                    ))}
                  </div>
                )}
                <div className="flex gap-2 mt-1">
                  <button onClick={handleSaveProfile} className="flex-1 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-white text-[12px] font-medium hover:opacity-90">{t("mprof.save-changes")}</button>
                  <button onClick={cancelModelEdit} className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)]">{t("common.cancel")}</button>
                </div>
              </div>
            )}

            {/* Cancel from provider pick step */}
            {addingNew && !providerPicked && (
              <button onClick={cancelModelEdit}
                className="mt-1 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors">
                {t("common.cancel")}
              </button>
            )}
          </div>
        ) : (
          /* ── List view ── */
          <div className="flex flex-col gap-1.5">
            {config.model_profiles.length === 0 ? (
              <div className="py-6 text-center text-[rgba(255,255,255,0.25)] text-[12px]">
                {t("mprof.none")}
              </div>
            ) : (
              config.model_profiles.map((p) => {
                // Determine if this profile is a current default
                const defaultBadges: string[] = [];
                if (p.id === config.stt_profile) defaultBadges.push(t("mprof.default-badge").replace("{svc}", "STT"));
                if (p.id === config.tts_profile) defaultBadges.push(t("mprof.default-badge").replace("{svc}", "TTS"));
                if (p.id === config.llm_profile) defaultBadges.push(t("mprof.default-badge").replace("{svc}", "LLM"));

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
                        {p.provider}{p.model ? ` · ${p.model}` : ""}{p.base_url ? ` · ${p.base_url}` : ""}
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
                      {p.capabilities?.includes("stt") && (
                        <>
                          {sttTest?.id === p.id && (
                            <span
                              title={sttTest.msg}
                              className={[
                                "text-[9px] max-w-[160px] truncate",
                                sttTest.status === "ok" ? "text-[rgba(134,239,172,0.7)]"
                                  : sttTest.status === "err" ? "text-[rgba(239,68,68,0.7)]"
                                  : "text-[rgba(255,255,255,0.3)]",
                              ].join(" ")}
                            >
                              {sttTest.status === "testing" ? t("mprof.testing") : sttTest.status === "ok" ? t("mprof.ok") : t("mprof.failed")}
                            </span>
                          )}
                          <button
                            onClick={() => handleTestStt(p.id)}
                            disabled={sttTest?.id === p.id && sttTest.status === "testing"}
                            className="text-[rgba(251,191,36,0.5)] hover:text-[#fbbf24] text-[11px] px-1.5 transition-colors disabled:opacity-50"
                          >
                            {t("mprof.test")}
                          </button>
                        </>
                      )}
                      <button
                        onClick={() => startEditModel(p)}
                        className="text-[rgba(255,255,255,0.25)] hover:text-[rgba(255,255,255,0.5)] text-[11px] px-1.5 transition-colors"
                      >
                        {t("common.edit")}
                      </button>
                      <button
                        onClick={() => handleDeleteProfile(p.id)}
                        className="text-[rgba(255,255,255,0.15)] hover:text-red-400 text-[11px] px-1 transition-colors"
                      >
                        {"✕"}
                      </button>
                    </div>
                  </div>
                );
              })
            )}

            {/* Add button */}
            <button
              onClick={startAddModel}
              className="w-full mt-1 py-2 rounded-lg bg-transparent border border-dashed border-[rgba(245,158,11,0.12)] text-[rgba(251,191,36,0.6)] text-[12px] hover:border-[rgba(245,158,11,0.25)] transition-colors"
            >
              {t("mprof.add-model")}
            </button>
          </div>
        )}
      </div>

      {saving && (
        <div className="text-center text-[rgba(255,255,255,0.25)] text-[10px] mt-1">
          {t("settings.saving")}
        </div>
      )}
    </>
  );
}
