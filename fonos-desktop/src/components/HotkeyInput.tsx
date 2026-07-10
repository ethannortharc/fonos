// Shared hotkey-capture primitives — extracted from HotkeysTab so other tabs
// (WorkflowsTab, and later FlowsTab/AdvancedTab) can reuse the same input,
// section, and row without importing a whole settings tab.

import { useState, useCallback, useRef } from "react";
import { t, useT } from "../lib/i18n";

// ─── Hotkey capture input ────────────────────────────────────────────────────

export function HotkeyInput({ value, onChange, placeholder }: {
  value: string; onChange: (v: string) => void; placeholder?: string;
}) {
  useT();
  const [capturing, setCapturing] = useState(false);
  const ref = useRef<HTMLInputElement>(null);

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
    if (parts.length > 1) {
      onChange(parts.join("+"));
      setCapturing(false);
      // Blur to allow re-clicking the same input
      ref.current?.blur();
    }
  }, [capturing, onChange]);

  return (
    <input
      ref={ref}
      type="text"
      value={capturing ? t("hotkeys.presskeys") : value}
      readOnly
      onClick={() => { setCapturing(true); ref.current?.focus(); }}
      onBlur={() => setCapturing(false)}
      onKeyDown={handleKeyDown}
      placeholder={placeholder ?? t("hotkeys.clicktoset")}
      className={[
        "bg-[rgba(255,255,255,0.03)] border rounded-lg px-2.5 py-1.5 text-[#fafaf9] text-[11px] focus:outline-none font-mono cursor-pointer",
        capturing ? "border-[rgba(242,184,75,0.3)]" : "border-[rgba(255,255,255,0.06)]",
      ].join(" ")}
      style={{ width: 132 }}
    />
  );
}

// ─── Collapsible section ─────────────────────────────────────────────────────

export function Section({ label, children }: { label: string; children: React.ReactNode }) {
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

export function HotkeyRow({ label, hint, value, onChange }: {
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
