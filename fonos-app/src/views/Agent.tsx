// Agent conversation view — chat bubbles, skill execution cards, thinking indicator.
// Rendered by Dictation.tsx when dictationMode === "agent".

import { useState, useRef, useEffect, useCallback } from "react";
import {
  startRecording,
  stopRecording,
  agentProcess,
  agentReset,
  getConfig,
  generateAndPlay,
} from "../lib/api";
import type { SkillExecution } from "../types";

// ─── Types ────────────────────────────────────────────────────────────────────

interface ConversationMessage {
  id: string;
  role: "user" | "agent" | "error";
  text: string;
  skillExecutions?: SkillExecution[];
  timestamp: Date;
}

// ─── Icons ────────────────────────────────────────────────────────────────────

function MicIcon() {
  return (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <rect x="9" y="1" width="6" height="12" rx="3" />
      <path d="M5 10a7 7 0 0 0 14 0" />
      <line x1="12" y1="17" x2="12" y2="21" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="white">
      <rect x="5" y="5" width="14" height="14" rx="2" />
    </svg>
  );
}

function PlusIcon() {
  return (
    <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

// ─── Thinking indicator (three amber dots) ────────────────────────────────────

function ThinkingIndicator() {
  return (
    <div className="flex justify-start mb-3">
      <div
        className="rounded-[12px] rounded-bl-[4px] px-4 py-2.5 flex items-center gap-1.5"
        style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.04)" }}
      >
        {[0, 200, 400].map((delay) => (
          <div
            key={delay}
            className="w-[5px] h-[5px] rounded-full bg-[#fbbf24]"
            style={{ animation: `pulseDot 1.2s ease-in-out infinite`, animationDelay: `${delay}ms` }}
          />
        ))}
      </div>
    </div>
  );
}

// ─── Skill execution card ─────────────────────────────────────────────────────

function SkillCard({ skill }: { skill: SkillExecution }) {
  // Build a compact param summary from the params record
  const paramSummary = Object.values(skill.params).slice(0, 2).map(String).join(" ");

  return (
    <div className="mb-2 animate-slide-up">
      <div
        className="flex items-center gap-2 px-2 py-1.5 rounded-lg"
        style={{ background: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.03)" }}
      >
        {/* Status dot: green if success, red if blocked */}
        <div
          className="w-[5px] h-[5px] rounded-full flex-shrink-0"
          style={{ background: skill.blocked ? "#f87171" : "#86efac" }}
        />
        <span className="text-[9px] text-[rgba(255,255,255,0.3)] font-medium flex-shrink-0">
          {skill.skill_name}
        </span>
        {skill.blocked ? (
          <span className="text-[9px] text-[rgba(239,68,68,0.5)] font-mono flex-shrink-0">Blocked</span>
        ) : (
          <span className="text-[9px] text-[rgba(255,255,255,0.15)] font-mono truncate flex-1">
            {paramSummary}
          </span>
        )}
        {!skill.blocked && (
          <span className="text-[8px] text-[rgba(255,255,255,0.12)] font-mono flex-shrink-0">
            {skill.latency_ms}ms
          </span>
        )}
      </div>
    </div>
  );
}

// ─── Chat bubble ─────────────────────────────────────────────────────────────

function ChatBubble({ message }: { message: ConversationMessage }) {
  const isUser = message.role === "user";
  const isError = message.role === "error";

  if (isUser) {
    return (
      <div className="flex justify-end mb-3 animate-slide-up">
        <div
          className="max-w-[75%] rounded-[12px] rounded-br-[4px] px-3 py-2 text-[12px] leading-relaxed text-[#fafaf9]"
          style={{ background: "rgba(245,158,11,0.08)", border: "1px solid rgba(245,158,11,0.06)" }}
        >
          {message.text}
        </div>
      </div>
    );
  }

  if (isError) {
    return (
      <div className="flex justify-start mb-3 animate-slide-up">
        <div
          className="max-w-[80%] rounded-[12px] rounded-bl-[4px] px-3 py-2 text-[12px] leading-relaxed text-[rgba(239,68,68,0.8)]"
          style={{ background: "rgba(239,68,68,0.05)", border: "1px solid rgba(239,68,68,0.08)" }}
        >
          {message.text}
        </div>
      </div>
    );
  }

  // Agent message — may include skill execution cards before the bubble
  return (
    <>
      {message.skillExecutions && message.skillExecutions.length > 0 && (
        <div>
          {message.skillExecutions.map((skill, i) => (
            <SkillCard key={`${message.id}-skill-${i}`} skill={skill} />
          ))}
        </div>
      )}
      <div className="flex justify-start mb-3 animate-slide-up">
        <div
          className="max-w-[80%] rounded-[12px] rounded-bl-[4px] px-3 py-2 text-[12px] leading-relaxed text-[rgba(255,255,255,0.7)]"
          style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.04)" }}
        >
          {message.text}
        </div>
      </div>
    </>
  );
}

// ─── Empty state ──────────────────────────────────────────────────────────────

const SUGGESTIONS = ["What's my IP?", "Open Safari", "System memory", "Disk usage"];

function EmptyState({ onSuggestion }: { onSuggestion: (text: string) => void }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-5 h-full">
      <div
        className="w-16 h-16 rounded-full flex items-center justify-center"
        style={{ background: "rgba(245,158,11,0.04)", border: "1px solid rgba(245,158,11,0.06)" }}
      >
        <span className="text-[28px]" style={{ opacity: 0.5 }}>✨</span>
      </div>
      <div className="text-center">
        <div className="text-[13px] text-[rgba(255,255,255,0.25)] font-medium">Ask me anything</div>
        <div className="text-[10px] text-[rgba(255,255,255,0.1)] mt-1 leading-relaxed text-center">
          Run commands, open apps, check system info,<br />or ask questions about your Mac.
        </div>
      </div>
      <div className="flex flex-wrap gap-1.5 justify-center mt-2">
        {SUGGESTIONS.map((s) => (
          <button
            key={s}
            onClick={() => onSuggestion(s)}
            className="px-2.5 py-1 rounded-lg text-[10px] text-[rgba(255,255,255,0.2)] cursor-pointer hover:text-[rgba(255,255,255,0.4)] transition-colors"
            style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.04)" }}
          >
            {s}
          </button>
        ))}
      </div>
    </div>
  );
}

// ─── AgentView ────────────────────────────────────────────────────────────────

interface AgentViewProps {
  /** Conversation messages persisted in parent so they survive mode switching. */
  messages: ConversationMessage[];
  onMessagesChange: (msgs: ConversationMessage[]) => void;
}

export default function AgentView({ messages, onMessagesChange }: AgentViewProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [recordDuration, setRecordDuration] = useState(0);
  const [ttsEnabled, setTtsEnabled] = useState(false);
  const durationRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when messages change
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, isProcessing]);

  // Load TTS preference from config
  useEffect(() => {
    getConfig().then((cfg) => {
      setTtsEnabled(cfg.agent_tts_enabled ?? false);
    }).catch(() => {});
  }, []);

  // Recording duration timer
  useEffect(() => {
    if (isRecording) {
      setRecordDuration(0);
      const start = Date.now();
      durationRef.current = setInterval(() => setRecordDuration((Date.now() - start) / 1000), 100);
    } else {
      if (durationRef.current) clearInterval(durationRef.current);
    }
    return () => { if (durationRef.current) clearInterval(durationRef.current); };
  }, [isRecording]);

  const addMessage = useCallback((msg: Omit<ConversationMessage, "id" | "timestamp">) => {
    const full: ConversationMessage = {
      ...msg,
      id: `${Date.now()}-${Math.random()}`,
      timestamp: new Date(),
    };
    onMessagesChange([...messages, full]);
    return full;
  }, [messages, onMessagesChange]);

  const processText = useCallback(async (text: string) => {
    // Add user message
    const updatedMessages: ConversationMessage[] = [
      ...messages,
      { id: `${Date.now()}-u`, role: "user", text, timestamp: new Date() },
    ];
    onMessagesChange(updatedMessages);
    setIsProcessing(true);

    try {
      const result = await agentProcess(text);
      const agentMsg: ConversationMessage = {
        id: `${Date.now()}-a`,
        role: "agent",
        text: result.response_text,
        skillExecutions: result.skill_executions,
        timestamp: new Date(),
      };
      onMessagesChange([...updatedMessages, agentMsg]);
      // Speak the response if TTS is enabled
      if (ttsEnabled && result.response_text) {
        generateAndPlay(result.response_text, "default", 1.0).catch(() => {});
      }
    } catch (e: unknown) {
      const errorMsg: ConversationMessage = {
        id: `${Date.now()}-e`,
        role: "error",
        text: e instanceof Error ? e.message : String(e),
        timestamp: new Date(),
      };
      onMessagesChange([...updatedMessages, errorMsg]);
    } finally {
      setIsProcessing(false);
    }
  }, [messages, onMessagesChange]);

  const handleMicClick = useCallback(async () => {
    if (isProcessing) return;

    if (isRecording) {
      setIsRecording(false);
      setIsProcessing(true);
      try {
        const result = await stopRecording(undefined);
        if (result.text && result.text.trim()) {
          await processText(result.text.trim());
        }
      } catch (e: unknown) {
        addMessage({ role: "error", text: e instanceof Error ? e.message : String(e) });
      } finally {
        setIsProcessing(false);
      }
    } else {
      try {
        await startRecording();
        setIsRecording(true);
      } catch (e: unknown) {
        addMessage({ role: "error", text: e instanceof Error ? e.message : String(e) });
      }
    }
  }, [isRecording, isProcessing, processText, addMessage]);

  const handleNewConversation = useCallback(async () => {
    try {
      await agentReset();
    } catch {
      // Even if the backend reset fails, clear the local messages
    }
    onMessagesChange([]);
  }, [onMessagesChange]);

  const handleSuggestion = useCallback((text: string) => {
    if (isProcessing || isRecording) return;
    processText(text);
  }, [isProcessing, isRecording, processText]);

  return (
    <>
      {/* Keyframe styles injected inline */}
      <style>{`
        @keyframes pulseDot {
          0%, 100% { opacity: 0.4; }
          50% { opacity: 1; }
        }
        @keyframes slideUp {
          from { opacity: 0; transform: translateY(8px); }
          to { opacity: 1; transform: translateY(0); }
        }
        .animate-slide-up {
          animation: slideUp 0.3s ease-out;
        }
      `}</style>

      {/* ── Header ── */}
      <div className="flex items-center justify-between px-5 py-2 flex-shrink-0">
        <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.2)] font-medium">
          Conversation
        </span>
        {messages.length > 0 && (
          <button
            onClick={handleNewConversation}
            className="text-[9px] text-[rgba(255,255,255,0.12)] hover:text-[rgba(255,255,255,0.3)] transition-colors flex items-center gap-1"
          >
            <PlusIcon />
            New
          </button>
        )}
      </div>

      {/* ── Conversation area ── */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto min-h-0 px-5 pb-3"
        style={{ scrollBehavior: "smooth" }}
      >
        {messages.length === 0 && !isProcessing ? (
          <EmptyState onSuggestion={handleSuggestion} />
        ) : (
          <>
            {messages.map((msg) => (
              <ChatBubble key={msg.id} message={msg} />
            ))}
            {isProcessing && <ThinkingIndicator />}
          </>
        )}
      </div>

      {/* ── Mic button area ── */}
      <div className="flex-shrink-0 pb-5 pt-2">
        <div className="flex flex-col items-center">
          <button
            onClick={handleMicClick}
            disabled={isProcessing}
            className={[
              "w-14 h-14 rounded-full flex items-center justify-center transition-all duration-300",
              isRecording
                ? "bg-gradient-to-br from-[#ef4444] to-[#dc2626] shadow-[0_0_40px_rgba(239,68,68,0.35)] scale-105"
                : "bg-gradient-to-br from-[#f59e0b] to-[#d97706] shadow-[0_0_30px_rgba(245,158,11,0.2)]",
              isProcessing ? "opacity-50 cursor-not-allowed" : "",
            ].join(" ")}
          >
            {isRecording ? <StopIcon /> : isProcessing ? (
              <span className="text-white text-sm font-bold">&middot;&middot;&middot;</span>
            ) : <MicIcon />}
          </button>
          <span className="text-[10px] font-mono text-[rgba(255,255,255,0.18)] mt-1.5">
            {isRecording ? `${recordDuration.toFixed(1)}s` : "Tap to speak"}
          </span>
        </div>
      </div>
    </>
  );
}

// Re-export ConversationMessage type so Dictation.tsx can use it
export type { ConversationMessage };
