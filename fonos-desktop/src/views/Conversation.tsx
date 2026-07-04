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

type TurnState = "idle" | "listening" | "thinking" | "speaking";

interface Message {
  role: "user" | "assistant" | "error";
  text: string;
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
  // Mirror of `inCall` for pointer handlers / timers (avoids stale closures).
  const inCallRef = useRef(false);
  // Press disambiguation: a short tap toggles the call; a ≥250ms hold is the
  // existing walkie-talkie press. `pressTimerRef` fires when the hold threshold
  // passes; `holdStartedRef` records that the walkie-talkie leg engaged.
  const pressTimerRef = useRef<number | null>(null);
  const holdStartedRef = useRef(false);
  const CLICK_MS = 250;

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
        const un = await listen<{ kind: string; text: string }>("sts:event", (e) => {
          const { kind, text } = e.payload;
          if (kind === "transcript") {
            setMessages((m) => [...m, { role: "user", text }]);
            setTurnState("thinking");
          } else if (kind === "reply") {
            setMessages((m) => [...m, { role: "assistant", text }]);
          } else if (kind === "speaking_started") {
            setTurnState("speaking");
          } else if (kind === "speaking_done" || kind === "turn_done") {
            setTurnState("idle");
          } else if (kind === "call_started") {
            setInCall(true);
          } else if (kind === "call_listening") {
            setInCall(true);
            setTurnState("listening");
          } else if (kind === "call_ended") {
            setInCall(false);
            setTurnState("idle");
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
    <div className="h-full flex flex-col">
      {/* ── header ── */}
      <div className="flex items-center justify-between px-5 pt-4 pb-3">
        <div className="flex items-center gap-3">
          <h1 className="text-[15px] font-semibold text-[#fafaf9]">{t("conv.title")}</h1>
          <span
            className="flex items-center gap-1.5 text-[10px] px-2 py-0.5 rounded-full bg-[rgba(255,255,255,0.04)]"
            style={{ color: meta.color }}
          >
            <span
              className={`w-1.5 h-1.5 rounded-full ${turnState !== "idle" ? "animate-pulse" : ""}`}
              style={{ background: meta.color }}
            />
            {t(meta.label)}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setPersonaOpen((v) => !v)}
            className={`text-[10px] px-2.5 py-1 rounded-md transition-colors ${
              personaOpen
                ? "bg-[rgba(251,191,36,0.12)] text-[#fbbf24]"
                : "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)]"
            }`}
          >
            {t("conv.persona")}
          </button>
          <button
            onClick={handleReset}
            className="text-[10px] px-2.5 py-1 rounded-md bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] transition-colors"
          >
            {t("conv.newchat")}
          </button>
        </div>
      </div>

      {/* ── persona drawer ── */}
      {personaOpen && (
        <div className="mx-5 mb-3 p-3 rounded-xl bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)]">
          <div className="text-[10px] text-[rgba(255,255,255,0.35)] mb-1.5">
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
            className="w-full bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-2 text-[#fafaf9] text-[11px] leading-relaxed resize-y focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
          />
          <div className="flex items-center justify-between mt-1.5">
            <span className="text-[9px] text-[rgba(255,255,255,0.2)]">
              {t("conv.persona.applies")}{personaDirty ? t("conv.persona.unsaved") : ""}
            </span>
            <button
              onClick={savePersonaDefault}
              disabled={!personaDirty}
              className="text-[9px] px-2 py-1 rounded-md bg-[rgba(251,191,36,0.1)] text-[#fbbf24] disabled:opacity-30 transition-opacity"
            >
              {t("conv.persona.savedefault")}
            </button>
          </div>
        </div>
      )}

      {/* ── transcript ── */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-5 pb-2 flex flex-col gap-2.5">
        {messages.length === 0 && turnState === "idle" && (
          <div className="flex-1 flex flex-col items-center justify-center gap-3 text-center">
            <div className="w-14 h-14 rounded-full bg-[rgba(251,191,36,0.06)] flex items-center justify-center">
              <svg width={26} height={26} viewBox="0 0 24 24" fill="none" stroke="#fbbf24" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
                <path d="M12 2a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z" />
                <path d="M19 10v1a7 7 0 0 1-14 0v-1" />
                <line x1="12" y1="18" x2="12" y2="22" />
              </svg>
            </div>
            <div className="text-[12px] text-[rgba(255,255,255,0.5)]">{t("conv.empty.title")}</div>
            <div className="text-[10px] text-[rgba(255,255,255,0.25)]">
              ⌥S {t("conv.empty.hint")} {config?.sts_max_turns ?? 8} {t("conv.empty.turns")}
            </div>
          </div>
        )}

        {messages.map((m, i) => (
          <div key={i} className={`flex ${m.role === "user" ? "justify-end" : "justify-start"}`}>
            <div
              className={`max-w-[78%] px-3 py-2 rounded-2xl text-[11.5px] leading-relaxed whitespace-pre-wrap ${
                m.role === "user"
                  ? "bg-[rgba(251,191,36,0.1)] text-[#fde68a] rounded-br-md"
                  : m.role === "assistant"
                    ? "bg-[rgba(255,255,255,0.045)] text-[#e7e5e4] rounded-bl-md"
                    : "bg-[rgba(239,68,68,0.08)] text-[rgba(252,165,165,0.9)] rounded-bl-md"
              }`}
            >
              {m.text}
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
            <span className="text-[9px] text-[rgba(74,222,128,0.6)] ml-1">{t("conv.speaking")}</span>
          </div>
        )}
      </div>

      {/* ── talk button: click = call toggle · hold = walkie-talkie ── */}
      <div className="flex flex-col items-center gap-2 pb-6 pt-2">
        <button
          onPointerDown={onPointerDown}
          onPointerUp={onPointerUp}
          onPointerLeave={onPointerLeave}
          onPointerCancel={onPointerLeave}
          style={{ touchAction: "none" }}
          disabled={!inCall && (turnState === "thinking" || turnState === "speaking")}
          aria-label={inCall ? t("conv.call.hangup") : t("conv.hold")}
          className={`relative w-[72px] h-[72px] rounded-full flex items-center justify-center transition-all select-none disabled:opacity-40 ${
            red
              ? "bg-[#ef4444] scale-110 shadow-[0_0_0_10px_rgba(239,68,68,0.15),0_0_0_20px_rgba(239,68,68,0.06)]"
              : "bg-[rgba(251,191,36,0.9)] hover:bg-[#fbbf24] shadow-[0_4px_20px_rgba(251,191,36,0.25)]"
          }`}
        >
          {red && (
            <span className="absolute inset-0 rounded-full bg-[rgba(239,68,68,0.4)] animate-ping" />
          )}
          {inCall ? (
            // Hang-up (phone-off) glyph.
            <svg width={26} height={26} viewBox="0 0 24 24" fill="none" stroke="#1a1917" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
              <path d="M10.68 13.31a16 16 0 0 0 3.41 2.6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.29.62 2 2 0 0 1 1.72 2v2a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.42 19.42 0 0 1-3.33-2.67" />
              <path d="M22 2 2 22" />
              <path d="M5 12.66a19.4 19.4 0 0 1-2.68-6.14A2 2 0 0 1 4.31 4H6a2 2 0 0 1 2 1.72c.13.85.32 1.68.57 2.49" />
            </svg>
          ) : (
            <svg width={28} height={28} viewBox="0 0 24 24" fill="none" stroke="#1a1917" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 2a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z" />
              <path d="M19 10v1a7 7 0 0 1-14 0v-1" />
              <line x1="12" y1="18" x2="12" y2="22" />
            </svg>
          )}
        </button>
        {inCall ? (
          <>
            <div className="text-[11px] font-medium tabular-nums text-[#ef4444]">{mmss}</div>
            <div className="text-[9px] text-[rgba(255,255,255,0.25)]">{t("conv.call.hangup")}</div>
          </>
        ) : (
          <>
            <div className="text-[9px] text-[rgba(255,255,255,0.25)]">
              {listening ? t("conv.release") : t("conv.hold")}
            </div>
            <div className="text-[9px] text-[rgba(255,255,255,0.18)]">{t("conv.call.hint")}</div>
          </>
        )}
      </div>
    </div>
  );
}
