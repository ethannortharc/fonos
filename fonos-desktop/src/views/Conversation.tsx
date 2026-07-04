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
} from "../lib/api";
import type { AppConfig } from "../types";

type TurnState = "idle" | "listening" | "thinking" | "speaking";

interface Message {
  role: "user" | "assistant" | "error";
  text: string;
}

const STATE_META: Record<TurnState, { label: string; color: string }> = {
  idle: { label: "Ready", color: "#78716c" },
  listening: { label: "Listening…", color: "#f87171" },
  thinking: { label: "Thinking…", color: "#fbbf24" },
  speaking: { label: "Speaking…", color: "#4ade80" },
};

export default function Conversation() {
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
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<{ kind: string; text: string }>("sts:event", (e) => {
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
          } else if (kind === "error") {
            setMessages((m) => [...m, { role: "error", text }]);
            setTurnState("idle");
          }
        });
      } catch {
        // demo mode: no event bridge
      }
    })();
    return () => unlisten?.();
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

  return (
    <div className="h-full flex flex-col">
      {/* ── header ── */}
      <div className="flex items-center justify-between px-5 pt-4 pb-3">
        <div className="flex items-center gap-3">
          <h1 className="text-[15px] font-semibold text-[#fafaf9]">Conversation</h1>
          <span
            className="flex items-center gap-1.5 text-[10px] px-2 py-0.5 rounded-full bg-[rgba(255,255,255,0.04)]"
            style={{ color: meta.color }}
          >
            <span
              className={`w-1.5 h-1.5 rounded-full ${turnState !== "idle" ? "animate-pulse" : ""}`}
              style={{ background: meta.color }}
            />
            {meta.label}
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
            Persona
          </button>
          <button
            onClick={handleReset}
            className="text-[10px] px-2.5 py-1 rounded-md bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] transition-colors"
          >
            New chat
          </button>
        </div>
      </div>

      {/* ── persona drawer ── */}
      {personaOpen && (
        <div className="mx-5 mb-3 p-3 rounded-xl bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)]">
          <div className="text-[10px] text-[rgba(255,255,255,0.35)] mb-1.5">
            System prompt for this conversation — replies are spoken aloud, keep them short.
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
              Applies from the next turn{personaDirty ? " · unsaved" : ""}
            </span>
            <button
              onClick={savePersonaDefault}
              disabled={!personaDirty}
              className="text-[9px] px-2 py-1 rounded-md bg-[rgba(251,191,36,0.1)] text-[#fbbf24] disabled:opacity-30 transition-opacity"
            >
              Save as default
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
            <div className="text-[12px] text-[rgba(255,255,255,0.5)]">Hold the button and talk</div>
            <div className="text-[10px] text-[rgba(255,255,255,0.25)]">
              ⌥S works from anywhere · memory lasts {config?.sts_max_turns ?? 8} turns
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
            <span className="text-[9px] text-[rgba(74,222,128,0.6)] ml-1">speaking</span>
          </div>
        )}
      </div>

      {/* ── hold-to-talk ── */}
      <div className="flex flex-col items-center gap-2 pb-6 pt-2">
        <button
          onMouseDown={holdStart}
          onMouseUp={holdStop}
          onMouseLeave={() => holdingRef.current && holdStop()}
          onTouchStart={holdStart}
          onTouchEnd={holdStop}
          disabled={turnState === "thinking" || turnState === "speaking"}
          className={`relative w-[72px] h-[72px] rounded-full flex items-center justify-center transition-all select-none disabled:opacity-40 ${
            listening
              ? "bg-[#ef4444] scale-110 shadow-[0_0_0_10px_rgba(239,68,68,0.15),0_0_0_20px_rgba(239,68,68,0.06)]"
              : "bg-[rgba(251,191,36,0.9)] hover:bg-[#fbbf24] shadow-[0_4px_20px_rgba(251,191,36,0.25)]"
          }`}
        >
          {listening && (
            <span className="absolute inset-0 rounded-full bg-[rgba(239,68,68,0.4)] animate-ping" />
          )}
          <svg width={28} height={28} viewBox="0 0 24 24" fill="none" stroke="#1a1917" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 2a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z" />
            <path d="M19 10v1a7 7 0 0 1-14 0v-1" />
            <line x1="12" y1="18" x2="12" y2="22" />
          </svg>
        </button>
        <div className="text-[9px] text-[rgba(255,255,255,0.25)]">
          {listening ? "Release to send" : "Hold to talk"}
        </div>
      </div>
    </div>
  );
}
