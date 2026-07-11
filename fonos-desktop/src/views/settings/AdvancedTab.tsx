// AdvancedTab.tsx — the Advanced page (Flows UI redesign, Task 5). Absorbs the
// Speech / Agent / Meeting settings tabs plus their non-workflow hotkeys behind
// a segmented control, retiring the standalone Hotkeys tab (workflow trigger
// keys already live per-recipe in the Workbench's Recipes segment — see
// RecipesSection).
//
// Each sub-page = the feature's existing settings component, unchanged, with
// its hotkey row(s) relocated verbatim from HotkeysTab above it: same config
// field names, same i18n label/hint keys and defaults, so nothing user-facing
// is lost when HotkeysTab.tsx is deleted.

import { useState } from "react";
import { t, useT } from "../../lib/i18n";
import type { AppConfig, ModeEntry } from "../../types";
import { HotkeyRow } from "../../components/HotkeyInput";
import SpeechTab from "./SpeechTab";
import AgentTab from "./AgentTab";
import SkillsTab from "./SkillsTab";
import MeetingTab from "./MeetingTab";
import InsertionTab from "./InsertionTab";

type Sub = "speech" | "agent" | "meeting" | "insertion";

// ─── Segmented control — same markup/classes as Workbench.tsx's top segment
//     switcher (container + active-state pill), inlined here as its own
//     SegButton per YAGNI (see design doc §9: "各自内联; YAGNI 内联即可"). ──────

function SegButton({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className={[
        "flex items-center gap-1.5 text-[10.5px] font-medium px-4 py-[5px] rounded-md transition-colors",
        active ? "bg-[rgba(255,255,255,0.06)] text-[#fafaf9]" : "text-[rgba(255,255,255,0.32)] hover:text-[rgba(255,255,255,0.55)]",
      ].join(" ")}
    >
      {children}
    </button>
  );
}

// ─── Main AdvancedTab ────────────────────────────────────────────────────────

export default function AdvancedTab({ config, modes, onSave }: {
  config: AppConfig; modes: ModeEntry[]; onSave: (updates: Partial<AppConfig>) => void;
}) {
  useT();
  const [sub, setSub] = useState<Sub>("speech");

  return (
    <div className="flex flex-col">
      {/* Segmented control [Speech] [Agent] [Meeting] [Insertion] */}
      <div className="inline-flex self-start bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] rounded-[9px] p-[3px] gap-[3px] mb-[18px]">
        <SegButton active={sub === "speech"} onClick={() => setSub("speech")}><span>{t("tab.speech")}</span></SegButton>
        <SegButton active={sub === "agent"} onClick={() => setSub("agent")}><span>{t("tab.agent")}</span></SegButton>
        <SegButton active={sub === "meeting"} onClick={() => setSub("meeting")}><span>{t("tab.meeting")}</span></SegButton>
        <SegButton active={sub === "insertion"} onClick={() => setSub("insertion")}><span>{t("tab.insertion")}</span></SegButton>
      </div>

      {/* ────────────── Speech sub — hotkey_tts + SpeechTab ────────────── */}
      {sub === "speech" && (
        <div className="flex flex-col gap-4">
          <HotkeyRow
            label={t("hotkeys.speech")}
            value={config.hotkey_tts ?? ""}
            onChange={(v) => onSave({ hotkey_tts: v })}
          />
          <SpeechTab config={config} modes={modes} onSave={onSave} />
        </div>
      )}

      {/* ────────────── Agent sub — hotkey_agent/_panel + AgentTab + SkillsTab ── */}
      {sub === "agent" && (
        <div className="flex flex-col gap-4">
          <div className="flex flex-col gap-2.5">
            <HotkeyRow
              label={t("hotkeys.agentspeak")}
              hint={t("hotkeys.holdtotalk")}
              value={config.hotkey_agent ?? "cmd+shift+a"}
              onChange={(v) => onSave({ hotkey_agent: v })}
            />
            <HotkeyRow
              label={t("hotkeys.agentpanel")}
              hint={t("hotkeys.toggle")}
              value={config.hotkey_agent_panel ?? "cmd+shift+g"}
              onChange={(v) => onSave({ hotkey_agent_panel: v })}
            />
          </div>
          <AgentTab config={config} onSave={onSave} />
          <div style={{ borderTop: "1px solid rgba(255,255,255,0.04)", marginTop: 16, paddingTop: 8 }} />
          <SkillsTab />
        </div>
      )}

      {/* ────────────── Meeting sub — hotkey_meeting + MeetingTab ────────────── */}
      {sub === "meeting" && (
        <div className="flex flex-col gap-4">
          <HotkeyRow
            label={t("hotkeys.meeting")}
            hint={t("hotkeys.toggle")}
            value={config.hotkey_meeting ?? ""}
            onChange={(v) => onSave({ hotkey_meeting: v })}
          />
          <MeetingTab config={config} onSave={onSave} />
        </div>
      )}

      {/* ────────────── Insertion sub — per-app text-insertion overrides ── */}
      {sub === "insertion" && (
        <InsertionTab config={config} onSave={onSave} />
      )}
    </div>
  );
}
