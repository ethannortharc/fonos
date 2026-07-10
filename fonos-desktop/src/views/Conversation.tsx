// Conversation — the in-app real-time speech-to-speech surface (issue #24).
// Hold the talk button (or ⌥S anywhere) → STT → persona LLM → spoken reply.
// Live turn progress arrives as `sts:event` from the Rust bridge, so turns
// started from the global hotkey render here too.

import { useState, useEffect, useRef, useCallback } from "react";
import {
  getConfig,
  saveConfig,
  stsPageStart,
  stsPageStop,
  getStsHistory,
  resetStsSession,
  stopPlayback,
  callStart,
  callStop,
} from "../lib/api";
import type { AppConfig } from "../types";
import { t, useT, type TKey } from "../lib/i18n";
import { FonosMark } from "../components/Icons";

type TurnState = "idle" | "listening" | "thinking" | "speaking";

interface Message {
  role: "user" | "assistant" | "error";
  text: string;
  /** Assistant reply that the user barged in on — renders an "· interrupted" tag. */
  interrupted?: boolean;
}

const STATE_META: Record<TurnState, { label: TKey; color: string }> = {
  idle: { label: "conv.state.idle", color: "#78716c" },
  listening: { label: "conv.state.listening", color: "#f87171" },
  thinking: { label: "conv.state.thinking", color: "#fbbf24" },
  speaking: { label: "conv.state.speaking", color: "#4ade80" },
};

export default function Conversation() {
  useT();
  const [messages, setMessages] = useState<Message[]>([]);
  const [turnState, setTurnState] = useState<TurnState>("idle");
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [persona, setPersona] = useState("");
  const [personaOpen, setPersonaOpen] = useState(false);
  const [personaDirty, setPersonaDirty] = useState(false);
  const holdingRef = useRef(false);
  // Mic lifecycle phase, tracked in a ref so mouse handlers never race React
  // renders: idle → starting (start_recording in flight) → recording.
  const phaseRef = useRef<"idle" | "starting" | "recording">("idle");
  const scrollRef = useRef<HTMLDivElement>(null);

  // ── Hands-free call mode ──
  const [inCall, setInCall] = useState(false);
  const [callSecs, setCallSecs] = useState(0);
  // Which audio path the live call engaged, for the AEC truth chip. `null`
  // until the backend's `call_started` reports it (or once the call ends).
  const [audioPath, setAudioPath] = useState<"aec" | "ec" | "fallback" | null>(null);
  // Mirror of `inCall` for pointer handlers / timers (avoids stale closures).
  const inCallRef = useRef(false);
  // Press disambiguation: a short tap toggles the call; a longer hold is the
  // existing walkie-talkie press. `pressTimerRef` fires when the hold threshold
  // passes; `holdStartedRef` records that the walkie-talkie leg engaged.
  // 450ms: relaxed clicks commonly dwell 200-350ms — a 250ms threshold
  // misrouted them into a hold and produced spurious "No speech detected".
  const pressTimerRef = useRef<number | null>(null);
  const holdStartedRef = useRef(false);
  const CLICK_MS = 450;

  useEffect(() => {
    inCallRef.current = inCall;
  }, [inCall]);

  // Call-duration timer (mm:ss), reset whenever a call starts/ends.
  useEffect(() => {
    if (!inCall) {
      setCallSecs(0);
      return;
    }
    setCallSecs(0);
    const id = window.setInterval(() => setCallSecs((s) => s + 1), 1000);
    return () => window.clearInterval(id);
  }, [inCall]);

  // Initial load: config (persona default) + session history.
  useEffect(() => {
    getConfig().then((c) => {
      setConfig(c);
      setPersona(c.sts_persona ?? "");
    });
    getStsHistory()
      .then((pairs) =>
        setMessages(
          pairs.flatMap(([u, a]) => [
            { role: "user" as const, text: u },
            { role: "assistant" as const, text: a },
          ]),
        ),
      )
      .catch(() => {});
  }, []);

  // Live turn events from the Rust bridge.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const un = await listen<{ kind: string; text: string; audio?: string }>("sts:event", (e) => {
          const { kind, text, audio } = e.payload;
          if (kind === "transcript") {
            setMessages((m) => [...m, { role: "user", text }]);
            setTurnState("thinking");
          } else if (kind === "reply") {
            setMessages((m) => [...m, { role: "assistant", text }]);
          } else if (kind === "speaking_started") {
            setTurnState("speaking");
          } else if (kind === "barge") {
            // The user talked over the reply: jump straight back to listening
            // and tag the truncated assistant bubble as interrupted.
            setTurnState("listening");
            setMessages((m) => {
              const last = m.length - 1;
              for (let i = last; i >= 0; i--) {
                if (m[i].role === "assistant") {
                  if (m[i].interrupted) return m;
                  const next = [...m];
                  next[i] = { ...next[i], interrupted: true };
                  return next;
                }
              }
              return m;
            });
          } else if (kind === "speaking_done" || kind === "turn_done") {
            setTurnState("idle");
          } else if (kind === "call_started") {
            setInCall(true);
            if (audio === "aec" || audio === "ec" || audio === "fallback") {
              setAudioPath(audio);
            }
          } else if (kind === "call_listening") {
            setInCall(true);
            setTurnState("listening");
          } else if (kind === "call_ended") {
            setInCall(false);
            setTurnState("idle");
            setAudioPath(null);
            if (text === "timeout") {
              setMessages((m) => [...m, { role: "error", text: t("conv.call.timeout") }]);
            }
          } else if (kind === "error") {
            setMessages((m) => [...m, { role: "error", text }]);
            // In a call the loop keeps listening after a transient error; only
            // drop out of the transient thinking/speaking chip.
            setTurnState(inCallRef.current ? "listening" : "idle");
          }
        });
        // The effect may have been cleaned up while `listen` was resolving
        // (StrictMode double-mount, tab switches) — drop the subscription
        // immediately instead of leaking a duplicate listener.
        if (disposed) un();
        else unlisten = un;
      } catch {
        // demo mode: no event bridge
      }
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // Auto-scroll on new content.
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [messages, turnState]);

  // Backend failures surface via the sts:event bridge (error bubble + idle);
  // the promise rejection here only needs to unstick the local state.
  const finishTurn = useCallback(async () => {
    setTurnState("thinking");
    try {
      // Page persona (possibly edited) overrides config for this turn.
      await stsPageStop(persona.trim() ? persona : undefined);
    } catch {
      setTurnState((s) => (s === "thinking" ? "idle" : s));
    }
  }, [persona]);

  const holdStart = useCallback(async () => {
    if (holdingRef.current || phaseRef.current !== "idle") return;
    holdingRef.current = true;
    phaseRef.current = "starting";
    try {
      await stsPageStart();
    } catch (err) {
      holdingRef.current = false;
      phaseRef.current = "idle";
      setMessages((m) => [...m, { role: "error", text: String(err) }]);
      setTurnState("idle");
      return;
    }
    if (holdingRef.current) {
      phaseRef.current = "recording";
      setTurnState("listening");
    } else {
      // Released before the mic finished starting: the recording IS live now,
      // so finish the turn instead of leaving it running.
      phaseRef.current = "idle";
      finishTurn();
    }
  }, [finishTurn]);

  const holdStop = useCallback(() => {
    if (!holdingRef.current) return;
    holdingRef.current = false;
    if (phaseRef.current === "recording") {
      phaseRef.current = "idle";
      finishTurn();
    }
    // else phase "starting": holdStart's tail finishes the turn.
  }, [finishTurn]);

  // ── Call toggle ──
  const startCall = useCallback(async () => {
    // Never on top of an in-flight hold-to-talk turn.
    if (holdingRef.current || phaseRef.current !== "idle") return;
    setInCall(true); // optimistic; the backend confirms via call_started
    try {
      await callStart();
    } catch (err) {
      setInCall(false);
      setMessages((m) => [...m, { role: "error", text: String(err) }]);
    }
  }, []);

  const endCall = useCallback(async () => {
    setInCall(false); // optimistic; call_ended will also reset
    await callStop().catch(() => {});
  }, []);

  // Distinguish a click (call toggle) from a hold (walkie-talkie). The timer
  // arms only when NOT in a call; during a call the button is a pure hang-up.
  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (e.button !== 0) return; // primary button / touch only
      holdStartedRef.current = false;
      if (inCallRef.current) return; // in a call: release hangs up, no hold leg
      pressTimerRef.current = window.setTimeout(() => {
        pressTimerRef.current = null;
        holdStartedRef.current = true;
        holdStart(); // crossed the hold threshold → walkie-talkie press
      }, CLICK_MS);
    },
    [holdStart],
  );

  const onPointerUp = useCallback(() => {
    if (pressTimerRef.current !== null) {
      // Released before the hold threshold → a click.
      window.clearTimeout(pressTimerRef.current);
      pressTimerRef.current = null;
      if (inCallRef.current) endCall();
      else startCall();
      return;
    }
    if (holdStartedRef.current) {
      holdStartedRef.current = false;
      holdStop();
    } else if (inCallRef.current) {
      // Press began during a call (no hold timer armed) → hang up on release.
      endCall();
    }
  }, [holdStop, startCall, endCall]);

  const onPointerLeave = useCallback(() => {
    // Abort a pending click, or end an engaged walkie-talkie hold. Never
    // toggles the call — leaving the button is not an intentional tap.
    if (pressTimerRef.current !== null) {
      window.clearTimeout(pressTimerRef.current);
      pressTimerRef.current = null;
      return;
    }
    if (holdStartedRef.current) {
      holdStartedRef.current = false;
      holdStop();
    }
  }, [holdStop]);

  const handleReset = async () => {
    stopPlayback().catch(() => {});
    await resetStsSession().catch(() => {});
    setMessages([]);
    setTurnState("idle");
  };

  const savePersonaDefault = async () => {
    if (!config) return;
    const next = { ...config, sts_persona: persona };
    setConfig(next);
    setPersonaDirty(false);
    await saveConfig(JSON.stringify(next)).catch(() => {});
  };

  const meta = STATE_META[turnState];
  const listening = turnState === "listening";
  const red = inCall || listening;
  const mmss = `${String(Math.floor(callSecs / 60)).padStart(2, "0")}:${String(
    callSecs % 60,
  ).padStart(2, "0")}`;

  return (
    <div className="h-full flex flex-col bg-[var(--bg)]">
      {/* ── header ── */}
      <div className="flex items-center justify-between px-5 pt-4 pb-3 border-b border-[rgba(255,255,255,0.045)]">
        <div className="flex items-center gap-3">
          <h1 className="fonos-page-title">{t("conv.title")}</h1>
          <span
            className="flex items-center gap-1.5 text-[9.5px] font-medium px-2 py-0.5 rounded-full bg-[rgba(255,255,255,0.05)] border border-[rgba(255,255,255,0.055)]"
            style={{ color: meta.color }}
          >
            <span
              className={`w-1.5 h-1.5 rounded-full ${turnState !== "idle" ? "animate-pulse" : ""}`}
              style={{ background: meta.color }}
            />
            {t(meta.label)}
          </span>
          {inCall && audioPath && (
            <span
              title={t(audioPath === "fallback" ? "conv.call.noaec.title" : "conv.call.aec.title")}
              className="text-[9px] px-1.5 py-0.5 rounded-full"
              style={
                audioPath === "fallback"
                  ? { background: "rgba(251,191,36,0.12)", color: "#fbbf24" }
                  : { background: "rgba(74,222,128,0.12)", color: "#4ade80" }
              }
            >
              {t(audioPath === "fallback" ? "conv.call.noaec" : "conv.call.aec")}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setPersonaOpen((v) => !v)}
            className={`text-[10px] px-2.5 py-1 rounded-[8px] transition-colors ${
              personaOpen
                ? "bg-[rgba(242,184,75,0.12)] text-[var(--accent)]"
                : "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)]"
            }`}
          >
            {t("conv.persona")}
          </button>
          <button
            onClick={handleReset}
            className="text-[10px] px-2.5 py-1 rounded-[8px] bg-[rgba(255,255,255,0.04)] text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[rgba(255,255,255,0.065)] transition-colors"
          >
            {t("conv.newchat")}
          </button>
        </div>
      </div>

      {/* ── persona drawer ── */}
      {personaOpen && (
        <div className="mx-5 mb-3 p-3.5 rounded-[14px] fonos-surface">
          <div className="text-[11px] text-[var(--text-muted)] mb-2">
            {t("conv.persona.desc")}
          </div>
          <textarea
            value={persona}
            onChange={(e) => {
              setPersona(e.target.value);
              setPersonaDirty(true);
            }}
            rows={3}
            spellCheck={false}
            className="w-full bg-[rgba(255,255,255,0.035)] border border-[rgba(255,255,255,0.08)] rounded-[10px] px-3 py-2.5 text-[var(--text-primary)] text-[12px] leading-relaxed resize-y focus:outline-none focus:border-[rgba(240,173,50,0.35)]"
          />
          <div className="flex items-center justify-between mt-1.5">
            <span className="text-[10px] text-[var(--text-faint)]">
              {t("conv.persona.applies")}{personaDirty ? t("conv.persona.unsaved") : ""}
            </span>
            <button
              onClick={savePersonaDefault}
              disabled={!personaDirty}
              className="text-[10px] px-2.5 py-1 rounded-md bg-[rgba(240,173,50,0.11)] text-[var(--accent)] disabled:opacity-30 transition-opacity"
            >
              {t("conv.persona.savedefault")}
            </button>
          </div>
        </div>
      )}

      {/* ── transcript ── */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-5 py-4">
        <div className="w-full max-w-[680px] min-h-full mx-auto flex flex-col gap-3">
        {messages.length === 0 && turnState === "idle" && (
          <div className="flex-1 flex flex-col items-center justify-center gap-3 text-center pb-3">
            <div className="w-14 h-14 rounded-[17px] fonos-surface fonos-surface-glow text-[var(--accent)] flex items-center justify-center">
              <FonosMark size={25} />
            </div>
            <div className="text-[14px] font-semibold text-[var(--text-primary)]">{t("conv.empty.title")}</div>
            <div className="text-[11px] text-[var(--text-muted)] max-w-[360px] leading-relaxed">
              <span className="font-mono text-[var(--text-secondary)] bg-[rgba(255,255,255,0.045)] border border-[rgba(255,255,255,0.07)] rounded-md px-1.5 py-0.5 mr-1">⌥S</span>
              {t("conv.empty.hint")} {config?.sts_max_turns ?? 8} {t("conv.empty.turns")}
            </div>
          </div>
        )}

        {messages.map((m, i) => (
          <div key={i} className={`flex ${m.role === "user" ? "justify-end" : "justify-start"}`}>
            <div
              className={`max-w-[76%] px-3.5 py-2.5 rounded-[16px] text-[12.5px] leading-relaxed whitespace-pre-wrap border ${
                m.role === "user"
                  ? "bg-[rgba(240,173,50,0.11)] border-[rgba(240,173,50,0.11)] text-[#f7d993] rounded-br-[5px]"
                  : m.role === "assistant"
                    ? "bg-[rgba(255,255,255,0.045)] border-[rgba(255,255,255,0.055)] text-[#e7e5e4] rounded-bl-[5px]"
                    : "bg-[rgba(239,68,68,0.08)] border-[rgba(239,68,68,0.1)] text-[rgba(252,165,165,0.9)] rounded-bl-[5px]"
              }`}
            >
              {m.text}
              {m.role === "assistant" && m.interrupted && (
                <span className="text-[rgba(255,255,255,0.3)] italic">{t("conv.interrupted")}</span>
              )}
            </div>
          </div>
        ))}

        {turnState === "thinking" && (
          <div className="flex justify-start">
            <div className="px-3.5 py-2.5 rounded-2xl rounded-bl-md bg-[rgba(255,255,255,0.045)] flex gap-1">
              {[0, 1, 2].map((i) => (
                <span
                  key={i}
                  className="w-1.5 h-1.5 rounded-full bg-[rgba(255,255,255,0.35)] animate-bounce"
                  style={{ animationDelay: `${i * 0.15}s` }}
                />
              ))}
            </div>
          </div>
        )}
        {turnState === "speaking" && (
          <div className="flex justify-start items-center gap-1.5 pl-1">
            {[10, 16, 8, 14, 6].map((h, i) => (
              <span
                key={i}
                className="w-[3px] rounded-full bg-[#4ade80] animate-pulse"
                style={{ height: h, animationDelay: `${i * 0.12}s`, animationDuration: "0.7s" }}
              />
            ))}
            <span className="text-[10px] text-[rgba(74,222,128,0.72)] ml-1">{t("conv.speaking")}</span>
          </div>
        )}
        </div>
      </div>

      {/* ── voice console: click = call toggle · hold = walkie-talkie ── */}
      <div className="fonos-call-dock w-[min(440px,calc(100%-40px))] mx-auto mb-5 rounded-[18px] grid grid-cols-[1fr_auto_1fr] items-center gap-3 px-4 py-3">
        <div className="relative z-10 min-w-0 flex flex-col gap-0.5">
          {inCall ? (
            <>
              <div className="text-[13px] leading-4 font-semibold tabular-nums tracking-[-0.01em] text-[#f07168]">
                {mmss}
              </div>
              <div className="text-[9.5px] leading-3.5 text-[var(--text-muted)]">
                {t("conv.call.hangup")}
              </div>
            </>
          ) : (
            <>
              <div className="text-[10.5px] leading-4 font-medium text-[var(--text-secondary)]">
                {listening ? t("conv.release") : t("conv.hold")}
              </div>
              <div className="text-[9px] leading-3.5 text-[var(--text-faint)] line-clamp-2">
                {t("conv.call.hint")}
              </div>
            </>
          )}
        </div>

        <button
          onPointerDown={onPointerDown}
          onPointerUp={onPointerUp}
          onPointerLeave={onPointerLeave}
          onPointerCancel={onPointerLeave}
          style={{ touchAction: "none" }}
          disabled={!inCall && (turnState === "thinking" || turnState === "speaking")}
          aria-label={inCall ? t("conv.call.hangup") : t("conv.hold")}
          className={`fonos-voice-button relative z-10 w-[60px] h-[60px] rounded-full flex items-center justify-center transition-all duration-300 select-none active:scale-[0.96] disabled:opacity-40 ${
            red
              ? "fonos-voice-button-live"
              : "fonos-voice-button-idle"
          }`}
        >
          {/* One handset, two orientations: state reads through form, not just
              color — a slight receiver tilt at idle (primary action: start a
              call; hold-to-talk stays a secondary gesture), rotated 135° into
              the hang-up position while in a call. */}
          <svg
            className={`fonos-voice-glyph relative z-10 transition-transform duration-300 motion-reduce:transition-none ${inCall ? "rotate-[135deg]" : "-rotate-[10deg]"}`}
            width={24}
            height={24}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.7}
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M21 16.8v2.8a1.9 1.9 0 0 1-2.08 1.9 18.7 18.7 0 0 1-8.15-2.9 18.35 18.35 0 0 1-5.66-5.66 18.7 18.7 0 0 1-2.9-8.2A1.9 1.9 0 0 1 4.1 2.67h2.8a1.9 1.9 0 0 1 1.9 1.63c.12.9.34 1.77.64 2.61a1.9 1.9 0 0 1-.43 2L7.83 10.1a15.2 15.2 0 0 0 6.07 6.07l1.19-1.19a1.9 1.9 0 0 1 2-.43c.84.3 1.71.52 2.61.64A1.9 1.9 0 0 1 21 16.8Z" />
          </svg>
        </button>

        <div className="relative z-10 flex min-w-0 flex-col items-end gap-1.5">
          <div className="flex h-4 items-center gap-[3px]" aria-hidden="true">
            {[5, 10, 7, 13, 8].map((height, i) => (
              <span
                key={i}
                className={`fonos-call-meter w-[2px] rounded-full ${red || turnState === "speaking" ? "fonos-call-meter-live" : ""}`}
                style={{ height, animationDelay: `${i * -0.11}s` }}
              />
            ))}
          </div>
          <div className="flex items-center gap-1.5">
            <span className={`w-1 h-1 rounded-full ${inCall ? "bg-[#f07168]" : "bg-[var(--accent)]"}`} />
            <span className="text-[8.5px] leading-3 font-medium tracking-[0.08em] text-[var(--text-faint)]">⌥S</span>
          </div>
        </div>
      </div>
    </div>
  );
}
