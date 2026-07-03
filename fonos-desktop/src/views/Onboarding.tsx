// First-run onboarding wizard — full-screen takeover shown until the user
// completes (or skips) setup. Walks a brand-new user from install to working
// dictation without docs:
//   0. Grant Microphone + Accessibility permissions (deep links to OS panes)
//   1. Pick an STT/LLM backend (Apple on-device / cloud / local endpoint)
//   2. Set/confirm the primary dictation hotkey
//   3. Live "try it now" test dictation

import { useState, useEffect, useRef, useCallback } from "react";
import {
  getConfig,
  saveConfig,
  hasMicrophone,
  checkAccessibility,
  openSettingsPane,
  startRecording,
  stopRecording,
  testStt,
} from "../lib/api";
import type { ModelProfile, SttResult } from "../types";
import { PROVIDERS } from "./settings/constants";
import { HotkeyInput } from "./settings/HotkeysTab";

const errStr = (e: unknown) => (e instanceof Error ? e.message : String(e));

const isMac =
  typeof navigator !== "undefined" &&
  /mac/i.test(navigator.platform || navigator.userAgent || "");

type Step = 0 | 1 | 2 | 3;
type Backend = "apple" | "cloud" | "local";
type TestState = { status: "testing" | "ok" | "err"; msg: string } | null;

const STEP_LABELS = ["Permissions", "Backend", "Hotkey", "Try it"];

// ─── Small building blocks ───────────────────────────────────────────────────

function StatusBadge({ granted }: { granted: boolean | null }) {
  if (granted === true) {
    return (
      <span className="inline-flex items-center gap-1 text-[11px] font-medium text-[rgba(134,239,172,0.9)]">
        <span className="text-[12px]">{"✓"}</span> Granted
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 text-[11px] font-medium text-[#fbbf24]">
      <span className="text-[12px]">{"○"}</span>{" "}
      {granted === null ? "Checking…" : "Pending"}
    </span>
  );
}

function ErrorLine({ msg }: { msg: string }) {
  if (!msg) return null;
  return <div className="text-[11px] text-[rgba(239,68,68,0.8)] mt-1">{msg}</div>;
}

// ─── Onboarding ──────────────────────────────────────────────────────────────

export default function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState<Step>(0);

  // Existing config snapshot (so we merge — never clobber — model profiles).
  const [profiles, setProfiles] = useState<ModelProfile[]>([]);
  const [hotkey, setHotkey] = useState("cmd+shift+space");
  const [hotkeyErr, setHotkeyErr] = useState("");

  useEffect(() => {
    getConfig()
      .then((cfg) => {
        setProfiles(cfg.model_profiles ?? []);
        if (cfg.hotkey_dictation) setHotkey(cfg.hotkey_dictation);
      })
      .catch(() => {});
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

  // ── Step 1: Backend ──────────────────────────────────────────────────────
  const [backend, setBackend] = useState<Backend | null>(isMac ? "apple" : null);
  const [cloudProvider, setCloudProvider] = useState("openai");
  const [cloudKey, setCloudKey] = useState("");
  const [cloudModel, setCloudModel] = useState("");
  const [localUrl, setLocalUrl] = useState("http://localhost:8000");
  const [localModel, setLocalModel] = useState("");
  const [savedId, setSavedId] = useState<string | null>(null);
  const [backendTest, setBackendTest] = useState<TestState>(null);
  const [backendErr, setBackendErr] = useState("");

  const selectBackend = (b: Backend) => {
    setBackend(b);
    setSavedId(null);
    setBackendTest(null);
    setBackendErr("");
  };

  const idPrefix = () =>
    backend === "apple" ? "apple" : backend === "cloud" ? cloudProvider : "local";

  // Build a model profile matching ModelsTab's shape for the chosen backend.
  const buildProfile = (id: string): ModelProfile | null => {
    if (backend === "apple") {
      return {
        id,
        name: "Apple on-device Speech",
        provider: "apple",
        model: "apple-speech",
        capabilities: ["stt"],
      };
    }
    if (backend === "cloud") {
      const prov = PROVIDERS.find((p) => p.id === cloudProvider);
      return {
        id,
        name: `${prov?.label ?? cloudProvider} Speech`,
        provider: cloudProvider,
        model: cloudModel.trim(),
        api_key: cloudKey.trim() || undefined,
        base_url: prov?.url || undefined,
        capabilities: ["stt", "llm"],
        stt_api: cloudProvider === "openrouter" ? "chat" : undefined,
      };
    }
    if (backend === "local") {
      return {
        id,
        name: "Local endpoint",
        provider: "custom",
        model: localModel.trim(),
        base_url: localUrl.trim() || undefined,
        capabilities: ["stt", "llm"],
      };
    }
    return null;
  };

  // Persist the chosen backend: append/replace the profile, set it as the
  // default STT (and LLM if the profile is llm-capable). Returns the profile id.
  const saveBackend = async (): Promise<string | null> => {
    if (!backend) return null;
    const id = savedId ?? `${idPrefix()}-${Date.now()}`;
    const profile = buildProfile(id);
    if (!profile) return null;
    const others = profiles.filter((p) => p.id !== id);
    const next = [...others, profile];
    const isLlm = profile.capabilities?.includes("llm") ?? false;
    const updates: Record<string, unknown> = {
      model_profiles: next,
      stt_profile: id,
    };
    if (isLlm) updates.llm_profile = id;
    await saveConfig(JSON.stringify(updates));
    setProfiles(next);
    setSavedId(id);
    return id;
  };

  const handleTest = async () => {
    setBackendErr("");
    setBackendTest({ status: "testing", msg: "" });
    try {
      const id = await saveBackend();
      if (!id) {
        setBackendTest(null);
        return;
      }
      const msg = await testStt(id);
      setBackendTest({ status: "ok", msg });
    } catch (e) {
      setBackendTest({ status: "err", msg: errStr(e) });
    }
  };

  const handleBackendContinue = async () => {
    setBackendErr("");
    if (backend) {
      try {
        await saveBackend();
      } catch (e) {
        setBackendErr(errStr(e));
        return;
      }
    }
    setStep(2);
  };

  // ── Step 2: Hotkey ───────────────────────────────────────────────────────
  const handleHotkeyChange = async (v: string) => {
    setHotkey(v);
    setHotkeyErr("");
    try {
      await saveConfig(JSON.stringify({ hotkey_dictation: v }));
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
          setTryErr("No microphone detected — grant Microphone access in step 1.");
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

  const handleSkip = async () => {
    if (
      !window.confirm(
        "Skip setup? You can configure permissions, models and hotkeys later in Settings."
      )
    )
      return;
    try {
      await saveConfig(JSON.stringify({ has_completed_onboarding: true }));
    } catch {
      // ignore
    }
    onDone();
  };

  // ── Shared field styles ──────────────────────────────────────────────────
  const inputCls =
    "bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]";

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
                {label}
              </span>
              {i < STEP_LABELS.length - 1 && (
                <span className="text-[rgba(255,255,255,0.1)] text-[10px] mx-0.5">
                  {"→"}
                </span>
              )}
            </div>
          ))}
        </div>
        <button
          onClick={handleSkip}
          className="absolute right-4 text-[11px] text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)] transition-colors"
        >
          Skip setup
        </button>
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
                  Welcome to Fonos
                </h1>
                <p className="text-[13px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                  Fonos turns your voice into text anywhere on your Mac. First,
                  grant two permissions so it can hear you and type for you.
                </p>
              </div>

              {/* Microphone row */}
              <div className="rounded-xl bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-2">
                <div className="flex items-center justify-between">
                  <span className="text-[13px] font-medium text-[#fafaf9]">
                    Microphone
                  </span>
                  <StatusBadge granted={micGranted} />
                </div>
                <p className="text-[11px] text-[rgba(255,255,255,0.4)] leading-relaxed">
                  Lets Fonos capture audio for dictation. macOS may only prompt
                  the first time you record.
                </p>
                <button
                  onClick={() => openPane("microphone")}
                  className="self-start mt-1 px-3 py-1.5 rounded-lg bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[11px] text-[rgba(255,255,255,0.6)] transition-colors"
                >
                  Open System Settings
                </button>
              </div>

              {/* Accessibility row */}
              <div className="rounded-xl bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-2">
                <div className="flex items-center justify-between">
                  <span className="text-[13px] font-medium text-[#fafaf9]">
                    Accessibility
                  </span>
                  <StatusBadge granted={axGranted} />
                </div>
                <p className="text-[11px] text-[rgba(255,255,255,0.4)] leading-relaxed">
                  Lets Fonos register your global hotkey and paste transcribed
                  text into other apps.
                </p>
                <button
                  onClick={() => openPane("accessibility")}
                  className="self-start mt-1 px-3 py-1.5 rounded-lg bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[11px] text-[rgba(255,255,255,0.6)] transition-colors"
                >
                  Open System Settings
                </button>
              </div>

              {(micGranted !== true || axGranted !== true) && (
                <p className="text-[11px] text-[rgba(251,191,36,0.7)] leading-relaxed">
                  You can continue now — macOS often prompts for the microphone
                  on your first recording. Accessibility is required for hotkeys
                  and pasting.
                </p>
              )}
              <ErrorLine msg={permErr} />
            </>
          )}

          {/* ── Step 1: Backend ── */}
          {step === 1 && (
            <>
              <div className="flex flex-col gap-1.5">
                <h1 className="text-[20px] font-semibold text-[#fafaf9]">
                  Choose a speech engine
                </h1>
                <p className="text-[13px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                  This is what turns your voice into text. You can add more or
                  change this later in Settings.
                </p>
              </div>

              <div className="flex flex-col gap-2.5">
                {/* Apple on-device */}
                {isMac && (
                  <BackendCard
                    active={backend === "apple"}
                    onClick={() => selectBackend("apple")}
                    title="Apple on-device"
                    badge="Recommended"
                    desc="Zero setup, fully private, works offline. Uses macOS built-in speech recognition."
                  />
                )}

                {/* Cloud provider */}
                <BackendCard
                  active={backend === "cloud"}
                  onClick={() => selectBackend("cloud")}
                  title="Cloud provider"
                  desc="Paste an API key from OpenAI, OpenRouter, Google, and more."
                >
                  {backend === "cloud" && (
                    <div className="flex flex-col gap-2 mt-3">
                      <div className="flex flex-col gap-1">
                        <label className="text-[10px] text-[rgba(255,255,255,0.4)]">
                          Provider
                        </label>
                        <select
                          value={cloudProvider}
                          onChange={(e) => {
                            setCloudProvider(e.target.value);
                            setSavedId(null);
                            setBackendTest(null);
                          }}
                          className={`${inputCls} cursor-pointer appearance-none`}
                        >
                          {PROVIDERS.filter((p) => p.id !== "custom").map((p) => (
                            <option key={p.id} value={p.id}>
                              {p.label}
                            </option>
                          ))}
                        </select>
                      </div>
                      <div className="flex flex-col gap-1">
                        <label className="text-[10px] text-[rgba(255,255,255,0.4)]">
                          API Key
                        </label>
                        <input
                          type="password"
                          value={cloudKey}
                          onChange={(e) => {
                            setCloudKey(e.target.value);
                            setSavedId(null);
                          }}
                          placeholder="sk-..."
                          className={`${inputCls} font-mono`}
                        />
                      </div>
                      <div className="flex flex-col gap-1">
                        <label className="text-[10px] text-[rgba(255,255,255,0.4)]">
                          Model{" "}
                          <span className="text-[rgba(255,255,255,0.2)]">
                            (optional)
                          </span>
                        </label>
                        <input
                          type="text"
                          value={cloudModel}
                          onChange={(e) => {
                            setCloudModel(e.target.value);
                            setSavedId(null);
                          }}
                          placeholder="e.g. gpt-4o-transcribe"
                          className={`${inputCls} font-mono`}
                        />
                      </div>
                    </div>
                  )}
                </BackendCard>

                {/* Local endpoint */}
                <BackendCard
                  active={backend === "local"}
                  onClick={() => selectBackend("local")}
                  title="Local endpoint"
                  desc="Point Fonos at a self-hosted OpenAI-compatible server."
                >
                  {backend === "local" && (
                    <div className="flex flex-col gap-2 mt-3">
                      <div className="flex flex-col gap-1">
                        <label className="text-[10px] text-[rgba(255,255,255,0.4)]">
                          Base URL
                        </label>
                        <input
                          type="text"
                          value={localUrl}
                          onChange={(e) => {
                            setLocalUrl(e.target.value);
                            setSavedId(null);
                          }}
                          placeholder="http://localhost:8000"
                          className={`${inputCls} font-mono`}
                        />
                      </div>
                      <div className="flex flex-col gap-1">
                        <label className="text-[10px] text-[rgba(255,255,255,0.4)]">
                          Model{" "}
                          <span className="text-[rgba(255,255,255,0.2)]">
                            (optional)
                          </span>
                        </label>
                        <input
                          type="text"
                          value={localModel}
                          onChange={(e) => {
                            setLocalModel(e.target.value);
                            setSavedId(null);
                          }}
                          placeholder="e.g. whisper-1"
                          className={`${inputCls} font-mono`}
                        />
                      </div>
                    </div>
                  )}
                </BackendCard>
              </div>

              {/* Test + status */}
              {backend && (
                <div className="flex items-center gap-3">
                  <button
                    onClick={handleTest}
                    disabled={backendTest?.status === "testing"}
                    className="px-4 py-2 rounded-lg bg-[rgba(255,255,255,0.04)] hover:bg-[rgba(255,255,255,0.08)] text-[11px] text-[rgba(255,255,255,0.6)] transition-colors disabled:opacity-40"
                  >
                    {backendTest?.status === "testing" ? "Testing…" : "Test"}
                  </button>
                  {backendTest?.status === "ok" && (
                    <span
                      title={backendTest.msg}
                      className="text-[11px] text-[rgba(134,239,172,0.9)] truncate max-w-[320px]"
                    >
                      {"✓"} {backendTest.msg}
                    </span>
                  )}
                  {backendTest?.status === "err" && (
                    <span
                      title={backendTest.msg}
                      className="text-[11px] text-[rgba(239,68,68,0.85)] truncate max-w-[320px]"
                    >
                      {"✗"} {backendTest.msg}
                    </span>
                  )}
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
                  Set your dictation hotkey
                </h1>
                <p className="text-[13px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                  Hold this shortcut anywhere to talk, release to insert the
                  transcript at your cursor.
                </p>
              </div>

              <div className="rounded-xl bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex items-center justify-between">
                <div className="flex flex-col gap-0.5">
                  <span className="text-[13px] font-medium text-[#fafaf9]">
                    Dictation
                  </span>
                  <span className="text-[11px] text-[rgba(255,255,255,0.35)]">
                    Hold to talk
                  </span>
                </div>
                <HotkeyInput value={hotkey} onChange={handleHotkeyChange} />
              </div>
              <p className="text-[11px] text-[rgba(255,255,255,0.3)]">
                Current: <span className="font-mono text-[rgba(255,255,255,0.55)]">{hotkey}</span>. Changes take effect immediately — no restart needed.
              </p>
              <ErrorLine msg={hotkeyErr} />
            </>
          )}

          {/* ── Step 3: Try it ── */}
          {step === 3 && (
            <>
              <div className="flex flex-col gap-1.5">
                <h1 className="text-[20px] font-semibold text-[#fafaf9]">
                  Try it now
                </h1>
                <p className="text-[13px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                  Click record, say a sentence, then click stop. Your words
                  should appear in the box below.
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
                  {recording ? "Recording… click to stop" : "Click to record"}
                </span>
              </div>

              <textarea
                ref={taRef}
                value={tryText}
                onChange={(e) => setTryText(e.target.value)}
                placeholder="Your dictated text will appear here…"
                rows={4}
                autoFocus
                className="w-full bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.08)] rounded-xl px-4 py-3 text-[13px] text-[#fafaf9] leading-relaxed focus:outline-none focus:border-[rgba(245,158,11,0.3)] resize-none"
              />

              {tryResult && (
                <div className="flex flex-col gap-1.5">
                  <div className="flex items-center gap-3 text-[10px] text-[rgba(255,255,255,0.35)]">
                    <span>Latency: {tryResult.latency_ms} ms</span>
                    {tryResult.stt_engine && (
                      <span>Engine: {tryResult.stt_engine}</span>
                    )}
                  </div>
                  {tryResult.text ? (
                    <div className="text-[11px] text-[rgba(255,255,255,0.5)] leading-relaxed">
                      <span className="text-[rgba(255,255,255,0.3)]">
                        Transcript:{" "}
                      </span>
                      {tryResult.text}
                      <div className="text-[10px] text-[rgba(251,191,36,0.6)] mt-1">
                        If nothing appeared in the box above, check the
                        Accessibility permission in step 1.
                      </div>
                    </div>
                  ) : (
                    <div className="text-[11px] text-[rgba(255,255,255,0.4)]">
                      No speech detected — try again and speak clearly.
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
          Back
        </button>

        {step < 3 ? (
          <button
            onClick={() =>
              step === 1 ? handleBackendContinue() : setStep((s) => (s + 1) as Step)
            }
            className="px-6 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-white text-[12px] font-medium hover:opacity-90 transition-opacity"
          >
            Continue
          </button>
        ) : (
          <button
            onClick={handleFinish}
            className="px-6 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-white text-[12px] font-medium hover:opacity-90 transition-opacity"
          >
            Finish
          </button>
        )}
      </div>
    </div>
  );
}

// ─── Backend choice card ─────────────────────────────────────────────────────

function BackendCard({
  active,
  onClick,
  title,
  badge,
  desc,
  children,
}: {
  active: boolean;
  onClick: () => void;
  title: string;
  badge?: string;
  desc: string;
  children?: React.ReactNode;
}) {
  return (
    <div
      onClick={onClick}
      className={[
        "rounded-xl border p-4 cursor-pointer transition-colors",
        active
          ? "bg-[rgba(245,158,11,0.06)] border-[rgba(245,158,11,0.3)]"
          : "bg-[rgba(255,255,255,0.02)] border-[rgba(255,255,255,0.06)] hover:border-[rgba(255,255,255,0.12)]",
      ].join(" ")}
    >
      <div className="flex items-center gap-2">
        <div
          className={[
            "w-3.5 h-3.5 rounded-full border flex items-center justify-center flex-shrink-0",
            active ? "border-[#fbbf24]" : "border-[rgba(255,255,255,0.2)]",
          ].join(" ")}
        >
          {active && <div className="w-1.5 h-1.5 rounded-full bg-[#fbbf24]" />}
        </div>
        <span className="text-[13px] font-medium text-[#fafaf9]">{title}</span>
        {badge && (
          <span className="px-1.5 py-0.5 rounded text-[9px] font-medium uppercase tracking-wide bg-[rgba(245,158,11,0.12)] text-[#fbbf24]">
            {badge}
          </span>
        )}
      </div>
      <p className="text-[11px] text-[rgba(255,255,255,0.4)] leading-relaxed mt-1.5 ml-[22px]">
        {desc}
      </p>
      {children && <div className="ml-[22px]">{children}</div>}
    </div>
  );
}
