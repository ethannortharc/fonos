// Widgets tab (Workflow P1, Task 14) — lists/edits widget instances grouped
// into Sources / Processors / Outputs, with a per-type_tag property form.
//
// Built-in widgets are editable (saveWidget overrides them by id) but never
// deletable, so they show no delete button. Custom widgets can be deleted;
// the backend rejects deletion of a widget still referenced by a workflow and
// returns the referrer list, which we surface verbatim in red.

import { useState, useEffect } from "react";
import { t, useT } from "../../lib/i18n";
import type { TKey } from "../../lib/i18n";
import type { AppConfig, ModelProfile, VocabBook, WidgetDef, WidgetRole } from "../../types";
import { listWidgets, saveWidget, deleteWidget } from "../../lib/api";
import { listContainers } from "../../lib/storage-api";
import type { Container } from "../../lib/storage-api";

// ─── Shared class recipes (match the other settings tabs) ─────────────────────

const inputClass =
  "w-full bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]";
const selectClass = inputClass + " cursor-pointer appearance-none";
const textareaClass = inputClass + " leading-relaxed resize-none font-mono";
const labelClass = "text-[10px] text-[rgba(255,255,255,0.35)]";
const headingClass =
  "text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] font-semibold";

// The type_tags each role can instantiate — mirrors the desktop registry
// (workflow_widgets.rs build_registry). v1 hardcoded map.
const TYPE_TAGS: Record<WidgetRole, string[]> = {
  source: ["microphone", "selection"],
  processor: ["stt", "llm"],
  output: ["insert", "replace", "clipboard", "notebook", "speak", "panel"],
};

const ROLES: { role: WidgetRole; label: TKey }[] = [
  { role: "source", label: "widgets.section.sources" },
  { role: "processor", label: "widgets.section.processors" },
  { role: "output", label: "widgets.section.outputs" },
];

const DEFAULT_ICONS: Record<string, string> = {
  microphone: "\u{1F399}", selection: "\u{1F5B1}", stt: "\u{270D}", llm: "✨",
  insert: "⌨", replace: "\u{1F501}", clipboard: "\u{1F4CB}",
  notebook: "\u{1F4D3}", speak: "\u{1F50A}", panel: "\u{1FA9F}",
};
const defaultIcon = (tt: string) => DEFAULT_ICONS[tt] ?? "\u{1F9E9}";

// ─── Props accessors (props is an untyped JSON object) ────────────────────────

type Props = Record<string, unknown>;
const pStr = (p: Props, k: string, d = ""): string => (typeof p[k] === "string" ? (p[k] as string) : d);
const pNum = (p: Props, k: string, d = 0): number => (typeof p[k] === "number" ? (p[k] as number) : d);
const pBool = (p: Props, k: string, d = false): boolean => (typeof p[k] === "boolean" ? (p[k] as boolean) : d);
const pArr = (p: Props, k: string): string[] => (Array.isArray(p[k]) ? (p[k] as string[]) : []);

// ─── Editing form model ───────────────────────────────────────────────────────

interface WidgetForm {
  id: string;
  role: WidgetRole;
  type_tag: string;
  name: string;
  icon: string;
  props: Props;
  builtin: boolean;
  /** New (unsaved) widget: id + type_tag are editable; existing: they are fixed. */
  isNew: boolean;
}

function widgetToForm(w: WidgetDef): WidgetForm {
  return {
    id: w.id, role: w.role, type_tag: w.type_tag, name: w.name,
    icon: w.icon ?? "", props: { ...(w.props ?? {}) }, builtin: !!w.builtin, isNew: false,
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
function ModelSelector({
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
      {capKey === "stt" && <option value="apple-speech">{t("modes.stt.apple")}</option>}
      {filtered.map((p) => (
        <option key={p.id} value={p.id}>{p.name} ({p.model})</option>
      ))}
      {filtered.length === 0 && capKey !== "stt" && (
        <option disabled value="__none__">{t("modes.no-models").replace("{cap}", capKey.toUpperCase())}</option>
      )}
    </select>
  );
}

function VocabChips({
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
                ? "bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.3)] text-[#fbbf24]"
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

// ─── Per-type_tag property form ────────────────────────────────────────────────

function PropsForm({
  form, config, containers, onProps,
}: {
  form: WidgetForm;
  config: AppConfig;
  containers: Container[];
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

    case "microphone":
      return (
        <Field label={t("widgets.field.capture")}>
          <select value={pStr(p, "capture", "hold")} onChange={(e) => set("capture", e.target.value)} className={selectClass}>
            <option value="hold">{t("widgets.field.capture.hold")}</option>
            <option value="toggle">{t("widgets.field.capture.toggle")}</option>
          </select>
        </Field>
      );

    case "notebook": {
      const notebooks = containers.filter((c) => c.container_type === "notebook");
      return (
        <Field label={t("widgets.field.container_id")}>
          <select value={String(pNum(p, "container_id", 0))} onChange={(e) => set("container_id", parseInt(e.target.value) || 0)} className={selectClass}>
            <option value="0">{t("widgets.notebook.quick")}</option>
            {notebooks.map((c) => (
              <option key={c.id} value={c.id}>{c.title}</option>
            ))}
          </select>
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
            <input type="checkbox" checked={pBool(p, "press_enter")} onChange={(e) => set("press_enter", e.target.checked)} className="accent-[#fbbf24]" />
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
        <label className="flex items-center gap-1.5 cursor-pointer text-[12px] text-[rgba(255,255,255,0.5)]">
          <input type="checkbox" checked={pBool(p, "markdown")} onChange={(e) => set("markdown", e.target.checked)} className="accent-[#fbbf24]" />
          {t("widgets.field.markdown")}
        </label>
      );

    // selection / replace / clipboard — no configurable props.
    default:
      return (
        <div className="text-[11px] text-[rgba(255,255,255,0.25)] italic py-1">
          {t("widgets.no-config")}
        </div>
      );
  }
}

// ─── Widget card (list row) ────────────────────────────────────────────────────

function WidgetCard({ w, onClick }: { w: WidgetDef; onClick: () => void }) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onClick(); }
      }}
      className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)] transition-colors cursor-pointer flex items-center gap-2.5 px-3.5 py-2.5"
    >
      <span className="flex-shrink-0 text-[15px] leading-none">{w.icon || "\u{1F9E9}"}</span>
      <span className="flex-1 min-w-0 text-[#fafaf9] text-[12px] font-medium truncate">{w.name}</span>
      <span className="text-[9px] text-[rgba(255,255,255,0.3)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded font-mono flex-shrink-0">{w.type_tag}</span>
      {w.builtin && (
        <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded flex-shrink-0">{t("common.builtin")}</span>
      )}
    </div>
  );
}

// ─── Main WidgetsTab ───────────────────────────────────────────────────────────

export default function WidgetsTab({ config }: { config: AppConfig }) {
  useT();
  const [widgets, setWidgets] = useState<WidgetDef[]>([]);
  const [containers, setContainers] = useState<Container[]>([]);
  const [editing, setEditing] = useState<WidgetForm | null>(null);
  const [error, setError] = useState<string>("");
  const [deleteErr, setDeleteErr] = useState<string>("");

  const load = async () => {
    try {
      setWidgets(await listWidgets());
    } catch (e) {
      console.error("list_widgets:", e);
    }
  };

  useEffect(() => { load(); }, []);
  useEffect(() => {
    listContainers().then(setContainers).catch(() => { /* no backend / ignore */ });
  }, []);

  const openNew = (role: WidgetRole) => {
    setError(""); setDeleteErr("");
    const type_tag = TYPE_TAGS[role][0];
    setEditing({
      id: `${type_tag}.custom-${Date.now()}`,
      role, type_tag, name: "", icon: defaultIcon(type_tag), props: {}, builtin: false, isNew: true,
    });
  };

  const openTemplate = (w: WidgetDef) => {
    setError(""); setDeleteErr("");
    setEditing({
      id: `llm.custom-${Date.now()}`,
      role: "processor", type_tag: "llm",
      name: `${w.name} (copy)`, icon: w.icon || defaultIcon("llm"),
      props: { ...(w.props ?? {}) }, builtin: false, isNew: true,
    });
  };

  const openEdit = (w: WidgetDef) => {
    setError(""); setDeleteErr("");
    setEditing(widgetToForm(w));
  };

  const changeType = (type_tag: string) => {
    if (!editing) return;
    // Props are type-specific, so reset them when the type changes on a new widget.
    setEditing({ ...editing, type_tag, props: {}, icon: editing.icon || defaultIcon(type_tag) });
  };

  const handleSave = async () => {
    if (!editing) return;
    if (!editing.name.trim()) { setError(t("widgets.err.name-required")); return; }
    if (!editing.id.trim()) { setError(t("widgets.err.type-required")); return; }
    setError("");
    try {
      await saveWidget({
        id: editing.id.trim(),
        role: editing.role,
        type_tag: editing.type_tag,
        name: editing.name.trim(),
        icon: editing.icon,
        props: editing.props,
        builtin: editing.builtin,
      });
      setEditing(null);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDelete = async () => {
    if (!editing) return;
    setDeleteErr("");
    try {
      await deleteWidget(editing.id);
      setEditing(null);
      await load();
    } catch (e) {
      // Backend rejects referenced widgets with the referrer list in the message.
      setDeleteErr(e instanceof Error ? e.message : String(e));
    }
  };

  // ── Editor ──────────────────────────────────────────────────────────────────
  if (editing) {
    return (
      <div className="flex flex-col gap-3">
        <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-4">
          <div className="text-[12px] font-medium text-[#fafaf9]">
            {editing.isNew ? t("widgets.editor.new") : t("widgets.editor.edit")}
          </div>

          {error && <div className="text-[11px] text-[#ef4444]">{error}</div>}

          {/* Identity */}
          <div className="flex flex-col gap-2">
            <div className={headingClass}>{t("widgets.editor.identity")}</div>
            <div className="grid grid-cols-[48px_1fr] gap-2">
              <input
                type="text"
                value={editing.icon}
                onChange={(e) => setEditing({ ...editing, icon: e.target.value })}
                title={t("widgets.field.icon")}
                className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2 py-2 text-center text-[16px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
              />
              <input
                type="text"
                value={editing.name}
                onChange={(e) => setEditing({ ...editing, name: e.target.value })}
                placeholder={t("widgets.ph.name")}
                className={inputClass}
              />
            </div>
            {editing.isNew ? (
              <div className="grid grid-cols-2 gap-2">
                <Field label={t("widgets.field.type")}>
                  <select value={editing.type_tag} onChange={(e) => changeType(e.target.value)} className={selectClass}>
                    {TYPE_TAGS[editing.role].map((tt) => (
                      <option key={tt} value={tt}>{tt}</option>
                    ))}
                  </select>
                </Field>
                <Field label="ID">
                  <input
                    type="text"
                    value={editing.id}
                    onChange={(e) => setEditing({ ...editing, id: e.target.value })}
                    className={inputClass + " font-mono"}
                  />
                </Field>
              </div>
            ) : (
              <div className="flex items-center gap-2">
                <span className="text-[9px] text-[rgba(255,255,255,0.3)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded font-mono">{editing.type_tag}</span>
                <span className="text-[9px] text-[rgba(255,255,255,0.2)] font-mono truncate">{editing.id}</span>
                {editing.builtin && (
                  <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded">{t("common.builtin")}</span>
                )}
              </div>
            )}
          </div>

          {/* Configuration (per-type_tag) */}
          <div className="flex flex-col gap-2">
            <div className={headingClass}>{t("widgets.editor.config")}</div>
            <PropsForm
              form={editing}
              config={config}
              containers={containers}
              onProps={(props) => setEditing({ ...editing, props })}
            />
          </div>

          {/* Actions */}
          <div className="flex gap-2 pt-1 items-center">
            <button
              onClick={handleSave}
              className="flex-1 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-[#1a1917] text-[12px] font-semibold hover:opacity-90 transition-opacity"
            >
              {t("common.save")}
            </button>
            <button
              onClick={() => { setEditing(null); setError(""); setDeleteErr(""); }}
              className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
            >
              {t("common.cancel")}
            </button>
            {/* Built-in widgets can't be deleted — hide the button entirely. */}
            {!editing.isNew && !editing.builtin && (
              <button
                onClick={handleDelete}
                className="px-3 py-2 rounded-lg bg-transparent border border-[rgba(239,68,68,0.1)] text-[rgba(239,68,68,0.6)] text-[12px] hover:text-[#ef4444] hover:border-[rgba(239,68,68,0.3)] transition-colors"
              >
                {t("common.delete")}
              </button>
            )}
          </div>

          {/* Referrer-list / delete error, verbatim from the backend. */}
          {deleteErr && <div className="text-[10px] text-[#ef4444] leading-relaxed">{deleteErr}</div>}
        </div>
      </div>
    );
  }

  // ── List: three role sections ─────────────────────────────────────────────────
  return (
    <div className="flex flex-col gap-5">
      {ROLES.map(({ role, label }) => {
        const items = widgets.filter((w) => w.role === role);
        const llmTemplates = widgets.filter((w) => w.role === "processor" && w.type_tag === "llm" && w.builtin);
        return (
          <div key={role} className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <span className={headingClass}>{t(label)}</span>
              <span className="text-[9px] text-[rgba(255,255,255,0.15)]">({items.length})</span>
            </div>

            {items.map((w) => (
              <WidgetCard key={w.id} w={w} onClick={() => openEdit(w)} />
            ))}

            {/* Processors: copy an LLM template into a new custom processor. */}
            {role === "processor" && llmTemplates.length > 0 && (
              <select
                value=""
                onChange={(e) => {
                  const w = llmTemplates.find((x) => x.id === e.target.value);
                  if (w) openTemplate(w);
                }}
                className={selectClass + " text-[rgba(251,191,36,0.7)]"}
              >
                <option value="">{t("widgets.copy-template")}</option>
                {llmTemplates.map((w) => (
                  <option key={w.id} value={w.id}>{w.name}</option>
                ))}
              </select>
            )}

            <button
              onClick={() => openNew(role)}
              className="w-full py-2 rounded-[10px] border border-dashed border-[rgba(245,158,11,0.12)] text-[rgba(251,191,36,0.6)] text-[12px] hover:border-[rgba(245,158,11,0.25)] transition-colors"
            >
              {t("widgets.new")}
            </button>
          </div>
        );
      })}
    </div>
  );
}
