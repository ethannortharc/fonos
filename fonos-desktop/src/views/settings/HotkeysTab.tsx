// Hotkeys tab — organized into sections with notebook selector for note shortcuts.

import { t, useT } from "../../lib/i18n";
import type { AppConfig } from "../../types";
import { HotkeyRow, Section } from "../../components/HotkeyInput";

// Re-exported so WorkflowsTab (which imports HotkeyInput from "./HotkeysTab")
// keeps resolving until it's updated to import from the shared component
// directly.
export { HotkeyInput } from "../../components/HotkeyInput";

// ─── HotkeysTab ──────────────────────────────────────────────────────────────

export default function HotkeysTab({ config, onSave }: {
  config: AppConfig; onSave: (updates: Partial<AppConfig>) => void;
}) {
  useT();

  return (
    <div className="flex flex-col gap-4">

      {/* Workflow triggers now live on the Workflows page (read-only hint). */}
      <div className="text-[10px] text-[rgba(255,255,255,0.3)] leading-relaxed">
        {t("hotkeys.workflow-hint")}
      </div>

      <Section label={t("hotkeys.section.agent")}>
        <HotkeyRow label={t("hotkeys.agentspeak")} hint={t("hotkeys.holdtotalk")} value={config.hotkey_agent ?? "cmd+shift+a"} onChange={(v) => onSave({ hotkey_agent: v })} />
        <HotkeyRow label={t("hotkeys.agentpanel")} hint={t("hotkeys.toggle")} value={config.hotkey_agent_panel ?? "cmd+shift+g"} onChange={(v) => onSave({ hotkey_agent_panel: v })} />
      </Section>

      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      <Section label={t("hotkeys.section.speech")}>
        <HotkeyRow label="TTS" value={config.hotkey_tts} onChange={(v) => onSave({ hotkey_tts: v })} />
      </Section>

      {config.hotkey_meeting && (
        <>
          <div className="border-t border-[rgba(255,255,255,0.04)]" />
          <Section label={t("hotkeys.section.meeting")}>
            <HotkeyRow label={t("hotkeys.meeting")} hint={t("hotkeys.toggle")} value={config.hotkey_meeting} onChange={(v) => onSave({ hotkey_meeting: v })} />
          </Section>
        </>
      )}
    </div>
  );
}
