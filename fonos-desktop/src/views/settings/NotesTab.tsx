// Notes settings — Quick Note (default) config + per-notebook STT/LLM/prompt + hotkey binding.

import { useState, useEffect } from "react";
import { PinIcon } from "../../components/Icons";
import { listContainers, createContainer, deleteContainer } from "../../lib/storage-api";
import type { Container } from "../../lib/storage-api";
import type { AppConfig, ModelProfile } from "../../types";
import { t, useT } from "../../lib/i18n";

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] font-semibold">
      {children}
    </div>
  );
}

const PROCESSORS = [
  { id: "light_polish", label: "ntab.proc.light" },
  { id: "none", label: "ntab.proc.raw" },
  { id: "summarize", label: "ntab.proc.summarize" },
] as const;

const selectClass = "w-full bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-1.5 text-[11px] text-[#fafaf9] focus:outline-none focus:border-[rgba(245,158,11,0.3)] cursor-pointer appearance-none";
const inputClass = "w-full bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-1.5 text-[11px] text-[#fafaf9] focus:outline-none focus:border-[rgba(245,158,11,0.3)]";
const textareaClass = "w-full bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-2 text-[11px] text-[#fafaf9] leading-relaxed focus:outline-none focus:border-[rgba(245,158,11,0.3)] resize-none font-mono";
const labelClass = "text-[10px] text-[rgba(255,255,255,0.35)]";

interface NbConfig {
  processor: string;
  stt_profile: string;
  llm_profile: string;
  prompt: string;
}

function getNbConfig(nb: Container): NbConfig {
  const m = (nb.metadata as Record<string, unknown>) || {};
  return {
    processor: (m.processor as string) || "light_polish",
    stt_profile: (m.stt_profile as string) || "",
    llm_profile: (m.llm_profile as string) || "",
    prompt: (m.prompt as string) || "Clean up filler words, fix punctuation. Keep original meaning. Output only cleaned text.",
  };
}

// ─── Config fields for a single notebook (reused for Quick Note and custom) ──

function NbConfigFields({
  cfg, profiles, onChange,
}: {
  cfg: NbConfig;
  profiles: ModelProfile[];
  onChange: (field: string, value: string) => void;
}) {
  const sttProfiles = profiles.filter((p) => p.capabilities?.includes("stt"));
  const llmProfiles = profiles.filter((p) => p.capabilities?.includes("llm"));

  return (
    <div className="flex flex-col gap-2.5">
      <div className="flex flex-col gap-1">
        <label className={labelClass}>{t("ntab.processor")}</label>
        <select value={cfg.processor} onChange={(e) => onChange("processor", e.target.value)} className={selectClass}>
          {PROCESSORS.map((p) => <option key={p.id} value={p.id}>{t(p.label)}</option>)}
        </select>
      </div>
      <div className="flex flex-col gap-1">
        <label className={labelClass}>{t("ntab.stt-model")}</label>
        <select value={cfg.stt_profile} onChange={(e) => onChange("stt_profile", e.target.value)} className={selectClass}>
          <option value="">{t("ntab.default-global")}</option>
          <option value="apple-speech">{t("ntab.apple-speech")}</option>
          {sttProfiles.map((p) => <option key={p.id} value={p.id}>{p.name} ({p.model})</option>)}
        </select>
      </div>
      <div className="flex flex-col gap-1">
        <label className={labelClass}>{t("ntab.llm-model")}</label>
        <select value={cfg.llm_profile} onChange={(e) => onChange("llm_profile", e.target.value)} className={selectClass}>
          <option value="">{t("ntab.default-global")}</option>
          {llmProfiles.map((p) => <option key={p.id} value={p.id}>{p.name} ({p.model})</option>)}
        </select>
      </div>
      <div className="flex flex-col gap-1">
        <label className={labelClass}>{t("ntab.prompt")}</label>
        <textarea value={cfg.prompt} onChange={(e) => onChange("prompt", e.target.value)} rows={2} className={textareaClass} />
      </div>
    </div>
  );
}

// ─── Per-notebook editor with hotkey binding ─────────────────────────────────

function NotebookEditor({
  notebook, profiles, onUpdate, onDelete, boundHotkey,
}: {
  notebook: Container;
  profiles: ModelProfile[];
  onUpdate: (id: number, metadata: Record<string, unknown>) => void;
  onDelete: (id: number) => void;
  boundHotkey: string | null; // e.g. "option+1" or null
}) {
  const [expanded, setExpanded] = useState(false);
  const cfg = getNbConfig(notebook);

  const save = (field: string, value: string) => {
    const newMeta = { ...((notebook.metadata as Record<string, unknown>) || {}), [field]: value };
    onUpdate(notebook.id, newMeta);
  };

  return (
    <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)] transition-colors">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3.5 py-2.5 text-left"
      >
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.2)" strokeWidth="2" strokeLinecap="round"
          className={`flex-shrink-0 transition-transform duration-200 ${expanded ? "rotate-90" : ""}`}><path d="M9 18l6-6-6-6" /></svg>
        <div className="w-[6px] h-[6px] rounded-full bg-[#4ade80] flex-shrink-0" />
        <span className="flex-1 text-[12px] font-medium text-[#fafaf9] truncate">{notebook.title}</span>
        {boundHotkey && (
          <span className="text-[10px] text-[#fbbf24] bg-[rgba(245,158,11,0.08)] border border-[rgba(245,158,11,0.15)] rounded px-1.5 py-0.5 font-mono flex-shrink-0">
            {boundHotkey}
          </span>
        )}
        <span className="text-[9px] text-[rgba(255,255,255,0.15)] flex-shrink-0">{cfg.processor}</span>
      </button>

      {expanded && (
        <div className="px-3.5 pb-3 pt-1 border-t border-[rgba(255,255,255,0.04)] flex flex-col gap-2.5">
          {/* Show bound hotkey if any (read-only — configure in Hotkeys tab) */}
          {boundHotkey && (
            <div className="flex items-center gap-2 pb-1">
              <label className={labelClass}>{t("ntab.hotkey")}</label>
              <span className="text-[11px] text-[#fbbf24] font-mono bg-[rgba(245,158,11,0.08)] rounded px-2 py-0.5">
                {boundHotkey}
              </span>
              <span className="text-[9px] text-[rgba(255,255,255,0.15)]">{t("ntab.change-in-hotkeys")}</span>
            </div>
          )}

          <NbConfigFields cfg={cfg} profiles={profiles} onChange={save} />

          <div className="flex justify-end pt-1">
            <button
              onClick={() => onDelete(notebook.id)}
              className="text-[10px] text-[rgba(239,68,68,0.5)] hover:text-[#ef4444] px-2 py-1 rounded-md bg-[rgba(239,68,68,0.04)] border border-[rgba(239,68,68,0.08)] hover:border-[rgba(239,68,68,0.2)] transition-colors"
            >
              {t("ntab.delete")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

// ─── Main NotesTab ──────────────────────────────────────────────────────────

export default function NotesTab({ config, onSave }: { config: AppConfig; onSave: (u: Partial<AppConfig>) => void }) {
  useT();
  const [notebooks, setNotebooks] = useState<Container[]>([]);
  const [quickNote, setQuickNote] = useState<Container | null>(null);
  const [newTitle, setNewTitle] = useState("");
  const [creating, setCreating] = useState(false);

  const profiles: ModelProfile[] = config.model_profiles || [];

  // Look up bound hotkey string for a notebook ID
  const hotkeyBindings: { id: number; hotkey: string }[] = [];
  if (config.notebook_hotkey_1 && config.hotkey_note_1) hotkeyBindings.push({ id: config.notebook_hotkey_1, hotkey: config.hotkey_note_1 });
  if (config.notebook_hotkey_2 && config.hotkey_note_2) hotkeyBindings.push({ id: config.notebook_hotkey_2, hotkey: config.hotkey_note_2 });
  if (config.notebook_hotkey_3 && config.hotkey_note_3) hotkeyBindings.push({ id: config.notebook_hotkey_3, hotkey: config.hotkey_note_3 });

  const getBoundHotkey = (nbId: number): string | null => {
    const found = hotkeyBindings.find((b) => b.id === nbId);
    return found?.hotkey ?? null;
  };

  const hasHotkey = (nbId: number): boolean => getBoundHotkey(nbId) !== null;

  const loadNotebooks = async () => {
    try {
      const all = await listContainers();
      const nbs = all.filter((c) => c.container_type === "notebook");
      const qn = nbs.find((c) => c.title === "Quick Note");
      setQuickNote(qn || null);
      setNotebooks(nbs.filter((c) => c.title !== "Quick Note"));
    } catch { /* ignore */ }
  };

  useEffect(() => { loadNotebooks(); }, []);

  const handleCreate = async () => {
    if (!newTitle.trim()) return;
    setCreating(true);
    try {
      await createContainer(newTitle.trim(), "notebook");
      setNewTitle("");
      loadNotebooks();
    } catch (e) { console.error(e); }
    setCreating(false);
  };

  const handleDelete = async (id: number) => {
    try {
      await deleteContainer(id);
      loadNotebooks();
    } catch (e) { console.error(e); }
  };

  const handleUpdateMeta = async (id: number, metadata: Record<string, unknown>) => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("update_container_metadata", { id, metadata: JSON.stringify(metadata) });
      loadNotebooks();
    } catch (e) {
      console.error("update metadata:", e);
    }
  };

  // Sort notebooks: hotkey-bound first (by slot number), then alphabetical
  const sortedNotebooks = [...notebooks].sort((a, b) => {
    const aHk = hasHotkey(a.id);
    const bHk = hasHotkey(b.id);
    if (aHk && !bHk) return -1;
    if (!aHk && bHk) return 1;
    return a.title.localeCompare(b.title);
  });

  // Quick Note config — stored in container metadata (same structure as other notebooks)
  const qnCfg = quickNote ? getNbConfig(quickNote) : {
    processor: config.note_processor || "light_polish",
    stt_profile: config.note_stt_profile || "",
    llm_profile: config.note_llm_profile || "",
    prompt: config.note_prompt || "Clean up filler words, fix punctuation. Keep original meaning. Output only cleaned text.",
  };

  const handleQnChange = (field: string, value: string) => {
    if (quickNote) {
      const newMeta = { ...((quickNote.metadata as Record<string, unknown>) || {}), [field]: value };
      handleUpdateMeta(quickNote.id, newMeta);
    } else {
      // Fallback to AppConfig fields
      const map: Record<string, string> = { processor: "note_processor", stt_profile: "note_stt_profile", llm_profile: "note_llm_profile", prompt: "note_prompt" };
      if (map[field]) onSave({ [map[field]]: value } as Partial<AppConfig>);
    }
  };

  return (
    <div className="flex flex-col gap-4">

      {/* ── Quick Note (Default Notebook) ── */}
      <div className="flex flex-col gap-2">
        <SectionLabel><span className="inline-flex items-center gap-1"><PinIcon size={11} /> {t("ntab.quick-note")}</span></SectionLabel>
        <p className="text-[9px] text-[rgba(255,255,255,0.2)] -mt-1">
          {t("ntab.quick-note-desc")} <span className="font-mono text-[rgba(255,255,255,0.3)]">{config.hotkey_note || "option+n"}</span>
        </p>
        <NbConfigFields cfg={qnCfg} profiles={profiles} onChange={handleQnChange} />
      </div>

      {/* ── Other Notebooks ── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("ntab.notebooks")}</SectionLabel>
        <p className="text-[9px] text-[rgba(255,255,255,0.2)] -mt-1">
          {t("ntab.notebooks-hint")}
        </p>

        {/* Create new */}
        <div className="flex gap-2">
          <input
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleCreate()}
            placeholder={t("ntab.new-placeholder")}
            className={inputClass + " flex-1"}
          />
          <button
            onClick={handleCreate}
            disabled={creating || !newTitle.trim()}
            className={[
              "px-3 py-1.5 rounded-lg text-[11px] font-semibold transition-opacity whitespace-nowrap",
              "bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-[#1a1917]",
              creating || !newTitle.trim() ? "opacity-30" : "hover:opacity-90",
            ].join(" ")}
          >
            {t("ntab.create")}
          </button>
        </div>

        {/* Notebook list */}
        {sortedNotebooks.length === 0 ? (
          <div className="text-[rgba(255,255,255,0.15)] text-[11px] py-3 text-center">
            {t("ntab.empty")}
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {sortedNotebooks.map((nb) => (
              <NotebookEditor
                key={nb.id}
                notebook={nb}
                profiles={profiles}
                onUpdate={handleUpdateMeta}
                onDelete={handleDelete}
                boundHotkey={getBoundHotkey(nb.id)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
