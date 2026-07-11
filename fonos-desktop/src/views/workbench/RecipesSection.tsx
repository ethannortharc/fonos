// RecipesSection.tsx — the Workbench's 配方 (Recipes) segment (Task 10).
// Moved wholesale from settings/FlowsTab.tsx (Flows UI redesign, Task 4),
// which superseded WorkflowsTab as a "flow-first" surface: the flow
// list/editor. (The Building Blocks widget-library view that used to live
// behind a segmented control here moved to the Workbench's Widgets section,
// Task 9.)
//
// A recipe renders as a card. Collapsed, its body is a read-only PipelineView
// (source → processors → outputs) — a glanceable picture of what the recipe
// does. Expanded, the same pipeline becomes interactive and IS the editor:
//
//   • Click any node  → its widget's WidgetForm opens in place below the
//     pipeline. Editing a node = editing the widget it references, without
//     leaving the recipe. Save persists the widget (save_widget) and reloads.
//   • Swap a node      → a role-scoped picker in the node panel swaps which
//     widget the slot references (save_workflow), or opens a fresh WidgetForm
//     ("＋ New…") whose saved id is written straight into the recipe.
//   • "+" between nodes → insert a processor at that position (picker → New).
//   • "+ output"        → add another output (fan-out; ≥1 enforced).
//   • Reorder / remove processors and remove extra outputs from the node panel.
//
// Every card also carries a trigger row (TriggerChips, Task 10) — hotkey and
// pill-roller chips, replacing the old single `hotkey` field — and a "test
// run" button that jumps to the Workbench's Test Run bench (`onBench`).
//
// Structural workflow edits (name, source/processor/output membership and
// order, triggers) auto-save immediately via save_workflow — the backend
// re-validates the chain and any Err surfaces in red. Widget prop edits save
// through WidgetForm's own Save button. The backend is the final validator
// throughout; frontend only pre-hints the microphone-needs-stt rule.

import { useState, useEffect, useRef } from "react";
import { t, useT } from "../../lib/i18n";
import type { AppConfig, WidgetDef, WidgetRole, WorkflowDef, WorkflowRow } from "../../types";
import { listWorkflows, listWidgets, saveWorkflow, deleteWorkflow, saveWidget } from "../../lib/api";
import { listContainers } from "../../lib/storage-api";
import type { Container } from "../../lib/storage-api";
import { workflowLabel, widgetLabel } from "../../lib/builtinLabels";
import { WidgetIcon, roleColor } from "../../components/WidgetIcon";
import PipelineView from "../../components/PipelineView";
import type { PipeNode } from "../../components/PipelineView";
import TriggerChips from "../../components/TriggerChips";
import { TYPE_TAGS } from "./typeMeta";
import WidgetForm, { widgetToForm } from "../settings/WidgetForm";
import type { WidgetFormValue } from "../settings/WidgetForm";
import { inputClass, selectClass } from "../settings/constants";
import NewRecipeModal from "./NewRecipeModal";

// ─── Shared class recipes (canonical: constants.ts; match WidgetForm) ──────────
// Local width variants: the flow-name field and the inline slot pickers use
// fixed/flex widths instead of w-full, so derive from the canonical
// inputClass/selectClass (no re-literal of the shared bg/border/text/focus
// recipe) rather than duplicating the string.

const nameInputClass = inputClass.replace("w-full ", "");
const slotSelectClass = selectClass
  .replace("w-full", "flex-1 min-w-[150px]")
  .replace("px-3 py-2", "px-2.5 py-1.5");
const labelClass = "text-[10px] text-[rgba(255,255,255,0.35)]";
const headingClass =
  "text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] font-semibold";
const cancelBtnClass =
  "px-3 py-1.5 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[11px] hover:border-[rgba(255,255,255,0.1)] transition-colors flex-shrink-0";

const NEW = "__new__";
const msg = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** Strip the WorkflowRow-only `source_type_tag` before save_workflow. */
function rowToDef(w: WorkflowRow): WorkflowDef {
  const { source_type_tag: _drop, ...def } = w;
  return def;
}

// Where a picked/created widget id lands in the flow.
type SlotTarget =
  | { kind: "source" }
  | { kind: "proc-replace"; index: number }
  | { kind: "proc-insert"; index: number }
  | { kind: "out-replace"; oldId: string }
  | { kind: "out-add" };

// The below-pipeline picker: either choose an existing widget (or New), or
// fill in a brand-new widget's form.
type Picker =
  | { mode: "select"; role: WidgetRole; target: SlotTarget }
  | { mode: "new"; role: WidgetRole; target: SlotTarget; value: WidgetFormValue };

// ─── Small stateless chrome (module-level; re-render via parent useT) ──────────

function Chevron({ expanded }: { expanded: boolean }) {
  return (
    <svg
      width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.32)" strokeWidth="2.4" strokeLinecap="round"
      className={"flex-shrink-0 transition-transform duration-200 " + (expanded ? "rotate-90" : "")}
    >
      <path d="M9 18l6-6-6-6" />
    </svg>
  );
}

function IconBtn({ title, onClick, disabled, danger, children }: {
  title: string; onClick: () => void; disabled?: boolean; danger?: boolean; children: React.ReactNode;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      disabled={disabled}
      className={[
        "px-1.5 py-1 text-[12px] transition-colors disabled:opacity-20 flex-shrink-0",
        danger ? "text-[rgba(255,255,255,0.2)] hover:text-[#ef4444]" : "text-[rgba(255,255,255,0.3)] hover:text-[rgba(255,255,255,0.6)]",
      ].join(" ")}
    >
      {children}
    </button>
  );
}

/** Add-output affordance — a dashed violet capsule after the pipeline. */
function AddOutputButton({ onClick }: { onClick: () => void }) {
  const rc = roleColor("output");
  return (
    <button
      onClick={onClick}
      title={t("flows.add-output")}
      className="flex items-center gap-1.5 rounded-[9px] px-2.5 py-1.5 text-[11px] font-medium border border-dashed hover:brightness-125 transition-[filter] flex-shrink-0"
      style={{ borderColor: `rgba(${rc.rgb},0.35)`, color: `rgba(${rc.rgb},0.8)` }}
    >
      <span className="text-[13px] leading-none">+</span>
      <span className="whitespace-nowrap">{t("flows.add-output")}</span>
    </button>
  );
}

/** Flow name input with local draft, committed on blur / Enter. Keyed by flow
 *  id so it re-seeds when a different flow expands; `initial` changes (e.g.
 *  after a save+reload) re-sync it too. */
function NameField({ initial, onCommit }: { initial: string; onCommit: (v: string) => void }) {
  const [v, setV] = useState(initial);
  useEffect(() => { setV(initial); }, [initial]);
  return (
    <input
      type="text"
      value={v}
      onChange={(e) => setV(e.target.value)}
      onClick={(e) => e.stopPropagation()}
      onBlur={() => { const nv = v.trim(); if (nv && nv !== initial) onCommit(nv); }}
      onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }}
      placeholder={t("wf.ph.name")}
      style={{ width: 180 }}
      className={nameInputClass}
    />
  );
}

// ─── Main RecipesSection ────────────────────────────────────────────────────

export default function RecipesSection({ config, onBench, focusRecipe }: {
  config: AppConfig;
  onBench: (recipeId: string) => void;
  /** Jump-to-recipe intent from the Overview page's trigger cheat-sheet.
   *  `nonce` (not just recipeId) so re-clicking the same row re-triggers the
   *  effect even when the id is unchanged. */
  focusRecipe?: { recipeId: string; nonce: number } | null;
}) {
  useT();
  const [workflows, setWorkflows] = useState<WorkflowRow[]>([]);
  const [widgets, setWidgets] = useState<WidgetDef[]>([]);
  const [containers, setContainers] = useState<Container[]>([]);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [activeNodeId, setActiveNodeId] = useState<string | null>(null);
  const [picker, setPicker] = useState<Picker | null>(null);
  const [error, setError] = useState<string>("");
  const [showNew, setShowNew] = useState(false);
  // createRecipe's failure, shown inline inside NewRecipeModal — `error`
  // above renders behind the modal backdrop while the modal is open, so it's
  // invisible right when it matters most.
  const [modalError, setModalError] = useState<string | null>(null);
  // In-flight guard for trigger-chip saves: holds the id of the workflow
  // currently persisting a chip edit, so a second rapid edit on the same
  // card can't fire before the first save's reload lands (which would
  // otherwise compute its patch from stale wf.triggers).
  const [chipsSaving, setChipsSaving] = useState<string | null>(null);
  // Root of the currently-expanded flow card — used by the outside-click
  // listener below to tell inside-the-editor clicks from page-background ones.
  const expandedCardRef = useRef<HTMLDivElement>(null);

  const load = async () => {
    try {
      const [wfs, wgs] = await Promise.all([listWorkflows(), listWidgets()]);
      setWorkflows(wfs);
      setWidgets(wgs);
    } catch (e) {
      console.error("list_workflows/list_widgets:", e);
    }
  };

  useEffect(() => { load(); }, []);
  useEffect(() => {
    listContainers().then(setContainers).catch(() => { /* no backend / ignore */ });
  }, []);

  // Jump-to-recipe intent from the Overview page: expand the target card and
  // scroll to it. Guarded by a handled-nonce ref (not just the nonce itself)
  // since `workflows` is a dependency too — a jump arriving before the
  // initial listWorkflows() resolves finds no `recipe-card-{id}` element yet,
  // so this no-ops and retries on the next `workflows` update (post-load)
  // instead of firing on every later, unrelated reload.
  const handledFocusNonce = useRef<number | null>(null);
  useEffect(() => {
    if (!focusRecipe?.nonce || handledFocusNonce.current === focusRecipe.nonce) return;
    const el = document.getElementById(`recipe-card-${focusRecipe.recipeId}`);
    if (!el) return;
    handledFocusNonce.current = focusRecipe.nonce;
    setError(""); setActiveNodeId(null); setPicker(null);
    setExpandedId(focusRecipe.recipeId);
    el.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [focusRecipe?.nonce, focusRecipe?.recipeId, workflows]);

  const widgetById = (id: string): WidgetDef | undefined => widgets.find((w) => w.id === id);
  const sourceWidgets = widgets.filter((w) => w.role === "source");
  const processorWidgets = widgets.filter((w) => w.role === "processor");
  const outputWidgets = widgets.filter((w) => w.role === "output");
  const roleWidgets = (role: WidgetRole): WidgetDef[] =>
    role === "source" ? sourceWidgets : role === "processor" ? processorWidgets : outputWidgets;
  const roleLabel = (role: WidgetRole): string =>
    role === "source" ? t("wf.field.source") : role === "processor" ? t("wf.field.processors") : t("wf.field.outputs");

  // Next `order` for a manually-added pill-slot chip: past the highest pill
  // order across ALL workflows (not just the card being edited), so a new
  // pill chip never ties with another workflow's migration-assigned slot.
  const nextPillOrder =
    Math.max(999, ...workflows.flatMap((w) => (w.triggers ?? []).flatMap((tr) => (tr.kind === "pill_slot" ? [tr.order ?? 0] : [])))) + 10;

  /** source → processors → outputs, mapped to PipeNodes (label via
   *  widgetLabel; typeTag/role for icon + color). Dangling ids show the raw id. */
  const flowNodes = (wf: WorkflowRow): PipeNode[] => {
    const mk = (id: string, role: WidgetRole): PipeNode => {
      const w = widgetById(id);
      return { id, role, typeTag: w?.type_tag ?? "", label: w ? widgetLabel(w) : id };
    };
    return [
      mk(wf.source, "source"),
      ...(wf.processors ?? []).map((p) => mk(p, "processor")),
      ...wf.outputs.map((o) => mk(o, "output")),
    ];
  };

  /** Which slot a node id occupies in this flow (role + where a swap lands).
   *  Derived from the flow arrays (not the widget list) so it works even for a
   *  dangling reference. */
  const nodeSlot = (wf: WorkflowRow, id: string): { role: WidgetRole; target: SlotTarget } | null => {
    if (id === wf.source) return { role: "source", target: { kind: "source" } };
    const pi = (wf.processors ?? []).indexOf(id);
    if (pi >= 0) return { role: "processor", target: { kind: "proc-replace", index: pi } };
    if (wf.outputs.includes(id)) return { role: "output", target: { kind: "out-replace", oldId: id } };
    return null;
  };

  const micNeedsStt = (wf: WorkflowRow): boolean => {
    const sourceW = widgetById(wf.source);
    const firstProcW = (wf.processors ?? [])[0] ? widgetById((wf.processors ?? [])[0]) : undefined;
    return sourceW?.type_tag === "microphone" && firstProcW?.type_tag !== "stt";
  };

  // ── Workflow-level persistence (structural edits auto-save) ──────────────────

  const saveFlow = async (wf: WorkflowRow, patch: Partial<WorkflowDef>) => {
    setError("");
    try {
      await saveWorkflow({ ...rowToDef(wf), ...patch });
      await load();
    } catch (e) {
      setError(msg(e));
    }
  };

  /** Write a widget id into the flow per `target` and persist. Returns the
   *  save_workflow promise so new-widget flows can await it and surface chain
   *  validation errors in the WidgetForm. */
  const applyTarget = (wf: WorkflowRow, target: SlotTarget, id: string): Promise<void> => {
    let patch: Partial<WorkflowDef>;
    switch (target.kind) {
      case "source":
        patch = { source: id }; break;
      case "proc-replace":
        patch = { processors: (wf.processors ?? []).map((p, k) => (k === target.index ? id : p)) }; break;
      case "proc-insert": {
        const ps = [...(wf.processors ?? [])];
        ps.splice(target.index, 0, id);
        patch = { processors: ps }; break;
      }
      case "out-replace":
        patch = { outputs: Array.from(new Set(wf.outputs.map((o) => (o === target.oldId ? id : o)))) }; break;
      case "out-add":
        patch = { outputs: wf.outputs.includes(id) ? wf.outputs : [...wf.outputs, id] }; break;
    }
    return saveWorkflow({ ...rowToDef(wf), ...patch });
  };

  // ── Node picker actions ─────────────────────────────────────────────────────

  const openNewWidget = (role: WidgetRole, target: SlotTarget) => {
    const tt = TYPE_TAGS[role][0];
    setActiveNodeId(null);
    setPicker({
      mode: "new", role, target,
      value: { id: `${tt}.custom-${Date.now()}`, role, type_tag: tt, name: "", icon: "", props: {}, builtin: false, isNew: true },
    });
  };

  const chooseExisting = async (wf: WorkflowRow, target: SlotTarget, id: string) => {
    setError("");
    try {
      await applyTarget(wf, target, id);
      await load();
      setPicker(null);
      setActiveNodeId(id);
    } catch (e) {
      setError(msg(e));
    }
  };

  /** WidgetForm onSave for a brand-new widget: persist it, wire its id into the
   *  flow, reload. Errors propagate so WidgetForm shows them inline. */
  const saveNewWidget = async (wf: WorkflowRow, target: SlotTarget, w: WidgetDef) => {
    await saveWidget(w);
    await applyTarget(wf, target, w.id);
    await load();
    setPicker(null);
    setActiveNodeId(w.id);
  };

  const onPick = (wf: WorkflowRow, role: WidgetRole, target: SlotTarget, current: string | null, value: string) => {
    if (!value || value === current) return;
    if (value === NEW) { openNewWidget(role, target); return; }
    chooseExisting(wf, target, value);
  };

  // ── In-place widget edit ────────────────────────────────────────────────────

  const editNodeSave = async (w: WidgetDef) => {
    await saveWidget(w);  // errors propagate → WidgetForm shows them inline
    await load();
    setActiveNodeId(null);
  };

  // ── Processor reorder / removal, output removal ──────────────────────────────

  const moveProc = (wf: WorkflowRow, index: number, dir: -1 | 1) => {
    const j = index + dir;
    const ps = [...(wf.processors ?? [])];
    if (j < 0 || j >= ps.length) return;
    [ps[index], ps[j]] = [ps[j], ps[index]];
    saveFlow(wf, { processors: ps });  // activeNodeId (widget id) still matches
  };
  const removeProc = (wf: WorkflowRow, index: number) => {
    setActiveNodeId(null);
    saveFlow(wf, { processors: (wf.processors ?? []).filter((_, k) => k !== index) });
  };
  const removeOutput = (wf: WorkflowRow, id: string) => {
    if (wf.outputs.length <= 1) return;  // keep ≥1
    setActiveNodeId(null);
    saveFlow(wf, { outputs: wf.outputs.filter((o) => o !== id) });
  };

  // ── Recipe create / delete / expand ──────────────────────────────────────────

  /** New-recipe modal's onCreate: source decides the recipe's seed shape —
   *  mic seeds a dictation-shaped chain (mic-hold → stt.default → insert),
   *  selection seeds a panel-shaped chain (selection → (no processor) →
   *  panel). Ids match the built-in widgets (fonos-core builtin.rs) so the
   *  chain type-checks immediately. */
  const createRecipe = async (name: string, src: "mic" | "sel") => {
    setError(""); setModalError(null); setActiveNodeId(null); setPicker(null);
    const id = `wf.custom-${Date.now()}`;
    try {
      await saveWorkflow({
        id,
        name: name || t("flows.new-name"),
        icon: "",
        source: src === "mic" ? "src.mic-hold" : "src.selection",
        processors: src === "mic" ? ["stt.default"] : [],
        outputs: src === "mic" ? ["out.insert"] : ["out.panel"],
        builtin: false,
        triggers: [],
      });
      setShowNew(false);
      await load();
      setExpandedId(id);
    } catch (e) {
      setModalError(msg(e));
    }
  };

  const deleteFlow = async (wf: WorkflowRow) => {
    setError("");
    try {
      await deleteWorkflow(wf.id);
      if (expandedId === wf.id) { setExpandedId(null); setActiveNodeId(null); setPicker(null); }
      await load();
    } catch (e) {
      setError(msg(e));
    }
  };

  /** Collapse the open flow — the exact state changes the chevron performs
   *  when it collapses (clears expanded id + active node + picker + error).
   *  Per-edit auto-save already persisted everything, so this discards nothing
   *  beyond a half-filled new-widget-in-slot form, same as the chevron. */
  const collapse = () => {
    setError(""); setActiveNodeId(null); setPicker(null);
    setExpandedId(null);
  };

  const toggleCard = (id: string) => {
    setError(""); setActiveNodeId(null); setPicker(null);
    setExpandedId((prev) => (prev === id ? null : id));
  };

  // Clicking anywhere outside the expanded card collapses it — same effect as
  // the chevron. `mousedown` (not click) so this fires before another card's
  // expand click handler: clicking a different flow's header collapses this
  // one AND expands that one in the same gesture. Clicks inside the card
  // subtree (including the in-place node editor, which renders inline — no
  // portal) are ignored via contains(). Listener only exists while expanded.
  useEffect(() => {
    if (!expandedId) return;
    const onDocMouseDown = (e: MouseEvent) => {
      const el = expandedCardRef.current;
      if (el && !el.contains(e.target as Node)) collapse();
    };
    document.addEventListener("mousedown", onDocMouseDown);
    return () => document.removeEventListener("mousedown", onDocMouseDown);
    // collapse only calls stable state setters; re-keying on expandedId alone
    // avoids re-registering the listener on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [expandedId]);

  // ── Render: node panel (below the pipeline in an expanded flow) ───────────────

  const renderSlotBar = (wf: WorkflowRow, slot: { role: WidgetRole; target: SlotTarget }, id: string) => {
    const { role, target } = slot;
    const rc = roleColor(role);
    const items = roleWidgets(role);
    const present = items.some((w) => w.id === id);
    const procIndex = target.kind === "proc-replace" ? target.index : -1;
    return (
      <div className="flex items-center gap-1.5 flex-wrap rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.05)] px-2.5 py-2">
        <span className="text-[9px] uppercase tracking-wider font-semibold flex-shrink-0" style={{ color: `rgba(${rc.rgb},0.8)` }}>
          {roleLabel(role)}
        </span>
        <select value={id} onChange={(e) => onPick(wf, role, target, id, e.target.value)} className={slotSelectClass}>
          {!present && <option value={id}>{id}</option>}
          {items.map((w) => (<option key={w.id} value={w.id}>{widgetLabel(w)}</option>))}
          <option value={NEW}>{t("flows.pick.new")}</option>
        </select>
        {role === "processor" && (
          <>
            <IconBtn title={t("wf.step.up")} disabled={procIndex <= 0} onClick={() => moveProc(wf, procIndex, -1)}>↑</IconBtn>
            <IconBtn title={t("wf.step.down")} disabled={procIndex >= (wf.processors ?? []).length - 1} onClick={() => moveProc(wf, procIndex, 1)}>↓</IconBtn>
            <IconBtn title={t("flows.node.remove")} danger onClick={() => removeProc(wf, procIndex)}>✕</IconBtn>
          </>
        )}
        {role === "output" && wf.outputs.length > 1 && (
          <IconBtn title={t("flows.node.remove")} danger onClick={() => removeOutput(wf, id)}>✕</IconBtn>
        )}
      </div>
    );
  };

  const renderNodePanel = (wf: WorkflowRow) => {
    // Picker (insert processor / add output / new widget) takes precedence.
    if (picker) {
      if (picker.mode === "new") {
        return (
          <WidgetForm
            key={picker.value.id}
            value={picker.value}
            config={config}
            containers={containers}
            typeTags={TYPE_TAGS[picker.role]}
            onSave={(w) => saveNewWidget(wf, picker.target, w)}
            onCancel={() => setPicker(null)}
          />
        );
      }
      const items =
        picker.role === "output"
          ? outputWidgets.filter((w) => !wf.outputs.includes(w.id))
          : roleWidgets(picker.role);
      const placeholder = picker.role === "output" ? t("flows.pick.output") : t("flows.pick.processor");
      return (
        <div className="rounded-[10px] bg-[rgba(255,255,255,0.025)] border border-[rgba(255,255,255,0.06)] p-3 flex items-center gap-2">
          <select autoFocus value="" onChange={(e) => onPick(wf, picker.role, picker.target, null, e.target.value)} className={slotSelectClass}>
            <option value="">{placeholder}</option>
            {items.map((w) => (<option key={w.id} value={w.id}>{widgetLabel(w)}</option>))}
            <option value={NEW}>{t("flows.pick.new")}</option>
          </select>
          <button onClick={() => setPicker(null)} className={cancelBtnClass}>{t("common.cancel")}</button>
        </div>
      );
    }

    if (!activeNodeId) return null;
    const slot = nodeSlot(wf, activeNodeId);
    if (!slot) return null;  // node no longer in flow — cleared by its own action
    const w = widgetById(activeNodeId);
    return (
      <div className="flex flex-col gap-2 mt-1">
        {renderSlotBar(wf, slot, activeNodeId)}
        {w ? (
          <WidgetForm
            key={activeNodeId}
            value={widgetToForm(w)}
            config={config}
            containers={containers}
            onSave={editNodeSave}
            onCancel={() => setActiveNodeId(null)}
          />
        ) : (
          <div className="text-[11px] text-[#ef4444] px-1">{t("flows.dangling")}</div>
        )}
      </div>
    );
  };

  // ── Render: one flow card ─────────────────────────────────────────────────────

  const flowIcon = (wf: WorkflowRow) => {
    const src = widgetById(wf.source);
    const rc = roleColor("source");
    return (
      <span
        className="w-[30px] h-[30px] rounded-[8px] flex items-center justify-center flex-shrink-0"
        style={{ background: `rgba(${rc.rgb},0.12)`, color: `rgba(${rc.rgb},0.95)` }}
      >
        <WidgetIcon typeTag={src?.type_tag ?? "panel"} size={16} />
      </span>
    );
  };

  const renderCard = (wf: WorkflowRow) => {
    const expanded = expandedId === wf.id;
    // Head subtitle is always the pipeline summary (same in both states) —
    // the edit hint lives only directly above the pipeline editor below, so
    // expanding no longer shows the same sentence twice.
    const sub = flowNodes(wf).map((n) => n.label).join("  →  ");
    return (
      <div
        key={wf.id}
        id={`recipe-card-${wf.id}`}
        ref={expanded ? expandedCardRef : undefined}
        className={[
          "rounded-[12px] transition-colors",
          expanded
            ? "border border-[rgba(242,184,75,0.3)] bg-[rgba(242,184,75,0.02)]"
            : "border border-[rgba(255,255,255,0.06)] bg-[rgba(255,255,255,0.02)] hover:border-[rgba(255,255,255,0.10)]",
        ].join(" ")}
      >
        {/* Head — click toggles expand */}
        <div className="flex items-center gap-[11px] px-[14px] py-3 cursor-pointer" onClick={() => toggleCard(wf.id)}>
          {flowIcon(wf)}
          <div className="flex-1 min-w-0">
            <div className="text-[12px] font-medium text-[#fafaf9] truncate">{workflowLabel(wf)}</div>
            <div className="text-[10.5px] text-[rgba(255,255,255,0.32)] truncate mt-px">{sub}</div>
          </div>
          {wf.builtin && (
            <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded flex-shrink-0">
              {t("wf.section.preset")}
            </span>
          )}
          <button
            onClick={(e) => { e.stopPropagation(); onBench(wf.id); }}
            className="flex-shrink-0 rounded-[8px] border border-[rgba(242,184,75,0.2)] bg-[rgba(240,173,50,0.06)] px-3 py-[5px] text-[11px] text-[rgba(242,184,75,0.85)] hover:border-[rgba(242,184,75,0.4)] hover:text-[var(--accent)] transition-colors"
          >
            {t("wb.recipes.bench")}
          </button>
          <Chevron expanded={expanded} />
        </div>

        {/* Trigger row — shown collapsed and expanded, inside the card root. */}
        <div
          className={
            "px-[14px] pb-[11px] pt-[9px] mt-0 border-t border-[rgba(255,255,255,0.045)]" +
            (chipsSaving === wf.id ? " opacity-60 pointer-events-none" : "")
          }
        >
          <TriggerChips
            wf={wf}
            isMic={wf.source_type_tag === "microphone"}
            nextPillOrder={nextPillOrder}
            onChange={async (triggers) => {
              if (chipsSaving) return;
              setChipsSaving(wf.id);
              try { await saveFlow(wf, { triggers }); } finally { setChipsSaving(null); }
            }}
          />
        </div>

        {expanded ? (
          <>
            {/* Editor chrome: name + delete */}
            <div className="px-4 pb-3 flex gap-2.5 items-end flex-wrap">
              <div className="flex flex-col gap-1">
                <label className={labelClass}>{t("wf.field.name")}</label>
                <NameField key={wf.id} initial={wf.name} onCommit={(name) => saveFlow(wf, { name })} />
              </div>
              {!wf.builtin && (
                <button
                  onClick={() => deleteFlow(wf)}
                  className="ml-auto self-center text-[11px] text-[rgba(239,68,68,0.6)] hover:text-[#ef4444] px-2 py-1 transition-colors"
                >
                  {t("flows.delete-flow")}
                </button>
              )}
            </div>

            <div className="px-4 pb-1 text-[10.5px] text-[rgba(255,255,255,0.32)]">{t("flows.hint.edit")}</div>

            {/* Interactive pipeline + add-output */}
            <div className="px-4 pt-1 pb-2 flex items-center gap-1.5 flex-wrap">
              <PipelineView
                interactive
                nodes={flowNodes(wf)}
                activeId={activeNodeId ?? undefined}
                onNodeClick={(id) => { setPicker(null); setActiveNodeId(id); }}
                onAddStep={(idx) => { setActiveNodeId(null); setPicker({ mode: "select", role: "processor", target: { kind: "proc-insert", index: idx } }); }}
              />
              <AddOutputButton onClick={() => { setActiveNodeId(null); setPicker({ mode: "select", role: "output", target: { kind: "out-add" } }); }} />
            </div>

            {micNeedsStt(wf) && (
              <div className="px-4 pb-2 text-[10px] text-[#ef4444] leading-relaxed">{t("wf.hint.mic-needs-stt")}</div>
            )}
            {error && <div className="px-4 pb-2 text-[11px] text-[#ef4444] leading-relaxed">{error}</div>}

            {/* In-place node panel */}
            <div className="px-4 pb-4">{renderNodePanel(wf)}</div>
          </>
        ) : (
          /* Collapsed — read-only pipeline */
          <div className="px-[14px] pb-[13px] pl-[55px]">
            <PipelineView nodes={flowNodes(wf)} interactive={false} />
          </div>
        )}
      </div>
    );
  };

  // ── Render ────────────────────────────────────────────────────────────────────

  const presets = workflows.filter((w) => w.builtin);
  const customs = workflows.filter((w) => !w.builtin);

  return (
    <div className="flex flex-col gap-5">
      {error && expandedId === null && (
        <div className="text-[11px] text-[#ef4444] leading-relaxed">{error}</div>
      )}

      {/* Preset */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <span className={headingClass}>{t("wf.section.preset")}</span>
          <span className="text-[9px] text-[rgba(255,255,255,0.15)]">({presets.length})</span>
        </div>
        {presets.map(renderCard)}
      </div>

      {/* Custom */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <span className={headingClass}>{t("wf.section.custom")}</span>
          <span className="text-[9px] text-[rgba(255,255,255,0.15)]">({customs.length})</span>
        </div>
        {customs.length === 0 && (
          <div className="text-[11px] text-[rgba(255,255,255,0.25)] italic py-1">{t("wf.empty.custom")}</div>
        )}
        {customs.map(renderCard)}
        <button
          onClick={() => { setModalError(null); setShowNew(true); }}
          className="w-full py-2.5 rounded-[11px] border border-dashed border-[rgba(242,184,75,0.14)] text-[rgba(242,184,75,0.65)] text-[11px] hover:border-[rgba(242,184,75,0.3)] hover:text-[var(--accent)] transition-colors flex items-center justify-center gap-1.5"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><line x1="12" y1="5" x2="12" y2="19" /><line x1="5" y1="12" x2="19" y2="12" /></svg>
          {t("wb.recipes.new")}
        </button>
      </div>

      <NewRecipeModal
        open={showNew}
        onClose={() => { setShowNew(false); setModalError(null); }}
        onCreate={createRecipe}
        errorText={modalError}
      />
    </div>
  );
}
