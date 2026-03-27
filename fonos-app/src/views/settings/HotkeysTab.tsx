// Hotkeys tab — hotkey capture inputs for dictation and TTS.

import { useState, useCallback } from "react";
import type { AppConfig } from "../../types";

// ─── Hotkey capture ───────────────────────────────────────────────────────────

function HotkeyInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const [capturing, setCapturing] = useState<boolean>(false);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
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
      if (parts.length > 1) {
        onChange(parts.join("+"));
        setCapturing(false);
      }
    },
    [capturing, onChange]
  );

  return (
    <div className="flex gap-2 items-center">
      <input
        type="text"
        value={capturing ? "Press hotkey..." : value}
        readOnly
        onFocus={() => setCapturing(true)}
        onBlur={() => setCapturing(false)}
        onKeyDown={handleKeyDown}
        placeholder="hotkey_dictation"
        className={[
          "flex-1 bg-[rgba(255,255,255,0.03)] border rounded-lg px-3 py-2 text-white text-sm focus:outline-none font-mono text-xs",
          capturing
            ? "border-[rgba(245,158,11,0.3)]"
            : "border-[rgba(255,255,255,0.06)] focus:border-[rgba(245,158,11,0.3)]",
        ].join(" ")}
      />
    </div>
  );
}

// ─── HotkeysTab ──────────────────────────────────────────────────────────────

export default function HotkeysTab({
  config,
  onSave,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-2">
        <label className="text-[rgba(255,255,255,0.4)] text-[11px]">
          Dictation hotkey
        </label>
        <HotkeyInput
          value={config.hotkey_dictation}
          onChange={(v) => onSave({ hotkey_dictation: v })}
        />
      </div>
      <div className="flex flex-col gap-2">
        <label className="text-[rgba(255,255,255,0.4)] text-[11px]">
          TTS hotkey
        </label>
        <HotkeyInput
          value={config.hotkey_tts}
          onChange={(v) => onSave({ hotkey_tts: v })}
        />
      </div>
      <div className="mx-0 my-1 border-t border-[rgba(255,255,255,0.04)]" />
      <div className="flex flex-col gap-2">
        <label className="text-[rgba(255,255,255,0.4)] text-[11px]">
          Agent speak hotkey
          <span className="ml-1 text-[rgba(255,255,255,0.15)]">(press-to-talk)</span>
        </label>
        <HotkeyInput
          value={config.hotkey_agent ?? "cmd+shift+a"}
          onChange={(v) => onSave({ hotkey_agent: v })}
        />
      </div>
      <div className="flex flex-col gap-2">
        <label className="text-[rgba(255,255,255,0.4)] text-[11px]">
          Agent panel hotkey
          <span className="ml-1 text-[rgba(255,255,255,0.15)]">(view history)</span>
        </label>
        <HotkeyInput
          value={config.hotkey_agent_panel ?? "cmd+shift+g"}
          onChange={(v) => onSave({ hotkey_agent_panel: v })}
        />
      </div>
    </div>
  );
}
