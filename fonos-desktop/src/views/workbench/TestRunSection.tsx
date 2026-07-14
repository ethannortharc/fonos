// TestRunSection — 试运行台架：加载配方或单个组件，mock 输入按 source 形态给
// （mic=真录音，其余=文本框），运行时订阅 bench:event 逐节点点亮，输出默认拦截。
//
// Two invoke-rejection surfaces the backend documents (Task 7 review notes):
//  - A step error arrives as `step_finished` with `error` set and NO
//    terminal `failed` event follows (e.g. mock-text-on-audio rejection) —
//    any step error is treated as terminal here: running/recording reset,
//    the error shown on the node AND in the status line.
//  - Pre-flight errors (a run already in flight, unknown id, mic-acquire
//    failure in bench_run_widget's audio branch) surface as an `invoke()`
//    promise rejection with no bench:event at all — both invokes below are
//    wrapped in try/catch so the bench never gets stuck "running" forever.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listWidgets, listWorkflows, finishCapture, saveWidget } from "../../lib/api";
import { t, useT } from "../../lib/i18n";
import { widgetLabel, workflowLabel } from "../../lib/builtinLabels";
import { usageCount } from "../../lib/triggers";
import BenchGraph, { type BenchNode } from "../../components/BenchGraph";
import MicButton from "../../components/MicButton";
import WaveCanvas from "../../components/WaveCanvas";
import WidgetForm, { widgetToForm } from "../settings/WidgetForm";
import { GROUPS, TYPE_META } from "./typeMeta";
import type { BenchTarget } from "../Workbench";
import type { Container } from "../../lib/storage-api";
import type { AppConfig, WidgetDef, WorkflowRow } from "../../types";

type BenchEvent =
  | { type: "step_started"; step_id: string; index: number; role: string }
  | { type: "step_finished"; step_id: string; index: number; role: string; preview: string; ms: number; error: string | null; intercepted: boolean }
  | { type: "processing" } | { type: "no_speech" }
  | { type: "done"; raw: string; final: string }
  | { type: "failed"; message: string };

const msg = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export default function TestRunSection({
  config, containers, onContainerCreated, target, onTargetChange,
}: {
  config: AppConfig;
  containers: Container[];
  /** Reload the owner's (Workbench's) containers list after a notebook widget
   *  mints a new container at save time — forwarded to WidgetForm to keep
   *  name-is-identity honest across this stale-once-loaded list. */
  onContainerCreated?: () => void;
  target: BenchTarget;
  onTargetChange: (t: BenchTarget) => void;
}) {
  useT();
  const [rows, setRows] = useState<WorkflowRow[]>([]);
  const [widgets, setWidgets] = useState<WidgetDef[]>([]);
  const [nodes, setNodes] = useState<BenchNode[]>([]);
  const [running, setRunning] = useState(false);
  const [recording, setRecording] = useState(false);
  const [deliver, setDeliver] = useState(false);
  const [status, setStatus] = useState<string>("");
  const [editNode, setEditNode] = useState<string | null>(null);
  const [text, setText] = useState("");
  const nodesRef = useRef<BenchNode[]>([]);
  nodesRef.current = nodes;
  const rowsRef = useRef<WorkflowRow[]>([]);
  rowsRef.current = rows;
  const widgetsRef = useRef<WidgetDef[]>([]);
  widgetsRef.current = widgets;
  const runningRef = useRef(false);
  runningRef.current = running;
  const recordingRef = useRef(false);
  recordingRef.current = recording;
  // Unmount-only release: a live mic recording (segment/nav switch away
  // mid-record) must not survive the component going away. MicSource::acquire
  // blocks on the backend until finish_capture arrives, holding the
  // InFlightGuard the whole time — without this, switching away with the mic
  // hot would leave it recording and the guard held forever.
  useEffect(() => () => { if (recordingRef.current) { void finishCapture(); } }, []);

  // Stable string key for the loaded target — a *value*, so the fallback
  // `benchTarget ?? { kind: "recipe", ... }` object Workbench.tsx re-creates
  // on every one of its own renders doesn't look like a "target switch"
  // here (its object identity changes; this string doesn't).
  const targetKey = target ? `${target.kind}:${target.id}` : "";

  useEffect(() => {
    void (async () => {
      setRows(await listWorkflows());
      setWidgets(await listWidgets());
    })();
  }, []);

  const wById = useCallback((id: string) => widgets.find((w) => w.id === id), [widgets]);
  const nodeOf = (w: WidgetDef | undefined, id: string): BenchNode => ({
    id,
    role: w?.role ?? "processor",
    typeTag: w?.type_tag ?? "",
    label: w ? widgetLabel(w) : id,
    state: "idle",
  });

  // target → 节点链 + 输入形态
  const recipe = target?.kind === "recipe" ? rows.find((r) => r.id === target.id) : undefined;
  const widget = target?.kind === "widget" ? wById(target.id) : undefined;
  const audioInput = useMemo(() => {
    if (recipe) return recipe.source_type_tag === "microphone";
    if (widget) return widget.type_tag === "microphone" || widget.type_tag === "stt";
    return false;
  }, [recipe, widget]);

  // Effect A — full reset + node rebuild. Fires ONLY on a genuine target
  // switch (targetKey, not `target` itself — see note above). Reads
  // CURRENT rows/widgets via refs rather than the `recipe`/`widget` consts
  // above, so this can't be dragged into re-running (and wiping a live
  // run's `running`/`recording`/`status`) by a mid-run data refetch, e.g.
  // the node editor's own `setWidgets(await listWidgets())` after a save
  // (Task 11 review finding).
  useEffect(() => {
    const curRows = rowsRef.current;
    const curWidgets = widgetsRef.current;
    const findWidget = (id: string) => curWidgets.find((w) => w.id === id);
    const curRecipe = target?.kind === "recipe" ? curRows.find((r) => r.id === target.id) : undefined;
    const curWidget = target?.kind === "widget" ? findWidget(target.id) : undefined;
    if (curRecipe) {
      const chain = [curRecipe.source, ...(curRecipe.processors ?? []), ...curRecipe.outputs];
      setNodes(chain.map((id) => nodeOf(findWidget(id), id)));
    } else if (curWidget) {
      setNodes([nodeOf(curWidget, curWidget.id)]);
    } else {
      setNodes([]);
    }
    setStatus(t(target?.kind === "widget" ? "wb.bench.status-widget" : "wb.bench.status-idle"));
    setEditNode(null);
    setRunning(false);
    setRecording(false);
    // `target`/rows/widgets intentionally omitted: keyed on targetKey alone;
    // current rows/widgets are read via the refs above instead.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetKey]);

  // Effect B — data-refresh rebuild. Rows/widgets refetch (e.g. after a
  // widget save) should refresh the idle node list's labels/roles, but must
  // NOT touch running/recording/status, and must NOT do anything at all
  // while a run is in flight — checked via runningRef (not a dep) so this
  // effect doesn't need `running` in its array. Losing a finished run's
  // payloads after a post-run data refresh is acceptable.
  useEffect(() => {
    if (runningRef.current) return;
    const curRecipe = target?.kind === "recipe" ? rows.find((r) => r.id === target.id) : undefined;
    const curWidget = target?.kind === "widget" ? wById(target.id) : undefined;
    if (curRecipe) {
      const chain = [curRecipe.source, ...(curRecipe.processors ?? []), ...curRecipe.outputs];
      setNodes(chain.map((id) => nodeOf(wById(id), id)));
    } else if (curWidget) {
      setNodes([nodeOf(curWidget, curWidget.id)]);
    } else {
      setNodes([]);
    }
    // `target` intentionally omitted: this effect only reacts to data
    // refetches — Effect A above owns target switches.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, widgets]);

  // bench:event 订阅（StrictMode 双挂载安全：disposed 守卫丢弃迟到的 listen()）
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    const release = async (un: () => void) => {
      try {
        await Promise.resolve(un());
      } catch {
        // The listener may already be gone during a StrictMode remount.
      }
    };
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const un = await listen<BenchEvent>("bench:event", (e) => {
        const ev = e.payload;
        const patch = (pred: (n: BenchNode, i: number) => boolean, up: (n: BenchNode) => BenchNode) =>
          setNodes((ns) => ns.map((n, i) => (pred(n, i) ? up(n) : n)));
        if (ev.type === "step_started") {
          if (ev.role === "source") setRecording(audioInput);
          patch((n) => n.id === ev.step_id && n.state === "idle", (n) => ({ ...n, state: "active" }));
        } else if (ev.type === "step_finished") {
          if (ev.role === "source") setRecording(false);
          patch(
            (n) => n.id === ev.step_id && (n.state === "active" || n.state === "idle"),
            (n) => ({
              ...n,
              state: ev.error ? "error" : "done",
              payload: { preview: ev.preview, ms: ev.ms, error: ev.error ?? undefined, intercepted: ev.intercepted },
            }),
          );
          // A step error is terminal: no `failed` event follows it. Stop the
          // run right here so the bench doesn't get stuck "running" forever.
          if (ev.error) {
            setRunning(false);
            setRecording(false);
            setStatus(ev.error);
          }
        } else if (ev.type === "done") {
          setRunning(false);
          const total = nodesRef.current.reduce((a, n) => a + (n.payload?.ms ?? 0), 0);
          setStatus(t("wb.bench.status-done").replace("{0}", String(total)));
        } else if (ev.type === "failed") {
          setRunning(false);
          setStatus(ev.message);
        } else if (ev.type === "no_speech") {
          setRunning(false);
          setRecording(false);
          setStatus(t("wb.bench.status-nospeech"));
        }
      });
      if (disposed) {
        await release(un);
      } else {
        unlisten = un;
      }
    })();
    return () => {
      disposed = true;
      if (unlisten) void release(unlisten);
    };
  }, [audioInput]);

  const run = async () => {
    if (!target || running) return;
    setRunning(true);
    setNodes((ns) => ns.map((n) => ({ ...n, state: "idle", payload: undefined })));
    setStatus(t("wb.bench.status-running"));
    try {
      if (target.kind === "recipe") {
        await invoke("bench_run_workflow", {
          workflow_id: target.id,
          mock_text: audioInput ? null : text || null,
          deliver,
        });
      } else {
        await invoke("bench_run_widget", {
          widget_id: target.id,
          input_text: audioInput ? null : text,
          deliver,
        });
      }
    } catch (e) {
      // Pre-flight rejection (busy in-flight, unknown id, mic-acquire failure)
      // — no bench:event will ever arrive for this attempt.
      setRunning(false);
      setRecording(false);
      setStatus(msg(e));
    }
  };
  const micClick = async () => {
    if (!running) {
      await run(); // 开跑（MicSource 立即开录）
    } else {
      try {
        await finishCapture(); // 停录，链继续
      } catch (e) {
        setStatus(msg(e));
      }
    }
  };

  return (
    <div>
      <div className="mb-3.5 flex items-center gap-2.5">
        <label className="text-[11px] text-[rgba(255,255,255,0.43)]">{t("wb.bench.loaded")}</label>
        <select
          value={targetKey}
          disabled={running}
          onChange={(e) => {
            const [kind, ...rest] = e.target.value.split(":");
            onTargetChange({ kind: kind as "recipe" | "widget", id: rest.join(":") });
          }}
          className="rounded-[8px] border border-[rgba(255,255,255,0.075)] bg-[rgba(255,255,255,0.04)] px-2.5 py-[5px] text-[11.5px] disabled:opacity-50"
        >
          <optgroup label={t("wb.seg.recipes")}>
            {rows.map((r) => <option key={r.id} value={`recipe:${r.id}`}>{workflowLabel(r)}</option>)}
          </optgroup>
          {GROUPS.flatMap(({ tags }) => tags)
            .filter((tag) => tag !== "selection")
            .map((tag) => {
              const matches = widgets.filter((w) => w.type_tag === tag);
              if (matches.length === 0) return null;
              return (
                <optgroup key={tag} label={`${t("wb.seg.widgets")} · ${t(TYPE_META[tag].name)}`}>
                  {matches.map((w) => (
                    <option key={w.id} value={`widget:${w.id}`}>{widgetLabel(w)}</option>
                  ))}
                </optgroup>
              );
            })}
        </select>
        <span className="flex-1" />
        <label className="flex items-center gap-[7px] text-[11px] text-[rgba(255,255,255,0.43)]" title={t("wb.bench.deliver-tip")}>
          {t("wb.bench.deliver")}
          <button
            role="switch"
            aria-checked={deliver}
            disabled={running}
            onClick={() => setDeliver((v) => !v)}
            className={[
              "relative h-[17px] w-[30px] rounded-[9px] border transition-colors disabled:opacity-50",
              deliver ? "border-[rgba(244,80,58,0.4)] bg-[rgba(244,80,58,0.35)]" : "border-[rgba(255,255,255,0.075)] bg-[rgba(255,255,255,0.05)]",
            ].join(" ")}
          >
            <span className={[
              "absolute top-[2px] h-[11px] w-[11px] rounded-full transition-transform",
              deliver ? "left-[2px] translate-x-[13px] bg-[#ffb3a6]" : "left-[2px] bg-[rgba(255,255,255,0.45)]",
            ].join(" ")} />
          </button>
        </label>
      </div>

      <div className="fonos-surface rounded-[12px] px-[22px] py-5">
        {/* MicButton's own `isolate` + negative z-index only contains the aura
            WITHIN MicButton's subtree (aura vs. its sibling <button>) — it does
            NOT stop the MicButton root div itself from escaping. That root div
            is `position: relative` (needed as the aura's containing block)
            with z-index: auto, so it paints in the "z-index: 0" bucket, which
            ALWAYS paints after in-flow non-positioned content of its nearest
            ancestor stacking context, regardless of DOM order — without a
            container of its own, this row (and MicButton's aura/blur/breathing
            overflow along with it) would paint over the graph/editor/run-row
            below despite coming first in the markup.
            Empirically verified (real-browser Playwright repro, see P2 Task 1
            fix-round-1 report): giving this row `isolate` alone does NOT fix
            it — an isolated-but-unpositioned-z-index row is itself just
            another auto-z-index escapee one level up, confirmed identical to
            the unfixed baseline via both elementFromPoint and rendered-pixel
            sampling. What DOES work (kept from Task 11): `relative z-0` here
            + `relative z-[1]` on every sibling below (BenchGraph, the node
            editor panel, the run row) pulls all of them out of the ambiguous
            "positioned vs. non-positioned" comparison entirely and sorts them
            by explicit z-index instead, so 0 always paints under 1 — this
            pairing is the load-bearing containment, not `isolate`. Aura layers
            are already pointer-events-none regardless. */}
        <div className="relative z-0 mb-5 flex items-center gap-[18px]">
          {audioInput ? (
            <>
              <div className="relative h-[88px] w-[88px] flex-shrink-0 flex items-center justify-center">
                <MicButton recording={recording} onClick={micClick} />
              </div>
              <div className="min-w-0 flex-1">
                <div className="mb-[7px] text-[11px] text-[rgba(255,255,255,0.43)]">
                  {recording ? t("wb.bench.mic-live") : t("wb.bench.mic-idle")}
                </div>
                <div className="relative h-[30px]"><WaveCanvas active={recording} /></div>
              </div>
            </>
          ) : (
            <div className="flex-1">
              <div className="mb-1.5 text-[11px] text-[rgba(255,255,255,0.43)]">{t("wb.bench.text-hint")}</div>
              <textarea
                value={text}
                onChange={(e) => setText(e.target.value)}
                className="min-h-[58px] w-full resize-y rounded-[9px] border border-[rgba(255,255,255,0.075)] bg-[rgba(255,255,255,0.035)] px-[11px] py-[9px] text-[12px] leading-[1.55] outline-none focus:border-[rgba(242,184,75,0.5)]"
              />
            </div>
          )}
        </div>

        {/* fonos-surface-opaque-fill: the z-0/z-[1] pairing above already paints
            the mic's live-recording aura BEHIND this wrapper (correct paint
            order) — but the first node's capsule is only 8% opaque
            (BenchGraph.tsx), so the aura was bleeding THROUGH it rather than
            being hidden by it. This gives the wrapper an opaque backdrop
            (matching .fonos-surface's own composited tone, so the seam is
            invisible) that fully occludes the aura here while leaving it free
            to bloom in the input-row band above/around the mic. Empirically
            verified (Playwright pixel sampling, Test Run Live-Aura Bleed
            Fix) — the aura's visual reach fades out well above the node
            editor/run-row below, and both are unreachable while a run (and
            its live aura) is in flight anyway (onNodeClick is disabled while
            running), so no further occlusion is needed there. */}
        <div className="relative z-[1] fonos-surface-opaque-fill">
          <BenchGraph
            nodes={nodes}
            onNodeClick={running ? undefined : (id) => setEditNode((cur) => (cur === id ? null : id))}
          />
        </div>

        {editNode && wById(editNode) && (
          <div className="relative z-[1] mt-3.5 rounded-[12px] border border-[rgba(255,255,255,0.075)] bg-[rgba(255,255,255,0.02)] p-[15px]">
            {usageCount(editNode, rows, widgets) > 0 && (
              <div className="mb-1.5 text-[10px] text-[rgba(242,184,75,0.8)]">{t("wb.widgets.share-warn").replace("{0}", String(usageCount(editNode, rows, widgets)))}</div>
            )}
            <WidgetForm
              value={widgetToForm(wById(editNode)!)}
              config={config}
              containers={containers}
              widgets={widgets}
              onSave={async (w) => {
                await saveWidget(w);
                setWidgets(await listWidgets());
                setEditNode(null);
              }}
              onCancel={() => setEditNode(null)}
              onContainerCreated={onContainerCreated}
            />
          </div>
        )}

        <div className="relative z-[1] mt-4 flex items-center gap-3">
          {!audioInput && (
            <button
              onClick={run}
              disabled={running || !target}
              className="rounded-[8px] px-3 py-[6px] text-[11px] font-semibold text-[#45300e] disabled:opacity-50"
              style={{ background: "linear-gradient(148deg,#ffdd85 0%,#f5b043 46%,#d67e1c 78%)" }}
            >
              {t("wb.bench.run")}
            </button>
          )}
          <span className="text-[11px] text-[rgba(255,255,255,0.43)]">{status}</span>
        </div>
      </div>
    </div>
  );
}
