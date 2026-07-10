// Dictation view — jumping blocks + mic + drum-roller mode slider + activity.

import React, { useState, useRef, useEffect, useCallback } from "react";
import { TranscriptIcon, HourglassIcon, SparklesIcon, AlertIcon, PinIcon, NotebookIcon, ModeIcon, FonosMark } from "../components/Icons";
import {
  hasMicrophone,
  runWorkflowById,
  finishCapture,
  listWorkflows,
  getConfig,
} from "../lib/api";
import { workflowLabel } from "../lib/builtinLabels";
import { listContainers } from "../lib/storage-api";
import { t, useT } from "../lib/i18n";
import type { Container } from "../lib/storage-api";
import type { WorkflowRow } from "../types";

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
    const barCount = 36;
    const gap = 3;
    // Don't span full width — leave ~15% margin on each side
    const margin = W * 0.12;
    const usableW = W - margin * 2;
    const barW = (usableW - (barCount - 1) * gap) / barCount;
    const maxH = H * 0.34;
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
        const opacity = (0.07 + opacityEnvelope * 0.34) * fade;
        ctx.fillStyle = `rgba(240, 173, 50, ${opacity})`;
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
  modes: WorkflowRow[]; current: string; onChange: (id: string) => void;
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
    return <div className="text-[11px] text-[var(--text-faint)] text-center py-2">{t("dict.no-modes")}</div>;
  }

  if (modes.length === 1) {
    const only = modes[0];
    return (
      <div className="h-10 flex items-center justify-center">
        <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-[9px] bg-[rgba(240,173,50,0.1)] border border-[rgba(240,173,50,0.18)] text-[var(--accent)]">
          <ModeIcon icon={only.icon ?? ""} size={13} />
          <span className="text-[11px] font-semibold">{workflowLabel(only)}</span>
        </div>
      </div>
    );
  }

  if (modes.length === 2) {
    return (
      <div className="h-10 flex items-center justify-center">
        <div className="flex items-center rounded-[10px] border border-[rgba(255,255,255,0.07)] bg-[rgba(255,255,255,0.025)] p-0.5">
          {modes.map((mode) => {
            const activeMode = mode.id === current;
            return (
              <button
                key={mode.id}
                onClick={() => onChange(mode.id)}
                className={[
                  "flex items-center gap-1.5 px-3 py-1 rounded-[8px] text-[10.5px] font-medium transition-colors",
                  activeMode
                    ? "bg-[rgba(240,173,50,0.12)] text-[var(--accent)]"
                    : "text-[var(--text-muted)] hover:text-[var(--text-secondary)]",
                ].join(" ")}
              >
                <ModeIcon icon={mode.icon ?? ""} size={12} />
                {workflowLabel(mode)}
              </button>
            );
          })}
        </div>
      </div>
    );
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
      <div className="absolute left-0 top-0 bottom-0 w-16 bg-gradient-to-r from-[var(--bg)] to-transparent z-10 pointer-events-none" />
      <div className="absolute right-0 top-0 bottom-0 w-16 bg-gradient-to-l from-[var(--bg)] to-transparent z-10 pointer-events-none" />

      {/* 3-column layout: left items | center item | right items — center always at 50% */}
      {(() => {
        const renderSlot = (offset: number) => {
          const m = modeAt(idx + offset);
          const isCenter = offset === 0;
          const dist = Math.abs(offset);
          const opacity = isCenter ? 1 : dist === 1 ? 0.62 : 0.3;
          // Subtle depth cue: flanking items sit slightly back so the selected
          // (center) item reads unambiguously. Opacity above still does the bulk.
          const itemScale = isCenter ? 1 : 0.92;
          const textColor = isCenter ? "var(--accent)" : "rgba(255,255,255,0.62)";
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
              style={{ opacity, transform: `scale(${itemScale})` }}
            >
              <span style={{ color: textColor }}><ModeIcon icon={m.icon ?? ""} size={isCenter ? 13 : 12} /></span>
              <span style={{
                color: textColor,
                fontSize: isCenter ? 11 : 10.5,
                fontWeight: isCenter ? 600 : 500,
                whiteSpace: "nowrap",
              }}>{workflowLabel(m)}</span>
            </button>
          );
        };

        return (
          <div className="flex items-center w-full transition-transform duration-200">
            <div className="flex-1 flex justify-end overflow-hidden">
              {[-2, -1].map(renderSlot)}
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
    <svg className="relative z-10" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <rect x="9" y="1" width="6" height="12" rx="3" />
      <path d="M5 10a7 7 0 0 0 14 0" />
      <line x1="12" y1="17" x2="12" y2="21" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg className="relative z-10" width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
      <rect x="5" y="5" width="14" height="14" rx="2" />
    </svg>
  );
}

function VoiceAura({ active }: { active: boolean }) {
  return (
    <div
      className={["fonos-mic-ambient absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-[59%] w-[142px] h-[112px] pointer-events-none", active ? "fonos-mic-ambient-live" : ""].join(" ")}
      aria-hidden="true"
    >
      <span className="fonos-mic-ambient-bloom absolute inset-[5px] rounded-full" />
      <span className="fonos-mic-ambient-core absolute inset-[24px] rounded-full" />
      <span className="fonos-mic-ambient-floor absolute left-[28px] right-[28px] bottom-[9px] h-[18px] rounded-full" />
    </div>
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
  // Note-target selector state: `dictationMode === "note"` reveals the notebook
  // strip below (legacy note-target UI, kept intact). The mic button and the
  // drum no longer read this — both run the workflow engine now.
  const [dictationMode, setDictationMode] = useState<string>("raw");
  // Voice-workflow picker (drum): microphone-source workflows. This view's
  // selection is VIEW-LOCAL (test-page scope): initialized from
  // config.active_voice_workflow as the default but NOT saved back — only this
  // view's in-view mic button follows it (runWorkflowById below). The float
  // pill's roller is the sole writer of config.active_voice_workflow, and the
  // global dictation hotkey follows that.
  const [voiceWorkflows, setVoiceWorkflows] = useState<WorkflowRow[]>([]);
  const [activeWorkflow, setActiveWorkflow] = useState<string>("wf.dictation");
  const [notebooks, setNotebooks] = useState<Container[]>([]);
  // null = Quick Note (no specific notebook), number = notebook id
  const [selectedNotebookId, setSelectedNotebookId] = useState<number | null>(null);
  const [isRecording, setIsRecording] = useState(false);
  const [hasMic, setHasMic] = useState<boolean | null>(null);
  const [processing, setProcessing] = useState(false);
  const [recordDuration, setRecordDuration] = useState(0);
  const [activity, setActivity] = useState<ActivityEntry[]>([]);
  const durationRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const activityRef = useRef<HTMLDivElement>(null);
  // Live handle to the voice workflows, so the once-registered workflow:done
  // listener can label a result by the workflow that actually produced it —
  // resolved from the run's own id, not the drum's current selection — without
  // re-subscribing. Kept in sync by the effect below.
  const voiceWorkflowsRef = useRef<WorkflowRow[]>([]);

  useEffect(() => {
    if (activityRef.current) activityRef.current.scrollTop = activityRef.current.scrollHeight;
  }, [activity]);

  useEffect(() => {
    listWorkflows()
      .then((rows) => setVoiceWorkflows(rows.filter((w) => w.source_type_tag === "microphone")))
      .catch(() => {});
    hasMicrophone().then(setHasMic).catch(() => setHasMic(false));
    getConfig().then((cfg) => {
      if (cfg.dictation_mode) setDictationMode(cfg.dictation_mode);
      setActiveWorkflow(cfg.active_voice_workflow || "wf.dictation");
    }).catch(() => {});
    listContainers()
      .then((all) => setNotebooks(all.filter((c) => c.container_type === "notebook")))
      .catch(() => {});
  }, []);

  // Mirror the voice workflows into a ref so the once-registered workflow:done
  // listener resolves the run's workflow id against the current set.
  useEffect(() => {
    voiceWorkflowsRef.current = voiceWorkflows;
  }, [voiceWorkflows]);

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

  // Subscribe to the engine's terminal lifecycle so this view's feed reflects
  // EVERY workflow run — the in-view mic button and hotkey-triggered runs alike
  // (both go through the same engine path now). Per run the engine emits:
  // float:start (mic capture began) → float:processing → float:stop (state
  // reset; "" = no speech) and, for engine runs, workflow:done (the raw
  // transcript + final result + workflow id that drives the feed entries) — or
  // float:error (surfaced error JSON).
  useEffect(() => {
    // Listeners register asynchronously (dynamic import). If the effect is torn
    // down before registration finishes — as StrictMode's dev double-mount does
    // — a plain cleanup array is still empty, so the stale listeners leak and
    // every event fires twice (double feed entries). `disposed` closes that
    // race: unlisten fns that resolve after teardown are dropped immediately.
    let disposed = false;
    const cleanup: (() => void)[] = [];
    const track = (unlisten: () => void) => {
      if (disposed) unlisten();
      else cleanup.push(unlisten);
    };
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");

        // Capture began (a mic source started recording).
        track(await listen<string>("float:start", () => {
          setIsRecording(true);
          setProcessing(false);
        }));

        // Capture ended, processing (STT/LLM) started. Flip out of the recording
        // state and drop a placeholder the terminal float:stop replaces.
        track(await listen("float:processing", () => {
          setIsRecording(false);
          setProcessing(true);
          setActivity((prev) => {
            if (prev.some((e) => e.type === "processing")) return prev;
            return [...prev, {
              id: `${Date.now()}-p-${Math.random()}`,
              timestamp: new Date(),
              type: "processing" as const,
              icon: <HourglassIcon size={12} />,
              label: t("dict.processing"),
            }];
          });
        }));

        // Terminal delivery: state reset only. The feed entries are owned by the
        // workflow:done listener (which carries raw + final + the run's
        // workflow), so a non-empty payload pushes nothing here — that avoids
        // double entries. An empty payload is a no-speech run (which emits no
        // workflow:done): drop the processing placeholder and leave a note.
        track(await listen<string>("float:stop", (event) => {
          setIsRecording(false);
          setProcessing(false);
          const text = typeof event.payload === "string" ? event.payload : String(event.payload ?? "");
          if (text) return;
          setActivity((prev) => {
            const filtered = prev.filter((e) => e.type !== "processing");
            return [...filtered, {
              id: `${Date.now()}-ns-${Math.random()}`,
              timestamp: new Date(),
              type: "transcript" as const,
              icon: <TranscriptIcon size={12} />,
              label: t("dict.no-speech"),
            }];
          });
        }));

        // Engine run completed: {raw, final, workflow_id}. Fires for EVERY
        // engine run, not just microphone-source ones — selection-source
        // workflows (e.g. wf.explain) go through the same engine path and
        // land here too. This feed is scoped to voice runs only, so a
        // workflow_id that isn't one of the current voiceWorkflows (mirrored
        // via voiceWorkflowsRef, kept in sync by the effect above) is silently
        // ignored: no entry is added. float:processing fires ahead of every
        // engine run regardless of source, so an ignored run can still have
        // left a "processing" placeholder in the feed — drop it here too so
        // it doesn't strand.
        track(await listen<{ raw?: string; final?: string; workflow_id?: string }>(
          "workflow:done",
          (event) => {
            const p = event.payload ?? {};
            const wfId = typeof p.workflow_id === "string" ? p.workflow_id : "";
            const wf = voiceWorkflowsRef.current.find((w) => w.id === wfId);
            if (!wf) {
              setActivity((prev) => prev.filter((e) => e.type !== "processing"));
              return;
            }
            const raw = typeof p.raw === "string" ? p.raw : "";
            const final = typeof p.final === "string" ? p.final : "";
            const label = workflowLabel(wf);
            const icon = wf.icon ?? "";
            const resultIcon = icon ? <ModeIcon icon={icon} size={12} /> : <SparklesIcon size={12} />;
            setActivity((prev) => {
              const filtered = prev.filter((e) => e.type !== "processing");
              const now = Date.now();
              if (raw && final && raw !== final) {
                return [...filtered,
                  {
                    id: `${now}-t-${Math.random()}`,
                    timestamp: new Date(),
                    type: "transcript" as const,
                    icon: <TranscriptIcon size={12} />,
                    label: t("dict.transcript"),
                    content: raw,
                  },
                  {
                    id: `${now}-r-${Math.random()}`,
                    timestamp: new Date(),
                    type: "result" as const,
                    icon: resultIcon,
                    label,
                    content: final,
                  },
                ];
              }
              const text = final || raw;
              if (!text) return filtered;
              return [...filtered, {
                id: `${now}-r-${Math.random()}`,
                timestamp: new Date(),
                type: "result" as const,
                icon: resultIcon,
                label,
                content: text,
              }];
            });
          }
        ));

        track(await listen<string>("float:error", (event) => {
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
          // A failed run is terminal too — leave the recording state and drop
          // any pending processing placeholder before recording the error.
          setIsRecording(false);
          setProcessing(false);
          setActivity((prev) => {
            const base = prev.filter((e) => e.type !== "processing");
            // Best-effort dedup: skip if the most recent entry is an identical
            // error added within the last 1.5s (e.g. the in-view catch below
            // already recorded it). Errors with differing text still appear.
            const last = base[base.length - 1];
            if (last && last.type === "error" && last.content === message &&
                Date.now() - last.timestamp.getTime() < 1500) {
              return base;
            }
            return [...base, {
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
    return () => { disposed = true; cleanup.forEach((fn) => fn()); };
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

  // Mic button: run the selected voice workflow through the engine (the same
  // path hotkeys use) and finish its capture on the second click. The feed and
  // the recording/processing state are driven entirely by the engine's
  // float:* events (listeners above) — the click only kicks off / ends a run.
  const handleStartStop = useCallback(() => {
    if (isRecording) {
      // Second click → end the in-flight capture. float:processing / float:stop
      // then update the feed and clear the recording state.
      finishCapture().catch((e: unknown) => {
        addActivity({ type: "error", icon: <AlertIcon size={12} />, label: t("dict.error"), content: e instanceof Error ? e.message : String(e) });
      });
    } else {
      // Fresh feed for a button-initiated session. Optimistically flip into the
      // recording state; float:start confirms it (events are the source of
      // truth). On a failed invoke, reset so the button recovers.
      setActivity([]);
      setIsRecording(true);
      runWorkflowById(activeWorkflow || "wf.dictation").catch((e: unknown) => {
        setIsRecording(false);
        addActivity({ type: "error", icon: <AlertIcon size={12} />, label: t("dict.error"), content: e instanceof Error ? e.message : String(e) });
      });
    }
  }, [isRecording, activeWorkflow, addActivity]);

  // Derive the current notebook for display in the activity label
  const currentNotebook = notebooks.find((n) => n.id === selectedNotebookId) ?? null;
  const currentWorkflow = voiceWorkflows.find((w) => w.id === activeWorkflow) ?? voiceWorkflows[0] ?? null;
  const currentWorkflowName = currentWorkflow ? workflowLabel(currentWorkflow) : t("nav.dictation");

  return (
    <div className="flex flex-col h-full bg-[var(--bg)]">
      {/* ══ Top: Recording panel ══ */}
      <div className="relative flex flex-col items-center justify-center flex-shrink-0 overflow-hidden" style={{ height: 218 }}>
        <div className="absolute inset-x-[18%] top-[-90px] h-[210px] rounded-full bg-[rgba(240,173,50,0.06)] blur-[54px] pointer-events-none" />
        <div className="absolute inset-x-5 top-3 flex items-center justify-between z-[6]">
          <span className="fonos-page-title">{t("nav.dictation")}</span>
          <span className="flex items-center gap-1.5 text-[10px] font-medium text-[var(--text-muted)]">
            <span className={[
              "w-1.5 h-1.5 rounded-full",
              isRecording ? "bg-[var(--danger)] animate-pulse" : processing ? "bg-[var(--accent)] animate-pulse" : hasMic === false ? "bg-[var(--danger)]" : "bg-[rgba(134,239,172,0.75)]",
            ].join(" ")} />
            {isRecording ? `${recordDuration.toFixed(1)}s` : processing ? t("dict.processing") : hasMic === false ? t("dict.no-mic") : t("dict.ready")}
          </span>
        </div>
        {/* Layer 1: Jumping blocks — hidden when idle, fades in when recording */}
        <JumpingBlocks active={isRecording} />

        {/* Center blur — clears space for mic */}
        <div className="absolute z-[2] rounded-full pointer-events-none"
          style={{ width: 116, height: 116, left: "50%", top: "43%", transform: "translate(-50%, -50%)",
            background: "radial-gradient(circle, #181714 28%, rgba(24,23,20,0.92) 50%, transparent 72%)" }} />

        {/* Layer 2: Mic button */}
        <div className="relative z-[5] flex flex-col items-center mt-1">
          <VoiceAura active={isRecording} />
          <button
            onClick={handleStartStop}
            disabled={processing || hasMic === false}
            className={[
              "fonos-dictation-button w-16 h-16 rounded-full flex items-center justify-center transition-all duration-300 active:scale-[0.97]",
              isRecording
                ? "fonos-dictation-button-live"
                : "fonos-dictation-button-idle",
              processing ? "opacity-50 cursor-not-allowed" : "",
            ].join(" ")}
          >
            {isRecording ? <StopIcon /> : processing ? (
              <span className="text-white text-sm font-bold">&middot;&middot;&middot;</span>
            ) : <MicIcon />}
          </button>
          <span className="text-[10px] font-mono text-[var(--text-muted)] mt-2 tabular-nums">
            {isRecording ? `${recordDuration.toFixed(1)}s` : hasMic === false ? t("dict.no-mic") : "\u2318\u21e7Space"}
          </span>
        </div>

        {/* Layer 3: Horizontal drum-roller mode selector */}
        <div className="absolute bottom-0 left-5 right-5 z-[5]">
          <ModeDrum modes={voiceWorkflows} current={activeWorkflow} onChange={(id) => {
            // View-local selection only (test-page scope): drives this view's mic
            // button, NOT persisted. The float pill's roller is the sole writer of
            // config.active_voice_workflow; the global hotkey follows that.
            setActiveWorkflow(id);
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

      <div className="mx-5 border-t border-[rgba(255,255,255,0.07)]" />

      {/* ══ Bottom: Activity ══ */}
      <div className="flex flex-col flex-1 min-h-0 px-5 pt-3 pb-4">
        <div className="flex items-center justify-between mb-2">
          <span className="fonos-eyebrow">{t("dict.activity")}</span>
          {activity.length > 0 && (
            <button onClick={() => setActivity([])} className="text-[9px] text-[rgba(255,255,255,0.12)] hover:text-[rgba(255,255,255,0.3)] transition-colors">{t("dict.clear")}</button>
          )}
        </div>
        <div ref={activityRef} className="flex-1 overflow-y-auto min-h-0">
          {activity.length === 0 ? (
            <div className="flex items-center justify-center h-full pb-5">
              <div className="fonos-surface w-full max-w-[430px] rounded-[16px] px-4 py-3.5 flex items-center gap-3.5">
                <div className="w-9 h-9 rounded-[11px] bg-[rgba(240,173,50,0.1)] border border-[rgba(240,173,50,0.14)] text-[var(--accent)] flex items-center justify-center flex-shrink-0">
                  <FonosMark size={18} />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="text-[12px] font-semibold text-[var(--text-primary)]">
                    {processing ? t("dict.processing") : t("dict.ready")}
                  </div>
                  {!processing && <div className="text-[11px] text-[var(--text-muted)] mt-0.5 truncate">{t("dict.results-hint")}</div>}
                </div>
                <div className="flex flex-col items-end gap-1 flex-shrink-0">
                  <span className="text-[9px] font-mono text-[var(--text-secondary)] bg-[rgba(255,255,255,0.045)] border border-[rgba(255,255,255,0.07)] rounded-md px-1.5 py-0.5">⌘⇧Space</span>
                  <span className="max-w-[118px] truncate text-[9px] text-[var(--text-faint)]">{currentWorkflowName}</span>
                </div>
              </div>
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
