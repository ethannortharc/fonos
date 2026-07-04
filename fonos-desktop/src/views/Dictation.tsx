// Dictation view — jumping blocks + mic + drum-roller mode slider + activity.

import React, { useState, useRef, useEffect, useCallback } from "react";
import { MicIcon as MicSvg, TranscriptIcon, HourglassIcon, SparklesIcon, AlertIcon, PinIcon, NotebookIcon, ModeIcon } from "../components/Icons";
import {
  hasMicrophone,
  startRecording,
  stopRecording,
  processWithLlm,
  listModes,
  getConfig,
} from "../lib/api";
import { listContainers } from "../lib/storage-api";
import { t, useT } from "../lib/i18n";
import type { Container } from "../lib/storage-api";
import type { ModeEntry, ModelProfile } from "../types";

// ─── Jumping color blocks (Canvas) — only when recording ─────────────────────

function JumpingBlocks({ active }: { active: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animRef = useRef<number>(0);
  const tRef = useRef(0);
  const fadeRef = useRef(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);
    const W = rect.width;
    const H = rect.height;
    const barCount = 48;
    const gap = 2.5;
    // Don't span full width — leave ~15% margin on each side
    const margin = W * 0.12;
    const usableW = W - margin * 2;
    const barW = (usableW - (barCount - 1) * gap) / barCount;
    const maxH = H * 0.45;
    const baseY = H * 0.72;

    const draw = () => {
      const target = active ? 1 : 0;
      fadeRef.current += (target - fadeRef.current) * 0.04;
      const fade = fadeRef.current;

      ctx.clearRect(0, 0, W, H);
      if (fade < 0.005) { animRef.current = requestAnimationFrame(draw); return; }

      tRef.current += 0.03;
      const t = tRef.current;

      for (let i = 0; i < barCount; i++) {
        const x = margin + i * (barW + gap);

        // Normalized position: -1 (left edge) to +1 (right edge)
        const nx = (i - (barCount - 1) / 2) / ((barCount - 1) / 2);
        const absNx = Math.abs(nx);

        // Sound radiating from mic center:
        // - Near center (mic): short bars (mic is there)
        // - Mid range: tallest bars (sound radiates outward)
        // - Edges: bars shrink and fade (sound dissipates)
        const micClear = 1 - Math.exp(-absNx * absNx * 8); // 0 at center, ~1 at mid
        const edgeDecay = Math.max(0, 1 - Math.pow(absNx, 2.5) * 1.2); // 1 at mid, 0 at edges
        const heightEnvelope = micClear * edgeDecay;

        // Organic height — layered waves
        const h1 = Math.sin(i * 0.4 + t * 1.2) * 0.5 + 0.5;
        const h2 = Math.sin(i * 0.7 + t * 0.8 + 2) * 0.3 + 0.5;
        const h3 = Math.sin(i * 0.2 + t * 1.6 - 1) * 0.2 + 0.5;
        const h = (h1 * 0.5 + h2 * 0.3 + h3 * 0.2) * maxH * heightEnvelope * fade;
        const barH = Math.max(1.5, h);

        // Opacity: bright near-center, smoothly fades to invisible at edges
        const opacityEnvelope = micClear * Math.max(0, 1 - Math.pow(absNx, 1.8) * 0.95);
        const opacity = (0.12 + opacityEnvelope * 0.5) * fade;
        ctx.fillStyle = `rgba(251, 191, 36, ${opacity})`;
        ctx.beginPath();
        ctx.roundRect(x, baseY - barH, barW, barH, 1.5);
        ctx.fill();
      }

      animRef.current = requestAnimationFrame(draw);
    };

    animRef.current = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(animRef.current);
  }, [active]);

  return <canvas ref={canvasRef} className="absolute inset-0 w-full h-full pointer-events-none" />;
}

// ─── Horizontal drum-roller mode selector ────────────────────────────────────
// Like the float pill's vertical roller, but horizontal.
// Shows 3 items: prev | CURRENT | next. Drag or scroll to cycle. Circular/infinite.

function ModeDrum({
  modes, current, onChange,
}: {
  modes: ModeEntry[]; current: string; onChange: (id: string) => void;
}) {
  const idx = Math.max(0, modes.findIndex((m) => m.id === current));
  const dragRef = useRef({ active: false, startX: 0, moved: 0 });

  const modeAt = (i: number) => modes[((i % modes.length) + modes.length) % modes.length];
  const go = (dir: number) => {
    if (modes.length === 0) return;
    const ni = ((idx + dir) % modes.length + modes.length) % modes.length;
    onChange(modes[ni].id);
  };
  const selectAt = (offset: number) => {
    if (modes.length === 0) return;
    const ni = ((idx + offset) % modes.length + modes.length) % modes.length;
    onChange(modes[ni].id);
  };

  const onDown = (e: React.PointerEvent) => {
    dragRef.current = { active: true, startX: e.clientX, moved: 0 };
  };
  const onMove = (e: React.PointerEvent) => {
    if (!dragRef.current.active) return;
    dragRef.current.moved = e.clientX - dragRef.current.startX;
  };
  const onUp = () => {
    const m = dragRef.current.moved;
    dragRef.current.active = false;
    // Only trigger drag-switch if moved enough (not a click)
    if (Math.abs(m) > 25) {
      go(m < 0 ? 1 : -1);
    }
  };

  if (modes.length === 0) {
    return <div className="text-[10px] text-[rgba(255,255,255,0.12)] text-center py-2">{t("dict.no-modes")}</div>;
  }

  return (
    <div
      className="relative flex items-center justify-center h-10 select-none cursor-grab active:cursor-grabbing"
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onWheel={(e) => { e.preventDefault(); go(e.deltaX > 0 || e.deltaY > 0 ? 1 : -1); }}
    >
      {/* No background highlight — text gradient alone provides the visual cue */}

      {/* Fade edges */}
      <div className="absolute left-0 top-0 bottom-0 w-24 bg-gradient-to-r from-[#1a1917] to-transparent z-10 pointer-events-none" />
      <div className="absolute right-0 top-0 bottom-0 w-24 bg-gradient-to-l from-[#1a1917] to-transparent z-10 pointer-events-none" />

      {/* 3-column layout: left items | center item | right items — center always at 50% */}
      {(() => {
        const renderSlot = (offset: number) => {
          const m = modeAt(idx + offset);
          const isCenter = offset === 0;
          const dist = Math.abs(offset);
          const t = dist / 3;
          const baseOpacity = Math.max(0.06, 1 - t * t);
          const opacity = baseOpacity;
          const scale = 1 - t * 0.1;
          const amberAmount = Math.max(0, 1 - t * 0.9);
          const r = Math.round(251 * amberAmount + 180 * (1 - amberAmount));
          const g = Math.round(191 * amberAmount + 180 * (1 - amberAmount));
          const b = Math.round(36 * amberAmount + 180 * (1 - amberAmount));
          const textAlpha = Math.max(0.1, 1 - t * t);
          const textColor = `rgba(${r}, ${g}, ${b}, ${textAlpha})`;
          return (
            <button
              key={`${offset}-${m.id}`}
              onPointerUp={(e) => {
                if (Math.abs(dragRef.current.moved) < 10 && !isCenter) {
                  e.stopPropagation();
                  selectAt(offset);
                }
              }}
              className={[
                "flex items-center gap-1 py-1.5 transition-all duration-300 whitespace-nowrap flex-shrink-0",
                isCenter ? "px-3" : "px-2 cursor-pointer",
              ].join(" ")}
              style={{ opacity, transform: `scale(${scale})` }}
            >
              <span style={{ color: textColor }}><ModeIcon icon={m.icon} size={isCenter ? 14 : Math.max(10, 13 - dist * 1)} /></span>
              <span style={{
                color: textColor,
                fontSize: isCenter ? 12 : Math.max(9, 11 - dist * 0.7),
                fontWeight: isCenter ? 600 : 500,
                whiteSpace: "nowrap",
              }}>{m.name}</span>
            </button>
          );
        };

        return (
          <div className="flex items-center w-full transition-transform duration-200">
            <div className="flex-1 flex justify-end overflow-hidden">
              {[-3, -2, -1].map(renderSlot)}
            </div>
            <div className="flex-shrink-0">{renderSlot(0)}</div>
            <div className="flex-1 flex justify-start overflow-hidden">
              {[1, 2].map(renderSlot)}
            </div>
          </div>
        );
      })()}
    </div>
  );
}

// ─── Icons ───────────────────────────────────────────────────────────────────

function MicIcon() {
  return (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <rect x="9" y="1" width="6" height="12" rx="3" /><path d="M5 10a7 7 0 0 0 14 0" /><line x1="12" y1="17" x2="12" y2="21" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="white">
      <rect x="5" y="5" width="14" height="14" rx="2" />
    </svg>
  );
}

// ─── Activity entry ──────────────────────────────────────────────────────────

interface ActivityEntry {
  id: string;
  type: "recording" | "transcript" | "processing" | "result" | "error";
  icon: React.ReactNode;
  label: string;
  content?: string;
  model?: string;
  latency?: number;
  tokens?: string;
  duration?: number;
  timestamp: Date;
}

// ─── Dictation view ──────────────────────────────────────────────────────────

export default function Dictation() {
  useT();
  const [modes, setModes] = useState<ModeEntry[]>([]);
  const [dictationMode, setDictationMode] = useState<string>("raw");
  const [notebooks, setNotebooks] = useState<Container[]>([]);
  // null = Quick Note (no specific notebook), number = notebook id
  const [selectedNotebookId, setSelectedNotebookId] = useState<number | null>(null);
  const [isRecording, setIsRecording] = useState(false);
  const [hasMic, setHasMic] = useState<boolean | null>(null);
  const [processing, setProcessing] = useState(false);
  const [recordDuration, setRecordDuration] = useState(0);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const [sttModel, setSttModel] = useState("");
  const [llmModel, setLlmModel] = useState("");
  const [profiles, setProfiles] = useState<ModelProfile[]>([]);
  const durationRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const activityRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (activityRef.current) activityRef.current.scrollTop = activityRef.current.scrollHeight;
  }, [activity]);

  useEffect(() => {
    listModes().then(setModes).catch(() => {});
    hasMicrophone().then(setHasMic).catch(() => setHasMic(false));
    getConfig().then((cfg) => {
      if (cfg.dictation_mode) setDictationMode(cfg.dictation_mode);
      setProfiles(cfg.model_profiles);
      const sttP = cfg.model_profiles.find((p) => p.id === cfg.stt_profile);
      const llmP = cfg.model_profiles.find((p) => p.id === cfg.llm_profile);
      if (sttP) setSttModel(`${sttP.name}${sttP.model ? " · " + sttP.model : ""}`);
      if (llmP) setLlmModel(`${llmP.name}${llmP.model ? " · " + llmP.model : ""}`);
    }).catch(() => {});
    listContainers()
      .then((all) => setNotebooks(all.filter((c) => c.container_type === "notebook")))
      .catch(() => {});
  }, []);

  // When dictationMode changes away from "note", clear the notebook selection
  useEffect(() => {
    if (dictationMode !== "note") {
      setSelectedNotebookId(null);
    }
  }, [dictationMode]);

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

  // Surface background pipeline/permission errors (hotkey-driven flows that
  // bypass this view's own try/catch) into the activity feed. The payload is
  // the structured {message, pane} JSON from error_surface::emit_float_error,
  // or a plain string (backward compatible).
  useEffect(() => {
    const cleanup: (() => void)[] = [];
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        cleanup.push(await listen<string>("float:error", (event) => {
          const raw = typeof event.payload === "string" ? event.payload : String(event.payload ?? "");
          let message = raw;
          try {
            const parsed = JSON.parse(raw);
            if (parsed && typeof parsed === "object" && "message" in parsed) {
              message = (parsed as { message?: string }).message ?? raw;
            }
          } catch {
            // Plain-string payload — use it as-is.
          }
          if (!message) return;
          setActivity((prev) => {
            // Best-effort dedup: skip if the most recent entry is an identical
            // error added within the last 1.5s (e.g. the in-view catch below
            // already recorded it). Errors with differing text still appear.
            const last = prev[prev.length - 1];
            if (last && last.type === "error" && last.content === message &&
                Date.now() - last.timestamp.getTime() < 1500) {
              return prev;
            }
            return [...prev, {
              id: `${Date.now()}-fe-${Math.random()}`,
              timestamp: new Date(),
              type: "error" as const,
              icon: <AlertIcon size={12} />,
              label: t("dict.error"),
              content: message,
            }];
          });
        }));
      } catch {
        // Not running under Tauri.
      }
    })();
    return () => { cleanup.forEach((fn) => fn()); };
  }, []);

  // Select a notebook for note-mode dictation: switches mode to "note"
  const handleSelectNotebook = useCallback(async (id: number | null) => {
    setSelectedNotebookId(id);
    setDictationMode("note");
    // Tell Rust which notebook to save entries to
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      // If null (Quick Note), find Quick Note's real container ID
      const targetId = id ?? notebooks.find((n) => n.title === "Quick Note")?.id ?? 0;
      await invoke("set_note_notebook", { container_id: targetId });
    } catch (e) {
      console.error("set_note_notebook:", e);
    }
  }, [notebooks]);

  const addActivity = useCallback((entry: Omit<ActivityEntry, "id" | "timestamp">) => {
    setActivity((prev) => [...prev, { ...entry, id: `${Date.now()}-${Math.random()}`, timestamp: new Date() }]);
  }, []);

  const handleStartStop = useCallback(async () => {
    if (isRecording) {
      // Resolve actual model names from mode overrides (not just global defaults)
      const currentMode = modes.find((m) => m.id === dictationMode);
      const profileLabel = (id: string) => {
        const p = profiles.find((pr) => pr.id === id);
        return p ? `${p.name}${p.model ? " · " + p.model : ""}` : id;
      };
      const actualStt = currentMode?.stt_model === "apple-speech"
        ? "Apple Speech"
        : currentMode?.stt_model ? profileLabel(currentMode.stt_model) : sttModel;
      const actualLlm = currentMode?.model ? profileLabel(currentMode.model) : llmModel;

      const notebookLabel = dictationMode === "note"
        ? (selectedNotebookId !== null
            ? (notebooks.find((n) => n.id === selectedNotebookId)?.title ?? t("dict.notebook"))
            : t("dict.quick-note"))
        : undefined;

      setIsRecording(false);
      setProcessing(true);
      addActivity({
        type: "recording",
        icon: <MicSvg size={12} />,
        label: notebookLabel ? `${t("dict.recorded")} → ${notebookLabel}` : t("dict.recorded"),
        duration: recordDuration,
        model: actualStt || undefined,
      });
      try {
        const result = await stopRecording(dictationMode);
        if (result.text) {
          const sttDisplay = result.stt_engine
            ? `${actualStt} (${result.stt_engine})`
            : actualStt;
          const preprocBadge = (result.noise_removed_pct > 0.5 || Math.abs(result.gain_db) > 0.5)
            ? `HPF: -${result.noise_removed_pct.toFixed(1)}% ${t("dict.noise")} | Norm: ${result.gain_db >= 0 ? "+" : ""}${result.gain_db.toFixed(1)}dB`
            : undefined;
          addActivity({ type: "transcript", icon: <TranscriptIcon size={12} />, label: t("dict.transcript"), content: result.text, latency: result.latency_ms, duration: result.duration_secs, model: sttDisplay || undefined, tokens: preprocBadge });
          if (currentMode?.system || currentMode?.user_template) {
            addActivity({ type: "processing", icon: <HourglassIcon size={12} />, label: t("dict.processing") });
            try {
              const llm = await processWithLlm(result.text, dictationMode);
              // stop_recording left the float pill in "Processing" for LLM
              // modes (it suppresses its own float:stop so "Done" can't show
              // before the LLM step). Now that the LLM step resolved, emit the
              // real final event so the pill shows "Done".
              try {
                const { emit } = await import("@tauri-apps/api/event");
                await emit("float:stop", llm.processed);
              } catch {
                // Not running under Tauri (demo/web) — no float pill to update.
              }
              setActivity((prev) => {
                const filtered = prev.filter((e) => e.type !== "processing");
                const me = modes.find((m) => m.id === dictationMode);
                return [...filtered, { id: `${Date.now()}-r`, timestamp: new Date(), type: "result" as const, icon: me?.icon ? <ModeIcon icon={me.icon} size={12} /> : <SparklesIcon size={12} />, label: llm.mode_name || dictationMode, content: llm.processed, latency: llm.latency_ms, model: actualLlm || undefined }];
              });
            } catch (e: unknown) {
              const msg = e instanceof Error ? e.message : String(e);
              // The pill is stuck in "Processing" (stop_recording suppressed its
              // float:stop for this LLM mode) — surface the failure on it too.
              try {
                const { emit } = await import("@tauri-apps/api/event");
                await emit("float:error", msg);
              } catch {
                // Not running under Tauri.
              }
              setActivity((prev) => [...prev.filter((x) => x.type !== "processing"), { id: `${Date.now()}-e`, timestamp: new Date(), type: "error" as const, icon: <AlertIcon size={12} />, label: t("dict.error"), content: msg }]);
            }
          }
        } else {
          addActivity({ type: "transcript", icon: <TranscriptIcon size={12} />, label: t("dict.no-speech"), latency: result.latency_ms });
        }
      } catch (e: unknown) {
        addActivity({ type: "error", icon: <AlertIcon size={12} />, label: t("dict.error"), content: e instanceof Error ? e.message : String(e) });
      } finally { setProcessing(false); }
    } else {
      setActivity([]);
      try { await startRecording(); setIsRecording(true); }
      catch (e: unknown) { addActivity({ type: "error", icon: <AlertIcon size={12} />, label: t("dict.error"), content: e instanceof Error ? e.message : String(e) }); }
    }
  }, [isRecording, dictationMode, selectedNotebookId, notebooks, recordDuration, addActivity, modes, sttModel, llmModel, profiles]);

  // Derive the current notebook for display in the activity label
  const currentNotebook = notebooks.find((n) => n.id === selectedNotebookId) ?? null;

  return (
    <div className="flex flex-col h-full bg-[#1a1917]">
      {/* ══ Top: Recording panel ══ */}
      <div className="relative flex flex-col items-center justify-center flex-shrink-0" style={{ height: 200 }}>
        {/* Layer 1: Jumping blocks — hidden when idle, fades in when recording */}
        <JumpingBlocks active={isRecording} />

        {/* Center blur — clears space for mic */}
        <div className="absolute z-[2] rounded-full pointer-events-none"
          style={{ width: 100, height: 100, left: "50%", top: "38%", transform: "translate(-50%, -50%)",
            background: "radial-gradient(circle, #1a1917 28%, rgba(26,25,23,0.9) 48%, transparent 70%)" }} />

        {/* Layer 2: Mic button */}
        <div className="relative z-[5] flex flex-col items-center">
          <button
            onClick={handleStartStop}
            disabled={processing || hasMic === false}
            className={[
              "w-16 h-16 rounded-full flex items-center justify-center transition-all duration-300",
              isRecording
                ? "bg-gradient-to-br from-[#ef4444] to-[#dc2626] shadow-[0_0_40px_rgba(239,68,68,0.35)] scale-105"
                : "bg-gradient-to-br from-[#f59e0b] to-[#d97706] shadow-[0_0_30px_rgba(245,158,11,0.2)]",
              processing ? "opacity-50 cursor-not-allowed" : "",
            ].join(" ")}
          >
            {isRecording ? <StopIcon /> : processing ? (
              <span className="text-white text-sm font-bold">&middot;&middot;&middot;</span>
            ) : <MicIcon />}
          </button>
          <span className="text-[10px] font-mono text-[rgba(255,255,255,0.18)] mt-1.5">
            {isRecording ? `${recordDuration.toFixed(1)}s` : hasMic === false ? t("dict.no-mic") : "\u2318\u21e7Space"}
          </span>
        </div>

        {/* Layer 3: Horizontal drum-roller mode selector */}
        <div className="absolute bottom-0 left-0 right-0 z-[5]">
          <ModeDrum modes={modes} current={dictationMode} onChange={(id) => {
            setDictationMode(id);
            // Switching away from "note" via drum clears notebook selection
            if (id !== "note") setSelectedNotebookId(null);
          }} />
        </div>
      </div>

      {/* ── Notebooks strip (shown only when note mode is selected) ── */}
      {dictationMode === "note" && notebooks.length > 0 && (
        <div className="flex-shrink-0 px-4 pt-2 pb-1.5">
          <div className="flex items-center gap-1.5 overflow-x-auto scrollbar-none">
            {/* Section label */}
            <span className="text-[9px] uppercase tracking-wider text-[rgba(255,255,255,0.2)] font-semibold flex-shrink-0 mr-0.5">
              {t("dict.notes")}
            </span>
            {/* Quick Note pill */}
            <button
              onClick={() => handleSelectNotebook(null)}
              className={[
                "flex-shrink-0 flex items-center gap-1 px-2.5 py-1 rounded-full text-[10px] font-medium transition-all duration-200 border",
                dictationMode === "note" && selectedNotebookId === null
                  ? "bg-[rgba(245,158,11,0.12)] border-[rgba(245,158,11,0.25)] text-[#fbbf24]"
                  : "bg-[rgba(255,255,255,0.03)] border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.35)] hover:border-[rgba(255,255,255,0.12)] hover:text-[rgba(255,255,255,0.55)]",
              ].join(" ")}
            >
              <PinIcon size={11} />
              <span>{t("dict.quick-note")}</span>
            </button>
            {/* Notebook pills (exclude Quick Note — shown above) */}
            {notebooks.filter((nb) => nb.title !== "Quick Note").map((nb) => (
              <button
                key={nb.id}
                onClick={() => handleSelectNotebook(nb.id)}
                className={[
                  "flex-shrink-0 flex items-center gap-1 px-2.5 py-1 rounded-full text-[10px] font-medium transition-all duration-200 border",
                  dictationMode === "note" && selectedNotebookId === nb.id
                    ? "bg-[rgba(245,158,11,0.12)] border-[rgba(245,158,11,0.25)] text-[#fbbf24]"
                    : "bg-[rgba(255,255,255,0.03)] border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.35)] hover:border-[rgba(255,255,255,0.12)] hover:text-[rgba(255,255,255,0.55)]",
                ].join(" ")}
              >
                <NotebookIcon size={11} />
                <span className="max-w-[80px] truncate">{nb.title}</span>
              </button>
            ))}
          </div>
          {/* Active destination label */}
          {dictationMode === "note" && (
            <div className="mt-1 text-[9px] text-[rgba(255,255,255,0.2)] pl-0.5">
              {t("dict.dictating-to")}{" "}
              <span className="text-[rgba(255,255,255,0.4)]">
                {currentNotebook ? currentNotebook.title : t("dict.quick-note")}
              </span>
            </div>
          )}
        </div>
      )}

      <div className="mx-5 border-t border-[rgba(255,255,255,0.04)]" />

      {/* ══ Bottom: Activity ══ */}
      <div className="flex flex-col flex-1 min-h-0 px-5 pt-3 pb-4">
        <div className="flex items-center justify-between mb-2">
          <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.2)] font-medium">{t("dict.activity")}</span>
          {activity.length > 0 && (
            <button onClick={() => setActivity([])} className="text-[9px] text-[rgba(255,255,255,0.12)] hover:text-[rgba(255,255,255,0.3)] transition-colors">{t("dict.clear")}</button>
          )}
        </div>
        <div ref={activityRef} className="flex-1 overflow-y-auto min-h-0">
          {activity.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full gap-1">
              <span className="text-[rgba(255,255,255,0.1)] text-[11px]">{processing ? t("dict.processing") : t("dict.ready")}</span>
              {!processing && <span className="text-[rgba(255,255,255,0.06)] text-[10px]">{t("dict.results-hint")}</span>}
            </div>
          ) : (
            <div className="relative pl-5">
              <div className="absolute left-[5px] top-1 bottom-1 w-[1px] bg-[rgba(255,255,255,0.04)]" />
              {activity.map((entry) => (
                <div key={entry.id} className="relative pb-3 last:pb-0">
                  <div className={["absolute left-[-16px] top-[3px] w-[7px] h-[7px] rounded-full",
                    entry.type === "error" ? "bg-[#ef4444]" : entry.type === "result" ? "bg-[#fbbf24]" : entry.type === "transcript" ? "bg-[rgba(255,255,255,0.25)]" : "bg-[rgba(255,255,255,0.12)]",
                  ].join(" ")} />
                  <div className="flex items-center gap-1.5 mb-0.5">
                    <span className={["text-[12px]",
                      entry.type === "result" ? "text-[#fbbf24]" : entry.type === "error" ? "text-[#ef4444]" : "text-[rgba(255,255,255,0.4)]",
                    ].join(" ")}>{entry.icon}</span>
                    <span className={["text-[11px] font-medium",
                      entry.type === "result" ? "text-[#fbbf24]" : entry.type === "error" ? "text-[#ef4444]" : "text-[rgba(255,255,255,0.4)]",
                    ].join(" ")}>{entry.label}</span>
                    <span className="flex-1" />
                    <div className="flex items-center gap-1.5 flex-shrink-0">
                      {entry.duration !== undefined && <span className="text-[8px] text-[rgba(255,255,255,0.15)] font-mono">{entry.duration.toFixed(1)}s</span>}
                      {entry.latency !== undefined && <span className="text-[8px] text-[rgba(255,255,255,0.15)] font-mono">{entry.latency}ms</span>}
                      {entry.tokens && <span className="text-[8px] text-[rgba(255,255,255,0.15)] font-mono">{entry.tokens}</span>}
                    </div>
                  </div>
                  {entry.model && <div className="mb-1"><span className="text-[8px] text-[rgba(255,255,255,0.12)] bg-[rgba(255,255,255,0.03)] px-1.5 py-0.5 rounded">{entry.model}</span></div>}
                  {entry.content && (
                    <div className={["text-[12px] leading-relaxed rounded-lg px-2.5 py-2 whitespace-pre-wrap",
                      entry.type === "result" ? "bg-[rgba(245,158,11,0.05)] border border-[rgba(245,158,11,0.08)] text-[#fafaf9]"
                        : entry.type === "error" ? "bg-[rgba(239,68,68,0.05)] border border-[rgba(239,68,68,0.08)] text-[rgba(239,68,68,0.7)]"
                        : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.55)]",
                    ].join(" ")}>{entry.content}</div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
