// Hotkeys tab — organized into sections with notebook selector for note shortcuts.

import { useState, useCallback, useEffect } from "react";
import { NotebookIcon } from "../../components/Icons";
import { listModes } from "../../lib/api";
import type { ModeEntry } from "../../types";
import { listContainers } from "../../lib/storage-api";
import type { Container } from "../../lib/storage-api";
import type { AppConfig } from "../../types";

// ─── Hotkey capture input ────────────────────────────────────────────────────

function HotkeyInput({ value, onChange, placeholder }: {
  value: string; onChange: (v: string) => void; placeholder?: string;
}) {
  const [capturing, setCapturing] = useState(false);

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!capturing) return;
    e.preventDefault();
    const parts: string[] = [];
    if (e.metaKey) parts.push("cmd");
    if (e.ctrlKey) parts.push("ctrl");
    if (e.altKey) parts.push("alt");
    if (e.shiftKey) parts.push("shift");
    if (e.key && !["Meta", "Control", "Alt", "Shift"].includes(e.key)) {
      parts.push(e.key.toLowerCase());
    }
    if (parts.length > 1) { onChange(parts.join("+")); setCapturing(false); }
  }, [capturing, onChange]);

  return (
    <input
      type="text"
      value={capturing ? "Press hotkey..." : value}
      readOnly
      onFocus={() => setCapturing(true)}
      onBlur={() => setCapturing(false)}
      onKeyDown={handleKeyDown}
      placeholder={placeholder ?? "Click to set"}
      className={[
        "bg-[rgba(255,255,255,0.03)] border rounded-lg px-3 py-1.5 text-[#fafaf9] text-[11px] focus:outline-none font-mono",
        capturing ? "border-[rgba(245,158,11,0.3)]" : "border-[rgba(255,255,255,0.06)]",
      ].join(" ")}
      style={{ width: 140 }}
    />
  );
}

// ─── Collapsible section ─────────────────────────────────────────────────────

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  const [open, setOpen] = useState(true);
  return (
    <div className="flex flex-col">
      <button onClick={() => setOpen((o) => !o)} className="flex items-center gap-2 py-1.5 text-left">
        <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="rgba(255,255,255,0.25)" strokeWidth="2.5" strokeLinecap="round"
          className={`flex-shrink-0 transition-transform duration-200 ${open ? "rotate-90" : ""}`}><path d="M9 18l6-6-6-6" /></svg>
        <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] font-semibold">{label}</span>
      </button>
      {open && <div className="flex flex-col gap-2.5 mt-1.5 ml-4">{children}</div>}
    </div>
  );
}

// ─── Hotkey row (simple) ─────────────────────────────────────────────────────

function HotkeyRow({ label, hint, value, onChange }: {
  label: string; hint?: string; value: string; onChange: (v: string) => void;
}) {
  return (
    <div className="flex items-center gap-3">
      <span className="text-[11px] text-[rgba(255,255,255,0.4)] min-w-[120px]">
        {label} {hint && <span className="text-[rgba(255,255,255,0.15)]">{hint}</span>}
      </span>
      <HotkeyInput value={value} onChange={onChange} />
    </div>
  );
}

// ─── Note shortcut row (hotkey + notebook selector) ──────────────────────────

function NoteShortcutRow({ label, hotkeyValue, notebookId, notebooks, excludeIds, onHotkeyChange, onNotebookChange }: {
  label: string;
  hotkeyValue: string;
  notebookId: number | undefined;
  notebooks: Container[];
  excludeIds: number[]; // IDs already bound to OTHER shortcuts
  onHotkeyChange: (v: string) => void;
  onNotebookChange: (id: number) => void;
}) {
  // Filter out Quick Note + notebooks already bound to other shortcuts
  const selectable = notebooks.filter((nb) =>
    nb.title !== "Quick Note" &&
    (nb.id === notebookId || !excludeIds.includes(nb.id))
  );
  const boundNb = notebooks.find((nb) => nb.id === notebookId);

  return (
    <div className="flex items-center gap-3">
      <span className="text-[11px] text-[rgba(255,255,255,0.4)] min-w-[120px]">{label}</span>
      <HotkeyInput value={hotkeyValue} onChange={onHotkeyChange} />
      <select
        value={notebookId ?? 0}
        onChange={(e) => onNotebookChange(parseInt(e.target.value, 10))}
        className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2 py-1.5 text-[11px] text-[#fafaf9] focus:outline-none focus:border-[rgba(245,158,11,0.3)] cursor-pointer appearance-none flex-1 min-w-[100px]"
      >
        <option value={0}>— Select notebook —</option>
        {selectable.map((nb) => (
          <option key={nb.id} value={nb.id}>{nb.title}</option>
        ))}
      </select>
      {boundNb && (
        <span className="text-[9px] text-[rgba(255,255,255,0.2)] flex-shrink-0 inline-flex items-center gap-0.5"><NotebookIcon size={9} /> {boundNb.title}</span>
      )}
    </div>
  );
}

// ─── HotkeysTab ──────────────────────────────────────────────────────────────

export default function HotkeysTab({ config, onSave }: {
  config: AppConfig; onSave: (updates: Partial<AppConfig>) => void;
}) {
  const [notebooks, setNotebooks] = useState<Container[]>([]);
  const [modes, setModes] = useState<ModeEntry[]>([]);

  useEffect(() => {
    listContainers()
      .then((all) => setNotebooks(all.filter((c) => c.container_type === "notebook")))
      .catch(() => {});
    listModes().then(setModes).catch(() => {});
  }, []);

  return (
    <div className="flex flex-col gap-4">

      <Section label="Dictation">
        <HotkeyRow label="Dictation" hint="(hold to talk)" value={config.hotkey_dictation} onChange={(v) => onSave({ hotkey_dictation: v })} />
        <HotkeyRow label="Dictation Toggle" hint="(press to start/stop)" value={config.hotkey_dictation_toggle ?? ""} onChange={(v) => onSave({ hotkey_dictation_toggle: v })} />
        <HotkeyRow label="TTS" value={config.hotkey_tts} onChange={(v) => onSave({ hotkey_tts: v })} />
      </Section>

      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      <Section label="Agent">
        <HotkeyRow label="Agent speak" hint="(hold-to-talk)" value={config.hotkey_agent ?? "cmd+shift+a"} onChange={(v) => onSave({ hotkey_agent: v })} />
        <HotkeyRow label="Agent panel" hint="(toggle)" value={config.hotkey_agent_panel ?? "cmd+shift+g"} onChange={(v) => onSave({ hotkey_agent_panel: v })} />
      </Section>

      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      <Section label="Notes">
        <HotkeyRow label="Note panel" hint="(hold-to-talk)" value={config.hotkey_note ?? "option+n"} onChange={(v) => onSave({ hotkey_note: v })} />

        <div className="mt-1 mb-0.5">
          <span className="text-[9px] text-[rgba(255,255,255,0.2)]">
            Notebook shortcuts — bind a hotkey to directly record into a specific notebook
          </span>
        </div>

        <NoteShortcutRow
          label="Shortcut 1"
          hotkeyValue={config.hotkey_note_1 ?? ""}
          notebookId={config.notebook_hotkey_1}
          notebooks={notebooks}
          excludeIds={[config.notebook_hotkey_2, config.notebook_hotkey_3].filter(Boolean) as number[]}
          onHotkeyChange={(v) => onSave({ hotkey_note_1: v })}
          onNotebookChange={(id) => onSave({ notebook_hotkey_1: id || undefined })}
        />
        <NoteShortcutRow
          label="Shortcut 2"
          hotkeyValue={config.hotkey_note_2 ?? ""}
          notebookId={config.notebook_hotkey_2}
          notebooks={notebooks}
          excludeIds={[config.notebook_hotkey_1, config.notebook_hotkey_3].filter(Boolean) as number[]}
          onHotkeyChange={(v) => onSave({ hotkey_note_2: v })}
          onNotebookChange={(id) => onSave({ notebook_hotkey_2: id || undefined })}
        />
        <NoteShortcutRow
          label="Shortcut 3"
          hotkeyValue={config.hotkey_note_3 ?? ""}
          notebookId={config.notebook_hotkey_3}
          notebooks={notebooks}
          excludeIds={[config.notebook_hotkey_1, config.notebook_hotkey_2].filter(Boolean) as number[]}
          onHotkeyChange={(v) => onSave({ hotkey_note_3: v })}
          onNotebookChange={(id) => onSave({ notebook_hotkey_3: id || undefined })}
        />
      </Section>

      {config.hotkey_meeting && (
        <>
          <div className="border-t border-[rgba(255,255,255,0.04)]" />
          <Section label="Meeting">
            <HotkeyRow label="Meeting" hint="(toggle)" value={config.hotkey_meeting} onChange={(v) => onSave({ hotkey_meeting: v })} />
          </Section>

          <Section label="Quick Transform">
            <div className="flex items-center gap-3">
              <span className="text-[11px] text-[rgba(255,255,255,0.4)] min-w-[120px]">Transform</span>
              <HotkeyInput value={config.hotkey_transform ?? ""} onChange={(v) => onSave({ hotkey_transform: v })} />
              <select
                value={config.transform_mode ?? "polish"}
                onChange={(e) => onSave({ transform_mode: e.target.value })}
                className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2 py-1.5 text-[11px] text-[#fafaf9] focus:outline-none focus:border-[rgba(245,158,11,0.3)] cursor-pointer appearance-none flex-1 min-w-[100px]"
              >
                {modes.filter((m) => m.system || m.user_template).map((m) => (
                  <option key={m.id} value={m.id}>{m.icon} {m.name}</option>
                ))}
              </select>
            </div>
            <div className="text-[9px] text-[rgba(255,255,255,0.15)] mt-1 pl-[132px]">
              Select text → press hotkey → mode's LLM step processes it → result replaces selection
            </div>
          </Section>
        </>
      )}
    </div>
  );
}
