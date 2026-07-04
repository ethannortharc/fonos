// First-run onboarding wizard — full-screen takeover shown until the user
// completes (or skips) setup. Walks a brand-new user from install to working
// dictation without docs:
//   0. Grant Microphone + Accessibility permissions + pick a mic
//   1. Set up a speech engine (reuses the Settings model-profile editor)
//   2. Set/confirm the primary dictation hotkey
//   3. Live "try it now" test dictation
//
// The backend + mic steps REUSE the same components as Settings (ModelProfileEditor,
// MicrophonePicker) so the model-profile shape and mic logic can't drift.

import { useState, useEffect, useRef, useCallback } from "react";
import {
  getConfig,
  saveConfig,
  hasMicrophone,
  checkAccessibility,
  openSettingsPane,
  startRecording,
  stopRecording,
} from "../lib/api";
import type { AppConfig, ModelProfile, SttResult } from "../types";
import { t, useT } from "../lib/i18n";
import { HotkeyInput } from "./settings/HotkeysTab";
import ModelProfileEditor from "./settings/ModelProfileEditor";
import MicrophonePicker from "./settings/MicrophonePicker";

const errStr = (e: unknown) => (e instanceof Error ? e.message : String(e));

const isMac =
  typeof navigator !== "undefined" &&
  /mac/i.test(navigator.platform || navigator.userAgent || "");

/** Whether the app already has a usable STT configuration — a set default STT
 *  profile, or any model profile advertising the "stt" capability. Used by the
 *  first-run gate so existing installs (models configured via Settings) never
 *  see the wizard even with has_completed_onboarding unset. */
export function isSttConfigured(cfg: AppConfig): boolean {
  const hasDefault = (cfg.stt_profile ?? "") !== "";
  const hasSttCapable = (cfg.model_profiles ?? []).some(
    (p) => Array.isArray(p.capabilities) && p.capabilities.includes("stt")
  );
  return hasDefault || hasSttCapable;
}

type Step = 0 | 1 | 2 | 3;

const STEP_LABELS = [
  "onboard.step.perms",
  "onboard.step.backend",
  "onboard.step.hotkey",
  "onboard.step.try",
] as const;

// ─── Small building blocks ───────────────────────────────────────────────────

function StatusBadge({ granted }: { granted: boolean | null }) {
  if (granted === true) {
    return (
      <span className="inline-flex items-center gap-1 text-[11px] font-medium text-[rgba(134,239,172,0.9)]">
        <span className="text-[12px]">{"✓"}</span> {t("onboard.granted")}
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 text-[11px] font-medium text-[#fbbf24]">
      <span className="text-[12px]">{"○"}</span>{" "}
      {granted === null ? t("onboard.checking") : t("onboard.pending")}
    </span>
  );
}

function ErrorLine({ msg }: { msg: string }) {
  if (!msg) return null;
  return <div className="text-[11px] text-[rgba(239,68,68,0.8)] mt-1">{msg}</div>;
}

// ─── Onboarding ──────────────────────────────────────────────────────────────

export default function Onboarding({ onDone }: { onDone: () => void }) {
  useT();
  const [step, setStep] = useState<Step>(0);

  // Full config snapshot so the shared editor sees + merges model_profiles the
  // same way Settings does. Loaded once; updated through handleSave.
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [hotkey, setHotkey] = useState("cmd+shift+space");
  const [hotkeyErr, setHotkeyErr] = useState("");

  useEffect(() => {
    getConfig()
      .then((cfg) => {
        setConfig(cfg);
        if (cfg.hotkey_dictation) setHotkey(cfg.hotkey_dictation);
      })
      .catch(() => {});
  }, []);

  // Persist a config delta and merge it into local state — mirrors Settings'
  // handleSave. Functional setState avoids stale closures across rapid saves
  // (e.g. model_profiles then stt_profile).
  const handleSave = useCallback(async (updates: Partial<AppConfig>) => {
    try {
      await saveConfig(JSON.stringify(updates));
      setConfig((prev) => (prev ? { ...prev, ...updates } : prev));
    } catch (e) {
      setBackendErr(errStr(e));
    }
  }, []);

  // ── Step 0: Permissions ──────────────────────────────────────────────────
  const [micGranted, setMicGranted] = useState<boolean | null>(null);
  const [axGranted, setAxGranted] = useState<boolean | null>(null);
  const [permErr, setPermErr] = useState("");

  const recheckPerms = useCallback(async () => {
    try {
      setMicGranted(await hasMicrophone());
    } catch (e) {
      setPermErr(errStr(e));
    }
    try {
      setAxGranted(await checkAccessibility());
    } catch (e) {
      setPermErr(errStr(e));
    }
  }, []);

  // Re-check every 2s while the permissions step is visible.
  useEffect(() => {
    if (step !== 0) return;
    recheckPerms();
    const t = setInterval(recheckPerms, 2000);
    return () => clearInterval(t);
  }, [step, recheckPerms]);

  const openPane = async (pane: string) => {
    setPermErr("");
    try {
      await openSettingsPane(pane);
    } catch (e) {
      setPermErr(errStr(e));
    }
  };

  // ── Step 1: Backend (shared model-profile editor) ────────────────────────
  const [backendErr, setBackendErr] = useState("");

  // When a profile is saved from the wizard, also assign it as the default STT
  // (and LLM) service — the same mechanism ServiceCardDropdown uses in ModelsTab.
  const applySttDefaults = useCallback(
    (added: ModelProfile[]) => {
      const stt = added.find((p) => p.capabilities?.includes("stt"));
      if (!stt) return;
      const updates: Partial<AppConfig> = { stt_profile: stt.id };
      if (stt.capabilities?.includes("llm")) updates.llm_profile = stt.id;
      handleSave(updates);
    },
    [handleSave]
  );

  // Apple on-device: Settings has no way to create a provider="apple" model
  // profile (PROVIDERS has no "apple", and the ModesTab "apple-speech" option is
  // a per-mode stt_model sentinel, not a model_profile). So the wizard offers a
  // quick button that builds the profile through the SAME save path as the
  // editor — it lands in model_profiles identically and routes to Apple STT
  // because the Rust STT resolver keys on profile.provider == "apple".
  const addAppleProfile = useCallback(() => {
    // Idempotent: a second click must not create a duplicate apple profile.
    if ((config?.model_profiles ?? []).some((p) => p.provider === "apple")) return;
    const appleProfile: ModelProfile = {
      id: `apple-${Date.now()}`,
      name: "Apple on-device Speech",
      provider: "apple",
      model: "apple-speech",
      capabilities: ["stt"],
    };
    const next = [...(config?.model_profiles ?? []), appleProfile];
    handleSave({ model_profiles: next });
    applySttDefaults([appleProfile]);
  }, [config, handleSave, applySttDefaults]);

  const appleAlreadyAdded = (config?.model_profiles ?? []).some(
    (p) => p.provider === "apple"
  );

  // ── Step 2: Hotkey ───────────────────────────────────────────────────────
  const handleHotkeyChange = async (v: string) => {
    setHotkey(v);
    setHotkeyErr("");
    try {
      await saveConfig(JSON.stringify({ hotkey_dictation: v }));
      setConfig((prev) => (prev ? { ...prev, hotkey_dictation: v } : prev));
    } catch (e) {
      setHotkeyErr(errStr(e));
    }
  };

  // ── Step 3: Try it ───────────────────────────────────────────────────────
  const [recording, setRecording] = useState(false);
  const [tryResult, setTryResult] = useState<SttResult | null>(null);
  const [tryText, setTryText] = useState("");
  const [tryErr, setTryErr] = useState("");
  const taRef = useRef<HTMLTextAreaElement>(null);

  const toggleRecord = async () => {
    setTryErr("");
    if (recording) {
      setRecording(false);
      try {
        taRef.current?.focus();
        // "raw" mode ALSO injects the transcript at the OS cursor — which lands
        // in the focused textarea below, exercising the full capture→inject loop.
        const r = await stopRecording("raw");
        setTryResult(r);
        taRef.current?.focus();
      } catch (e) {
        setTryErr(errStr(e));
      }
    } else {
      setTryResult(null);
      try {
        const mic = await hasMicrophone();
        if (!mic) {
          setTryErr(t("onboard.no-mic"));
          return;
        }
        await startRecording();
        setRecording(true);
        taRef.current?.focus();
      } catch (e) {
        setTryErr(errStr(e));
      }
    }
  };

  const handleFinish = async () => {
    try {
      await saveConfig(JSON.stringify({ has_completed_onboarding: true }));
    } catch {
      // Finish regardless — the wizard shouldn't trap the user on a save error.
    }
    onDone();
  };

  // Skip is friction-free: one click marks onboarding done and dismisses the
  // wizard. Everything remains configurable later in Settings.
  const handleSkip = async () => {
    try {
      await saveConfig(JSON.stringify({ has_completed_onboarding: true }));
    } catch {
      // ignore
    }
    onDone();
  };

  // ── Render ───────────────────────────────────────────────────────────────
  return (
    <div className="fixed inset-0 z-50 bg-[#1a1917] flex flex-col select-none">
      {/* Header — drag region, step dots, skip */}
      <div
        className="relative flex items-center justify-center h-[48px] flex-shrink-0 border-b border-[rgba(255,255,255,0.05)]"
        data-tauri-drag-region=""
      >
        <div className="flex items-center gap-2">
          {STEP_LABELS.map((label, i) => (
            <div key={label} className="flex items-center gap-1.5">
              <div
                className={[
                  "w-1.5 h-1.5 rounded-full transition-colors",
                  i === step
                    ? "bg-[#fbbf24]"
                    : i < step
                      ? "bg-[rgba(245,158,11,0.4)]"
                      : "bg-[rgba(255,255,255,0.15)]",
                ].join(" ")}
              />
              <span
                className={[
                  "text-[10px] transition-colors",
                  i === step ? "text-[#fafaf9]" : "text-[rgba(255,255,255,0.25)]",
                ].join(" ")}
              >
                {t(label)}
              </span>
              {i < STEP_LABELS.length - 1 && (
                <span className="text-[rgba(255,255,255,0.1)] text-[10px] mx-0.5">
                  {"→"}
                </span>
              )}
            </div>
          ))}
        </div>
        <div className="absolute right-4 flex items-center gap-2.5">
          <span className="hidden md:inline text-[10px] text-[rgba(255,255,255,0.22)] whitespace-nowrap">
            {t("onboard.later")}
          </span>
          <button
            onClick={handleSkip}
            className="text-[11px] text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)] transition-colors"
          >
            {t("onboard.skip")}
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-[520px] mx-auto px-8 py-10 flex flex-col gap-6">
          {/* ── Step 0: Permissions ── */}
          {step === 0 && (
            <>
              <div className="flex flex-col gap-1.5">
                <div className="w-11 h-11 rounded-[13px] bg-gradient-to-br from-[#f59e0b] to-[#d97706] flex items-center justify-center shadow-[0_2px_12px_rgba(245,158,11,0.3)] mb-1">
                  <span className="text-[#1a1917] text-lg font-bold">f</span>
                </div>
                <h1 className="text-[20px] font-semibold text-[#fafaf9]">
                  {t("onboard.welcome")}
                </h1>
                <p className="text-[13px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                  {t("onboard.welcome-desc")}
                </p>
              </div>

              {/* Microphone row */}
              <div className="rounded-xl bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-2">
                <div className="flex items-center justify-between">
                  <span className="text-[13px] font-medium text-[#fafaf9]">
                    {t("onboard.mic")}
                  </span>
                  <StatusBadge granted={micGranted} />
                </div>
                <p className="text-[11px] text-[rgba(255,255,255,0.4)] leading-relaxed">
                  {t("onboard.mic-desc")}
                </p>
                <button
                  onClick={() => openPane("microphone")}
                  className="self-start mt-1 px-3 py-1.5 rounded-lg bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[11px] text-[rgba(255,255,255,0.6)] transition-colors"
                >
                  {t("onboard.open-settings")}
                </button>
              </div>

              {/* Accessibility row */}
              <div className="rounded-xl bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-2">
                <div className="flex items-center justify-between">
                  <span className="text-[13px] font-medium text-[#fafaf9]">
                    {t("onboard.ax")}
                  </span>
                  <StatusBadge granted={axGranted} />
                </div>
                <p className="text-[11px] text-[rgba(255,255,255,0.4)] leading-relaxed">
                  {t("onboard.ax-desc")}
                </p>
                <button
                  onClick={() => openPane("accessibility")}
                  className="self-start mt-1 px-3 py-1.5 rounded-lg bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[11px] text-[rgba(255,255,255,0.6)] transition-colors"
                >
                  {t("onboard.open-settings")}
                </button>
              </div>

              {(micGranted !== true || axGranted !== true) && (
                <p className="text-[11px] text-[rgba(251,191,36,0.7)] leading-relaxed">
                  {t("onboard.perm-note")}
                </p>
              )}
              <ErrorLine msg={permErr} />

              {/* Divider */}
              <div className="border-t border-[rgba(255,255,255,0.05)]" />

              {/* Microphone device selection (shared with Settings › General) */}
              <MicrophonePicker
                value={config?.audio_input_device ?? ""}
                onSelect={(name) => handleSave({ audio_input_device: name })}
              />
            </>
          )}

          {/* ── Step 1: Backend ── */}
          {step === 1 && (
            <>
              <div className="flex flex-col gap-1.5">
                <h1 className="text-[20px] font-semibold text-[#fafaf9]">
                  {t("onboard.engine-title")}
                </h1>
                <p className="text-[13px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                  {t("onboard.engine-desc")}
                </p>
              </div>

              {config ? (
                <>
                  {/* Apple on-device quick button (macOS only) */}
                  {isMac && (
                    <button
                      onClick={addAppleProfile}
                      className={[
                        "rounded-xl border p-4 text-left transition-colors",
                        appleAlreadyAdded
                          ? "bg-[rgba(245,158,11,0.06)] border-[rgba(245,158,11,0.3)]"
                          : "bg-[rgba(255,255,255,0.02)] border-[rgba(255,255,255,0.06)] hover:border-[rgba(245,158,11,0.3)]",
                      ].join(" ")}
                    >
                      <div className="flex items-center gap-2">
                        <span className="text-[13px] font-medium text-[#fafaf9]">
                          {t("onboard.apple")}
                        </span>
                        <span className="px-1.5 py-0.5 rounded text-[9px] font-medium uppercase tracking-wide bg-[rgba(245,158,11,0.12)] text-[#fbbf24]">
                          {t("onboard.apple-badge")}
                        </span>
                        {appleAlreadyAdded && (
                          <span className="text-[11px] text-[rgba(134,239,172,0.9)] ml-auto">
                            {"✓"} {t("onboard.added")}
                          </span>
                        )}
                      </div>
                      <p className="text-[11px] text-[rgba(255,255,255,0.4)] leading-relaxed mt-1.5">
                        {t("onboard.apple-desc")}
                      </p>
                    </button>
                  )}

                  {/* Or add / manage models via the same editor Settings uses */}
                  <div className="flex items-center gap-2">
                    <div className="flex-1 border-t border-[rgba(255,255,255,0.05)]" />
                    <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.25)]">
                      {isMac ? t("onboard.or-add-model") : t("onboard.add-model")}
                    </span>
                    <div className="flex-1 border-t border-[rgba(255,255,255,0.05)]" />
                  </div>

                  <ModelProfileEditor
                    config={config}
                    onSave={handleSave}
                    setError={setBackendErr}
                    onProfilesAdded={applySttDefaults}
                    startInAddMode={!isMac}
                  />
                </>
              ) : (
                <div className="py-6 text-center text-[rgba(255,255,255,0.25)] text-[12px]">
                  {t("onboard.loading")}
                </div>
              )}
              <ErrorLine msg={backendErr} />
            </>
          )}

          {/* ── Step 2: Hotkey ── */}
          {step === 2 && (
            <>
              <div className="flex flex-col gap-1.5">
                <h1 className="text-[20px] font-semibold text-[#fafaf9]">
                  {t("onboard.hotkey-title")}
                </h1>
                <p className="text-[13px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                  {t("onboard.hotkey-desc")}
                </p>
              </div>

              <div className="rounded-xl bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex items-center justify-between">
                <div className="flex flex-col gap-0.5">
                  <span className="text-[13px] font-medium text-[#fafaf9]">
                    {t("onboard.dictation")}
                  </span>
                  <span className="text-[11px] text-[rgba(255,255,255,0.35)]">
                    {t("onboard.hold-to-talk")}
                  </span>
                </div>
                <HotkeyInput value={hotkey} onChange={handleHotkeyChange} />
              </div>
              <p className="text-[11px] text-[rgba(255,255,255,0.3)]">
                {t("onboard.current")} <span className="font-mono text-[rgba(255,255,255,0.55)]">{hotkey}</span>. {t("onboard.hotkey-effect")}
              </p>
              <ErrorLine msg={hotkeyErr} />
            </>
          )}

          {/* ── Step 3: Try it ── */}
          {step === 3 && (
            <>
              <div className="flex flex-col gap-1.5">
                <h1 className="text-[20px] font-semibold text-[#fafaf9]">
                  {t("onboard.try-title")}
                </h1>
                <p className="text-[13px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                  {t("onboard.try-desc")}
                </p>
              </div>

              <div className="flex flex-col items-center gap-4">
                <button
                  onClick={toggleRecord}
                  className={[
                    "w-16 h-16 rounded-full flex items-center justify-center transition-all duration-300",
                    recording
                      ? "bg-[rgba(239,68,68,0.15)] border border-[rgba(239,68,68,0.4)]"
                      : "bg-gradient-to-br from-[#f59e0b] to-[#d97706] shadow-[0_2px_14px_rgba(245,158,11,0.35)] hover:opacity-90",
                  ].join(" ")}
                >
                  {recording ? (
                    <span className="w-4 h-4 rounded-[3px] bg-[rgba(239,68,68,0.9)]" />
                  ) : (
                    <svg
                      width={24}
                      height={24}
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="#1a1917"
                      strokeWidth={1.8}
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    >
                      <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
                      <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
                      <line x1="12" y1="19" x2="12" y2="23" />
                    </svg>
                  )}
                </button>
                <span className="text-[11px] text-[rgba(255,255,255,0.4)]">
                  {recording ? t("onboard.recording") : t("onboard.click-record")}
                </span>
              </div>

              <textarea
                ref={taRef}
                value={tryText}
                onChange={(e) => setTryText(e.target.value)}
                placeholder={t("onboard.try-ph")}
                rows={4}
                autoFocus
                className="w-full bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.08)] rounded-xl px-4 py-3 text-[13px] text-[#fafaf9] leading-relaxed focus:outline-none focus:border-[rgba(245,158,11,0.3)] resize-none"
              />

              {tryResult && (
                <div className="flex flex-col gap-1.5">
                  <div className="flex items-center gap-3 text-[10px] text-[rgba(255,255,255,0.35)]">
                    <span>{t("onboard.latency")} {tryResult.latency_ms} ms</span>
                    {tryResult.stt_engine && (
                      <span>{t("onboard.engine")} {tryResult.stt_engine}</span>
                    )}
                  </div>
                  {tryResult.text ? (
                    <div className="text-[11px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                      <span className="text-[rgba(255,255,255,0.3)]">
                        {t("onboard.transcript")}{" "}
                      </span>
                      {tryResult.text}
                      <div className="text-[10px] text-[rgba(251,191,36,0.6)] mt-1">
                        {t("onboard.inject-hint")}
                      </div>
                    </div>
                  ) : (
                    <div className="text-[11px] text-[rgba(255,255,255,0.4)]">
                      {t("onboard.no-speech")}
                    </div>
                  )}
                </div>
              )}
              <ErrorLine msg={tryErr} />
            </>
          )}
        </div>
      </div>

      {/* Footer — Back / Continue */}
      <div className="flex items-center justify-between h-[60px] flex-shrink-0 px-8 border-t border-[rgba(255,255,255,0.05)]">
        <button
          onClick={() => setStep((s) => Math.max(0, s - 1) as Step)}
          disabled={step === 0}
          className="px-4 py-2 rounded-lg text-[12px] text-[rgba(255,255,255,0.4)] hover:text-[rgba(255,255,255,0.7)] transition-colors disabled:opacity-0"
        >
          {t("onboard.back")}
        </button>

        {step < 3 ? (
          <button
            onClick={() => setStep((s) => (s + 1) as Step)}
            className="px-6 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-white text-[12px] font-medium hover:opacity-90 transition-opacity"
          >
            {t("onboard.continue")}
          </button>
        ) : (
          <button
            onClick={handleFinish}
            className="px-6 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-white text-[12px] font-medium hover:opacity-90 transition-opacity"
          >
            {t("onboard.finish")}
          </button>
        )}
      </div>
    </div>
  );
}
