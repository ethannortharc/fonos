// Reusable per-type_tag widget property form (Flows UI redesign, Task 2) —
// extracted verbatim from WidgetsTab.tsx's PropsForm/ModelSelector/VocabChips
// so BuildingBlocks.tsx and RecipesSection.tsx can both reuse it: once for the
// widget-library editor, once for in-place pipeline-node editing.
//
// WidgetForm owns its own editable copy of `value` (reset whenever the
// target widget's `id` changes — callers may either mount it with
// `key={value.id}` or reuse one instance across targets, both work). It is
// otherwise a "dumb" controlled form: it never calls save_widget/
// delete_widget itself. Save delegates to `onSave`, awaiting it so a
// rejected promise (the backend's validation error) surfaces inline here,
// exactly like WidgetsTab's original error handling. Delete delegates to
// `onDelete` (void, fire-and-forget) — callers own delete-error display
// (e.g. the referrer-list message), since there's no channel back to this
// form for that; a "click again to confirm" guard (reusing the existing
// widgets.confirm-delete key) is the only safety net here.
//
// Icon editing is gone: the icon is always rendered from `type_tag` via
// WidgetIcon (Task 1), tinted by the widget's role via roleColor. The
// `icon` string field is still carried through to WidgetDef for backend
// compatibility, but is no longer user-editable — see task-2-report.md.
//
// isNew type/id picker (Task 3 addition): Task 2 scoped identity to
// name+icon only, leaving no way to choose a brand-new widget's type_tag or
// id. When `value.isNew`, the identity block now also renders a `type_tag`
// <select> (options via the new `typeTags` prop) + an editable `id` text
// input; switching type resets `props` to `{}` (props are type-specific),
// mirroring old WidgetsTab's `changeType`. Existing widgets are unaffected —
// they still show the read-only type_tag/id/builtin badge row.

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { t, useT } from "../../lib/i18n";
import { isMacOS } from "../../lib/platform";
import type { AppConfig, ModelProfile, VocabBook, WidgetDef, WidgetRole } from "../../types";
import type { Container } from "../../lib/storage-api";
import { createContainer } from "../../lib/storage-api";
import { WidgetIcon, roleColor } from "../../components/WidgetIcon";
import { widgetLabel } from "../../lib/builtinLabels";
import { LANGUAGES, inputClass, selectClass } from "./constants";
import SkillsPanel from "../workbench/SkillsPanel";
import NotebookCombobox from "./NotebookCombobox";

// ─── Shared class recipes (canonical: constants.ts; match WidgetsTab/WorkflowsTab) ──

const textareaClass = inputClass + " leading-relaxed resize-none font-mono";
const labelClass = "text-[10px] text-[rgba(255,255,255,0.35)]";
const headingClass =
  "text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] font-semibold";

// ─── Props accessors (props is an untyped JSON object) ────────────────────────
// Exported so BuildingBlocks/RecipesSection can read widget props the same way.

export type Props = Record<string, unknown>;
export const pStr = (p: Props, k: string, d = ""): string => (typeof p[k] === "string" ? (p[k] as string) : d);
export const pNum = (p: Props, k: string, d = 0): number => (typeof p[k] === "number" ? (p[k] as number) : d);
export const pBool = (p: Props, k: string, d = false): boolean => (typeof p[k] === "boolean" ? (p[k] as boolean) : d);
export const pArr = (p: Props, k: string): string[] => (Array.isArray(p[k]) ? (p[k] as string[]) : []);

// ─── Editing form model ───────────────────────────────────────────────────────

export interface WidgetFormValue {
  id: string;
  role: WidgetRole;
  type_tag: string;
  name: string;
  icon: string;
  props: Props;
  builtin: boolean;
  /** New (unsaved) widget: hides the Delete action. */
  isNew: boolean;
}

/** Build editable form state from a saved widget. `name` is prefilled with
 *  the translated built-in label (falls back to the literal name for
 *  customs) so the identity field shows something sensible regardless of
 *  UI language — saving back the translated string is harmless for
 *  built-ins since widgetLabel() always prefers BUILTIN_LABELS over the
 *  stored name for known ids. */
export function widgetToForm(w: WidgetDef): WidgetFormValue {
  return {
    id: w.id, role: w.role, type_tag: w.type_tag, name: widgetLabel(w),
    icon: w.icon ?? "", props: { ...(w.props ?? {}) }, builtin: !!w.builtin, isNew: false,
  };
}

function formToWidget(f: WidgetFormValue): WidgetDef {
  return {
    id: f.id.trim(),
    role: f.role,
    type_tag: f.type_tag,
    name: f.name.trim(),
    icon: f.icon,
    props: f.props,
    builtin: f.builtin,
  };
}

// ─── Small building blocks ─────────────────────────────────────────────────────

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <label className={labelClass}>{label}</label>
      {children}
    </div>
  );
}

/** Model dropdown filtered by capability.
 *  Empty value = fall back to the matching global profile (stt/llm/tts). */
export function ModelSelector({
  capKey, value, profiles, onChange,
}: {
  capKey: string;
  value: string;
  profiles: ModelProfile[];
  onChange: (v: string) => void;
}) {
  const filtered = profiles.filter((p) => p.capabilities?.includes(capKey));
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} className={selectClass} style={{ backgroundImage: "none" }}>
      <option value="">{t("modes.use-default")}</option>
      {capKey === "stt" && isMacOS && <option value="apple-speech">{t("modes.stt.apple")}</option>}
      {filtered.map((p) => (
        <option key={p.id} value={p.id}>{p.name} ({p.model})</option>
      ))}
      {filtered.length === 0 && capKey !== "stt" && (
        <option disabled value="__none__">{t("modes.no-models").replace("{cap}", capKey.toUpperCase())}</option>
      )}
    </select>
  );
}

/** Widget-instance dropdown for a composite's ref prop (Workbench P2
 *  foundation for the "dialog"/"call"/"agent"/"meeting" composites built in
 *  T4/T6-T9 — e.g. a "call" widget's stt_widget/llm_widget prop names the
 *  "stt"/"llm" widget it delegates to). Modeled on ModelSelector above:
 *  filtered to widgets of `wantTag`, empty value = "use default" (reuses
 *  modes.use-default — same copy, same "no override" meaning as
 *  ModelSelector's empty option). The "dialog"/"agent"/"meeting"/"call"
 *  PropsForm cases below all render it. */
export function WidgetRefSelector({
  wantTag, value, widgets, onChange,
}: {
  wantTag: string;
  value: string;
  widgets: WidgetDef[];
  onChange: (v: string) => void;
}) {
  const filtered = widgets.filter((w) => w.type_tag === wantTag);
  return (
    <select value={value} onChange={(e) => onChange(e.target.value)} className={selectClass} style={{ backgroundImage: "none" }}>
      <option value="">{t("modes.use-default")}</option>
      {filtered.map((w) => (
        <option key={w.id} value={w.id}>{widgetLabel(w)}</option>
      ))}
    </select>
  );
}

export function VocabChips({
  books, selected, onToggle,
}: {
  books: VocabBook[];
  selected: string[];
  onToggle: (id: string) => void;
}) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {books.map((b) => {
        const on = selected.includes(b.id);
        return (
          <button
            key={b.id}
            onClick={() => onToggle(b.id)}
            className={[
              "px-2.5 py-1 rounded-full text-[10px] transition-all",
              on
                ? "bg-[rgba(242,184,75,0.12)] border border-[rgba(242,184,75,0.3)] text-[var(--accent)]"
                : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] hover:border-[rgba(255,255,255,0.12)]",
            ].join(" ")}
          >
            {b.name}
          </button>
        );
      })}
    </div>
  );
}

/** Panel/dialog window size: preset buttons (S/M/L) + width/height number
 *  inputs, shared by the "panel" and "dialog" cases below. `size` is the
 *  widget's `props.size` object (possibly partial/undefined — defaults to
 *  420×320, matching PanelSize::default() on the backend). */
function SizeControl({
  size, onChange,
}: {
  size: { width?: number; height?: number };
  onChange: (size: { width: number; height: number }) => void;
}) {
  const width = size.width ?? 420;
  const height = size.height ?? 320;
  const presets: { key: string; label: string; width: number; height: number }[] = [
    { key: "s", label: t("widgets.size.s"), width: 320, height: 240 },
    { key: "m", label: t("widgets.size.m"), width: 420, height: 320 },
    { key: "l", label: t("widgets.size.l"), width: 560, height: 440 },
  ];
  return (
    <div className="flex flex-col gap-2">
      <div className="flex gap-1.5">
        {presets.map((preset) => {
          const active = width === preset.width && height === preset.height;
          return (
            <button
              key={preset.key}
              onClick={() => onChange({ width: preset.width, height: preset.height })}
              className={[
                "px-2.5 py-1 rounded-full text-[10px] transition-all",
                active
                  ? "bg-[rgba(242,184,75,0.12)] border border-[rgba(242,184,75,0.3)] text-[var(--accent)]"
                  : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] hover:border-[rgba(255,255,255,0.12)]",
              ].join(" ")}
            >
              {preset.label}
            </button>
          );
        })}
      </div>
      <div className="grid grid-cols-2 gap-2">
        <Field label={t("widgets.field.width")}>
          <input
            type="number" min={1} value={width}
            onChange={(e) => onChange({ width: parseInt(e.target.value) || width, height })}
            className={inputClass}
          />
        </Field>
        <Field label={t("widgets.field.height")}>
          <input
            type="number" min={1} value={height}
            onChange={(e) => onChange({ height: parseInt(e.target.value) || height, width })}
            className={inputClass}
          />
        </Field>
      </div>
    </div>
  );
}

/** Collapsed "Skills (global)" section rendered under the "agent" PropsForm
 *  case. Skills are a global registry (not per-widget-instance config — see
 *  `SkillsPanel`'s own header comment), so this just embeds the panel behind
 *  a disclosure toggle rather than exposing any of its own props. A separate
 *  component (not inlined in the "agent" case body) so its `useState` toggle
 *  is never called conditionally: `PropsForm`'s switch can re-render a
 *  different `case` across renders (the isNew type picker lets a user change
 *  `form.type_tag`), which would violate the rules of hooks if the toggle
 *  lived directly in that case's branch. */
function AgentSkillsSection() {
  const [open, setOpen] = useState(false);
  return (
    <div className="flex flex-col gap-2 pt-1" style={{ borderTop: "1px solid rgba(255,255,255,0.04)" }}>
      <button
        onClick={() => setOpen((o) => !o)}
        className="self-start text-[10px] text-[rgba(255,255,255,0.35)] hover:text-[rgba(255,255,255,0.55)] transition-colors"
      >
        {open ? "▾" : "▸"} {t("widgets.field.agent.skills")}
      </button>
      {open && <SkillsPanel />}
    </div>
  );
}

/** Speaker-separation toggle + "model not downloaded" hint for the "meeting"
 *  PropsForm case (Task 5 of the diarization epic). Split into its own
 *  component (same reason as AgentSkillsSection above): PropsForm's switch
 *  renders a different case per render whenever `form.type_tag` changes (the
 *  isNew type picker lets it), so a hook called directly inside one case's
 *  branch would violate the rules of hooks. The one-shot diarize_check probe
 *  (T4's command — `{available, models_present}`) lives here instead, and
 *  defaults `modelsReady` to true so the hint doesn't flash on before the
 *  probe resolves (mirrors the brief's "default 不吓人" note). */
function MeetingDiarizeField({
  checked, onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  const [modelsReady, setModelsReady] = useState(true);
  useEffect(() => {
    invoke<{ available: boolean; models_present: boolean }>("diarize_check")
      .then((s) => setModelsReady(s.available && s.models_present))
      .catch(() => setModelsReady(false));
  }, []);
  return (
    <>
      <label className="flex items-center gap-1.5 cursor-pointer text-[12px] text-[rgba(255,255,255,0.5)]">
        <input
          type="checkbox"
          checked={checked}
          onChange={(e) => onChange(e.target.checked)}
          className="accent-[var(--accent)]"
        />
        {t("widgets.field.meeting.diarize")}
      </label>
      {checked && !modelsReady && (
        <div className="text-[11px] text-[rgba(242,184,75,0.7)]">
          {t("widgets.field.meeting.diarize.no-model")}
        </div>
      )}
    </>
  );
}

// ─── Per-type_tag property form ────────────────────────────────────────────────

function PropsForm({
  form, config, containers, widgets, onProps,
}: {
  form: WidgetFormValue;
  config: AppConfig;
  containers: Container[];
  /** Loaded widget instances, for composite cases' WidgetRefSelector
   *  (stt_widget/llm_widget dropdowns). Threaded through from WidgetForm;
   *  "dialog" (Task 4), "agent" (Task 6), "meeting" (Task 7), and "call"
   *  (Task 9) consume it. */
  widgets: WidgetDef[];
  onProps: (props: Props) => void;
}) {
  const p = form.props;
  const set = (k: string, v: unknown) => onProps({ ...p, [k]: v });
  const books = config.vocab_books ?? [];
  const toggleBook = (key: string) => (id: string) => {
    const cur = pArr(p, key);
    set(key, cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id]);
  };

  switch (form.type_tag) {
    case "llm":
      return (
        <div className="flex flex-col gap-2.5">
          <Field label={t("widgets.field.model")}>
            <ModelSelector capKey="llm" value={pStr(p, "model_profile")} profiles={config.model_profiles} onChange={(v) => set("model_profile", v)} />
          </Field>
          <Field label={t("widgets.field.system")}>
            <textarea value={pStr(p, "system")} onChange={(e) => set("system", e.target.value)} rows={3} className={textareaClass} />
          </Field>
          <Field label={t("widgets.field.user_template")}>
            <textarea value={pStr(p, "user_template", "{text}")} onChange={(e) => set("user_template", e.target.value)} rows={2} className={textareaClass} />
          </Field>
          <div className="grid grid-cols-2 gap-2">
            <Field label={t("widgets.field.temperature")}>
              <input type="number" min={0} max={2} step={0.1} value={pNum(p, "temperature", 0.1)} onChange={(e) => set("temperature", parseFloat(e.target.value) || 0)} className={inputClass} />
            </Field>
            <Field label={t("widgets.field.max_tokens")}>
              <input type="number" min={1} max={128000} step={256} value={pNum(p, "max_tokens", 4096)} onChange={(e) => set("max_tokens", parseInt(e.target.value) || 4096)} className={inputClass} />
            </Field>
          </div>
          <Field label={t("widgets.field.output_language")}>
            <input type="text" value={pStr(p, "output_language", "auto")} onChange={(e) => set("output_language", e.target.value)} className={inputClass} />
          </Field>
          {books.length > 0 && (
            <Field label={t("widgets.field.vocab_books")}>
              <VocabChips books={books} selected={pArr(p, "vocab_books")} onToggle={toggleBook("vocab_books")} />
            </Field>
          )}
        </div>
      );

    case "stt":
      return (
        <div className="flex flex-col gap-2.5">
          <Field label={t("widgets.field.model")}>
            <ModelSelector capKey="stt" value={pStr(p, "model_profile")} profiles={config.model_profiles} onChange={(v) => set("model_profile", v)} />
          </Field>
          <Field label={t("widgets.field.language")}>
            <select value={pStr(p, "language", "auto")} onChange={(e) => set("language", e.target.value)} className={selectClass}>
              {LANGUAGES.map((l) => (
                <option key={l.code} value={l.code}>{l.flag} {l.label}</option>
              ))}
            </select>
          </Field>
          <Field label={t("widgets.field.stt_prompt")}>
            <input type="text" value={pStr(p, "stt_prompt")} onChange={(e) => set("stt_prompt", e.target.value)} className={inputClass} />
          </Field>
          <Field label={t("widgets.field.temperature")}>
            <input type="number" min={0} max={1} step={0.1} value={pNum(p, "temperature", 0)} onChange={(e) => set("temperature", parseFloat(e.target.value) || 0)} className={inputClass} />
          </Field>
          {books.length > 0 && (
            <Field label={t("widgets.field.vocab_books")}>
              <VocabChips books={books} selected={pArr(p, "vocab_books")} onToggle={toggleBook("vocab_books")} />
            </Field>
          )}
        </div>
      );

    case "notebook": {
      const notebooks = containers.filter((c) => c.container_type === "notebook");
      return (
        <Field label={t("widgets.field.container_id")}>
          <NotebookCombobox
            key={form.id}
            containerId={pNum(p, "container_id", 0)}
            pendingTitle={pStr(p, "container_title")}
            notebooks={notebooks}
            onChange={(sel) => {
              const next: Props = { ...p };
              if (sel.kind === "existing") {
                next.container_id = sel.container_id;
                delete next.container_title;
              } else {
                next.container_title = sel.title;
              }
              onProps(next);
            }}
          />
        </Field>
      );
    }

    case "insert":
      return (
        <div className="flex flex-col gap-2.5">
          <Field label={t("widgets.field.strategy")}>
            <select value={pStr(p, "strategy", "paste")} onChange={(e) => set("strategy", e.target.value)} className={selectClass}>
              <option value="paste">{t("widgets.strategy.paste")}</option>
              <option value="type">{t("widgets.strategy.type")}</option>
            </select>
          </Field>
          <label className="flex items-center gap-1.5 cursor-pointer text-[12px] text-[rgba(255,255,255,0.5)]">
            <input type="checkbox" checked={pBool(p, "press_enter")} onChange={(e) => set("press_enter", e.target.checked)} className="accent-[var(--accent)]" />
            {t("widgets.field.press_enter")}
          </label>
        </div>
      );

    case "speak":
      return (
        <div className="flex flex-col gap-2.5">
          <Field label={t("widgets.field.voice_profile")}>
            <ModelSelector capKey="tts" value={pStr(p, "voice_profile")} profiles={config.model_profiles} onChange={(v) => set("voice_profile", v)} />
          </Field>
          <Field label={t("widgets.field.voice")}>
            <input type="text" value={pStr(p, "voice", "default")} onChange={(e) => set("voice", e.target.value)} className={inputClass} />
          </Field>
        </div>
      );

    case "panel":
      return (
        <div className="flex flex-col gap-2.5">
          <label className="flex items-center gap-1.5 cursor-pointer text-[12px] text-[rgba(255,255,255,0.5)]">
            <input type="checkbox" checked={pBool(p, "markdown")} onChange={(e) => set("markdown", e.target.checked)} className="accent-[var(--accent)]" />
            {t("widgets.field.markdown")}
          </label>
          <Field label={t("widgets.field.size")}>
            <SizeControl size={(p.size as { width?: number; height?: number }) ?? {}} onChange={(size) => set("size", size)} />
          </Field>
        </div>
      );

    case "dialog": {
      const engine = (p.engine as { kind?: string; model_profile?: string; system?: string | null }) ?? {};
      const setEngine = (patch: Partial<{ model_profile: string; system: string }>) =>
        set("engine", { kind: "llm", model_profile: engine.model_profile ?? "", system: engine.system ?? "", ...patch });
      // Task 4 (additive ref): a non-empty `llm_widget` top-level prop wins
      // over the inline engine fields below — the desktop's DialogOutput
      // resolves it the same way (fonos_core::workflow::dialog::
      // resolve_llm_engine). Hides the inline model/system fields in favor
      // of a hint, rather than disabling them, so their stale values (if
      // any, from before a ref was chosen) aren't visually implied to still
      // matter.
      const llmWidget = pStr(p, "llm_widget");
      return (
        <div className="flex flex-col gap-2.5">
          <label className="flex items-center gap-1.5 cursor-pointer text-[12px] text-[rgba(255,255,255,0.5)]">
            <input type="checkbox" checked={pBool(p, "markdown")} onChange={(e) => set("markdown", e.target.checked)} className="accent-[var(--accent)]" />
            {t("widgets.field.markdown")}
          </label>
          <Field label={t("widgets.field.size")}>
            <SizeControl size={(p.size as { width?: number; height?: number }) ?? {}} onChange={(size) => set("size", size)} />
          </Field>
          <div className={headingClass}>{t("widgets.field.engine")}</div>
          <Field label={t("widgets.field.dialog.llm_widget")}>
            <WidgetRefSelector wantTag="llm" value={llmWidget} widgets={widgets} onChange={(v) => set("llm_widget", v)} />
          </Field>
          {llmWidget ? (
            <div className="text-[11px] text-[rgba(255,255,255,0.35)] italic">
              {t("widgets.field.dialog.provided-by-widget")}
            </div>
          ) : (
            <>
              <Field label={t("widgets.field.model")}>
                <ModelSelector capKey="llm" value={engine.model_profile ?? ""} profiles={config.model_profiles} onChange={(v) => setEngine({ model_profile: v })} />
              </Field>
              <Field label={t("widgets.field.dialog.system")}>
                <textarea value={engine.system ?? ""} onChange={(e) => setEngine({ system: e.target.value })} rows={3} className={textareaClass} />
              </Field>
            </>
          )}
        </div>
      );
    }

    case "agent": {
      const llmWidget = pStr(p, "llm_widget");
      return (
        <div className="flex flex-col gap-2.5">
          <Field label={t("widgets.field.agent.llm_widget")}>
            <WidgetRefSelector wantTag="llm" value={llmWidget} widgets={widgets} onChange={(v) => set("llm_widget", v)} />
          </Field>
          {/* system (persona) — inline fallback, mirrors the "dialog" case's
              hide-on-ref pattern above: a non-empty llm_widget ref supplies
              its own system prompt, so the inline field (and its now-stale
              value, if any) is hidden behind the same shared hint rather than
              shown-but-inert. Fix Round 1. */}
          {llmWidget ? (
            <div className="text-[11px] text-[rgba(255,255,255,0.35)] italic">
              {t("widgets.field.dialog.provided-by-widget")}
            </div>
          ) : (
            <Field label={t("widgets.field.agent.system")}>
              <textarea value={pStr(p, "system")} onChange={(e) => set("system", e.target.value)} rows={3} className={textareaClass} />
            </Field>
          )}
          <label className="flex items-center gap-1.5 cursor-pointer text-[12px] text-[rgba(255,255,255,0.5)]">
            <input type="checkbox" checked={pBool(p, "tts_enabled")} onChange={(e) => set("tts_enabled", e.target.checked)} className="accent-[var(--accent)]" />
            {t("widgets.field.agent.tts_enabled")}
          </label>
          <Field label={t("widgets.field.voice_profile")}>
            <ModelSelector capKey="tts" value={pStr(p, "voice_profile")} profiles={config.model_profiles} onChange={(v) => set("voice_profile", v)} />
          </Field>
          <Field label={t("widgets.field.voice")}>
            <input type="text" value={pStr(p, "voice", "default")} onChange={(e) => set("voice", e.target.value)} className={inputClass} />
          </Field>
          <Field label={t("widgets.field.agent.timeout_secs")}>
            <input
              type="number" min={5} max={120} value={pNum(p, "timeout_secs", 30)}
              onChange={(e) => set("timeout_secs", parseInt(e.target.value) || 30)}
              className={inputClass}
            />
          </Field>
          <AgentSkillsSection />
        </div>
      );
    }

    case "meeting": {
      return (
        <div className="flex flex-col gap-2.5">
          <Field label={t("widgets.field.meeting.stt_widget")}>
            <WidgetRefSelector wantTag="stt" value={pStr(p, "stt_widget")} widgets={widgets} onChange={(v) => set("stt_widget", v)} />
          </Field>
          <Field label={t("widgets.field.meeting.llm_widget")}>
            <WidgetRefSelector wantTag="llm" value={pStr(p, "llm_widget")} widgets={widgets} onChange={(v) => set("llm_widget", v)} />
          </Field>
          <Field label={t("widgets.field.meeting.summary_prompt")}>
            <textarea value={pStr(p, "summary_prompt")} onChange={(e) => set("summary_prompt", e.target.value)} rows={4} className={textareaClass} />
          </Field>
          <MeetingDiarizeField checked={pBool(p, "diarize", false)} onChange={(v) => set("diarize", v)} />
        </div>
      );
    }

    case "call": {
      // Every field here is genuinely wired (Task 9): stt_widget/llm_widget
      // resolve in CallOutput's ResolvedCallCfg, voice_profile/voice drive
      // the reply TTS, and max_turns/vad_sensitivity/vad_silence_ms/barge_in
      // parameterize the call loop. The llm ref supplies BOTH the model and
      // the persona (its `system`); empty = the built-in call persona.
      return (
        <div className="flex flex-col gap-2.5">
          <Field label={t("widgets.field.call.stt_widget")}>
            <WidgetRefSelector wantTag="stt" value={pStr(p, "stt_widget")} widgets={widgets} onChange={(v) => set("stt_widget", v)} />
          </Field>
          <Field label={t("widgets.field.call.llm_widget")}>
            <WidgetRefSelector wantTag="llm" value={pStr(p, "llm_widget")} widgets={widgets} onChange={(v) => set("llm_widget", v)} />
          </Field>
          <Field label={t("widgets.field.voice_profile")}>
            <ModelSelector capKey="tts" value={pStr(p, "voice_profile")} profiles={config.model_profiles} onChange={(v) => set("voice_profile", v)} />
          </Field>
          <Field label={t("widgets.field.voice")}>
            <input type="text" value={pStr(p, "voice", "default")} onChange={(e) => set("voice", e.target.value)} className={inputClass} />
          </Field>
          <div className="grid grid-cols-2 gap-2">
            <Field label={t("widgets.field.call.max_turns")}>
              <input
                type="number" min={0} max={50} value={pNum(p, "max_turns", 8)}
                onChange={(e) => set("max_turns", Math.max(0, Math.min(50, parseInt(e.target.value) || 0)))}
                className={inputClass}
              />
            </Field>
            <Field label={t("widgets.field.call.vad_silence_ms")}>
              <input
                type="number" min={500} max={2000} step={100} value={pNum(p, "vad_silence_ms", 800)}
                onChange={(e) => set("vad_silence_ms", parseInt(e.target.value) || 800)}
                className={inputClass}
              />
            </Field>
          </div>
          <Field label={t("widgets.field.call.vad_sensitivity")}>
            <input
              type="number" min={0} max={1} step={0.05} value={pNum(p, "vad_sensitivity", 0.5)}
              onChange={(e) => set("vad_sensitivity", Math.max(0, Math.min(1, parseFloat(e.target.value) || 0)))}
              className={inputClass}
            />
          </Field>
          <label className="flex items-center gap-1.5 cursor-pointer text-[12px] text-[rgba(255,255,255,0.5)]">
            <input type="checkbox" checked={pBool(p, "barge_in", true)} onChange={(e) => set("barge_in", e.target.checked)} className="accent-[var(--accent)]" />
            {t("widgets.field.call.barge_in")}
          </label>
        </div>
      );
    }

    // selection / replace / clipboard — no configurable props.
    case "uppercase":
    default:
      return (
        <div className="text-[11px] text-[rgba(255,255,255,0.25)] italic py-1">
          {t("widgets.no-config")}
        </div>
      );
  }
}

// ─── Main WidgetForm ────────────────────────────────────────────────────────

export default function WidgetForm({
  value, config, containers, widgets, typeTags, onSave, onCancel, onDelete, deleteError, onContainerCreated, readOnly = false,
}: {
  value: WidgetFormValue;
  config: AppConfig;
  containers: Container[];
  /** Every loaded widget instance — threaded down to PropsForm for composite
   *  cases' WidgetRefSelector (stt_widget/llm_widget dropdowns filtered by
   *  type_tag). Dialog (Task 4), agent (Task 6), meeting (Task 7), and call
   *  (Task 9) consume it. */
  widgets: WidgetDef[];
  /** Allowed type_tags for a NEW widget of value.role (e.g. BuildingBlocks'
   *  TYPE_TAGS[role]) — populates the isNew type_tag <select> below.
   *  Ignored for existing widgets (type_tag is fixed once saved). Falls
   *  back to [value.type_tag] when omitted, so the picker still renders
   *  something usable even if a caller forgets to pass it. */
  typeTags?: string[];
  /** Optional so read-only callers (the Building Blocks catalog) can omit it —
   *  the Save button is never rendered when `readOnly`, so persistence is moot
   *  there. Interactive callers (RecipesSection) always pass it. */
  onSave?: (w: WidgetDef) => Promise<void> | void;
  onCancel: () => void;
  onDelete?: () => void;
  /** Delete-referrer error owned by the caller (there's no channel back to
   *  this form for delete outcomes — see the header comment). When set,
   *  rendered inline in the card's footer, near the Delete button, instead
   *  of the caller having to render it as a detached sibling below the form. */
  deleteError?: string;
  /** Fired after a "notebook" widget's pending title is turned into a real
   *  container at save time. The `containers` list an owner threads in is
   *  loaded once, so it goes stale the moment we mint a new notebook here;
   *  the owner reloads its list in response, keeping name-is-identity honest
   *  — a sibling widget typing the same title then exact-matches this new
   *  notebook instead of forking a duplicate. Read-only consumers (Building
   *  Blocks catalog) never create containers, so they may omit it. */
  onContainerCreated?: () => void;
  /** Read-only detail view (Building Blocks catalog): every field is disabled
   *  (via a wrapping disabled <fieldset>) and the footer shows only a Close
   *  button wired to onCancel. Never combined with isNew. Default false keeps
   *  every existing caller (RecipesSection) visually and behaviourally unchanged. */
  readOnly?: boolean;
}) {
  useT();
  const [form, setForm] = useState<WidgetFormValue>(value);
  const [error, setError] = useState<string>("");
  const [confirmDelete, setConfirmDelete] = useState(false);

  // Re-sync local editable state when the caller points this form at a
  // different widget. Keyed on `value.id` (not the whole object) so a
  // caller that re-renders with a fresh-but-equivalent `value` on every
  // keystroke elsewhere in the tree doesn't clobber in-progress edits.
  useEffect(() => {
    setForm(value);
    setError("");
    setConfirmDelete(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value.id]);

  const rc = roleColor(form.role);

  // isNew only: props are type-specific, so switching type resets them.
  // The id is also regenerated to keep its `<type>.custom-…` prefix in sync
  // with the new type — but ONLY while the id is still the untouched
  // auto-generated value (matches `^[a-z]+\.custom-\d+$`, the pattern the
  // BuildingBlocks/RecipesSection "New" affordances mint). A hand-edited id is
  // never clobbered.
  const changeType = (type_tag: string) => {
    const isAutoId = /^[a-z]+\.custom-\d+$/.test(form.id);
    const id = form.isNew && isAutoId ? `${type_tag}.custom-${Date.now()}` : form.id;
    setForm({ ...form, type_tag, id, props: {} });
  };

  const handleSave = async () => {
    if (!onSave) return;
    if (!form.name.trim()) { setError(t("widgets.err.name-required")); return; }
    if (!form.id.trim()) { setError(t("widgets.err.type-required")); return; }
    setError("");
    try {
      let props = form.props;
      // Name-is-identity notebooks: a pending title (typed text matching no
      // existing notebook) becomes a real container only at save time —
      // cancelling the form never creates anything (spec §1).
      if (form.type_tag === "notebook") {
        const pending = pStr(props, "container_title").trim();
        if (pending) {
          const created = await createContainer(pending, "notebook");
          props = { ...props, container_id: created.id };
          delete (props as Record<string, unknown>).container_title;
          // Commit the exchanged props into form state right away: if the
          // create succeeds but onSave below rejects, a retry must bind the
          // already-created id — not re-run createContainer off the stale
          // container_title and fork a duplicate notebook (spec §1).
          setForm((f) => ({ ...f, props }));
          // The owner's `containers` list is now stale — ask it to reload so a
          // sibling widget can exact-match this new notebook by name.
          onContainerCreated?.();
        }
      }
      await onSave(formToWidget({ ...form, props }));
    } catch (e) {
      // The caller's save (backend validation) rejected — show it inline,
      // same as WidgetsTab's original handleSave.
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleCancel = () => {
    setError("");
    onCancel();
  };

  const handleDeleteClick = () => {
    if (!onDelete) return;
    if (!confirmDelete) { setConfirmDelete(true); return; }
    setConfirmDelete(false);
    onDelete();
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-4 animate-[panel-in_0.18s_ease] motion-reduce:animate-none">
        <div className="text-[12px] font-medium text-[#fafaf9]">
          {readOnly ? t("widgets.editor.view") : form.isNew ? t("widgets.editor.new") : t("widgets.editor.edit")}
        </div>

        {error && <div className="text-[11px] text-[#ef4444]">{error}</div>}

        {/* Fields — a disabled fieldset (display:contents, so zero layout
            change) propagates `disabled` to every descendant input/select/
            textarea/checkbox/button in read-only mode. */}
        <fieldset disabled={readOnly} className="contents">

        {/* Identity: icon (read-only, by type_tag) + name */}
        <div className="flex flex-col gap-2">
          <div className={headingClass}>{t("widgets.editor.identity")}</div>
          <div className="grid grid-cols-[48px_1fr] gap-2">
            <div
              title={form.type_tag}
              className="flex items-center justify-center rounded-lg"
              style={{
                background: `rgba(${rc.rgb},0.08)`,
                border: `1px solid rgba(${rc.rgb},0.22)`,
                color: `rgba(${rc.rgb},0.95)`,
              }}
            >
              <WidgetIcon typeTag={form.type_tag} size={18} />
            </div>
            <input
              type="text"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              placeholder={t("widgets.ph.name")}
              className={inputClass}
            />
          </div>
          {form.isNew ? (
            <div className="grid grid-cols-2 gap-2">
              <Field label={t("widgets.field.type")}>
                <select value={form.type_tag} onChange={(e) => changeType(e.target.value)} className={selectClass}>
                  {(typeTags && typeTags.length > 0 ? typeTags : [form.type_tag]).map((tt) => (
                    <option key={tt} value={tt}>{tt}</option>
                  ))}
                </select>
              </Field>
              <Field label={t("widgets.field.id")}>
                <input
                  type="text"
                  value={form.id}
                  onChange={(e) => setForm({ ...form, id: e.target.value })}
                  className={inputClass + " font-mono"}
                />
              </Field>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <span className="text-[9px] text-[rgba(255,255,255,0.3)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded font-mono">{form.type_tag}</span>
              <span className="text-[9px] text-[rgba(255,255,255,0.2)] font-mono truncate">{form.id}</span>
              {form.builtin && (
                <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded">{t("common.builtin")}</span>
              )}
            </div>
          )}
        </div>

        {/* Configuration (per-type_tag) */}
        <div className="flex flex-col gap-2">
          <div className={headingClass}>{t("widgets.editor.config")}</div>
          <PropsForm
            form={form}
            config={config}
            containers={containers}
            widgets={widgets}
            onProps={(props) => setForm({ ...form, props })}
          />
        </div>

        </fieldset>

        {/* Actions — read-only shows only a Close button (the Cancel button,
            relabelled + wired to onCancel); no Save/Delete. */}
        {readOnly ? (
          <div className="flex gap-2 pt-1 items-center">
            <button
              onClick={handleCancel}
              className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[11px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
            >
              {t("widgets.close")}
            </button>
          </div>
        ) : (
          <div className="flex gap-2 pt-1 items-center">
            <button
              onClick={handleSave}
              className="flex-1 py-2 rounded-lg bg-gradient-to-r from-[var(--accent-from)] to-[var(--accent-to)] text-[#1a1917] text-[11px] font-semibold hover:opacity-90 transition-opacity"
            >
              {t("common.save")}
            </button>
            <button
              onClick={handleCancel}
              className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[11px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
            >
              {t("common.cancel")}
            </button>
            {/* Built-in / not-yet-saved widgets can't be deleted — hide the button. */}
            {!form.isNew && !form.builtin && onDelete && (
              <button
                onClick={handleDeleteClick}
                className="px-3 py-2 rounded-lg bg-transparent border border-[rgba(239,68,68,0.1)] text-[rgba(239,68,68,0.6)] text-[11px] hover:text-[#ef4444] hover:border-[rgba(239,68,68,0.3)] transition-colors"
              >
                {confirmDelete ? t("widgets.confirm-delete") : t("common.delete")}
              </button>
            )}
          </div>
        )}

        {deleteError && (
          <div className="text-[11px] text-[#ef4444] leading-relaxed">{deleteError}</div>
        )}
      </div>
    </div>
  );
}
