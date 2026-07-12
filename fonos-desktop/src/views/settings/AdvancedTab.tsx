// AdvancedTab.tsx — the Advanced page (Flows UI redesign, Task 5). Absorbs the
// Speech / Insertion settings tabs plus their non-workflow hotkeys behind
// a segmented control, retiring the standalone Hotkeys tab (workflow trigger
// keys already live per-recipe in the Workbench's Recipes segment — see
// RecipesSection).
//
// Each sub-page = the feature's existing settings component, unchanged, with
// its hotkey row(s) relocated verbatim from HotkeysTab above it: same config
// field names, same i18n label/hint keys and defaults, so nothing user-facing
// is lost when HotkeysTab.tsx is deleted.
//
// The Agent segment (AgentTab + SkillsTab + the hotkey_agent/_panel rows) was
// retired by Workbench P2 Task 6: agent settings now live on the
// `agent.default` widget's own WidgetForm PropsForm case (llm_widget ref,
// inline system-prompt fallback, TTS toggle, voice fields, timeout — Fix
// Round 1 moved the system prompt here too and dropped the inert max_turns
// input; only the safety allow/blocklist stays global, config-backed with no
// settings-tab home of its own right now), and its two standalone hotkeys
// were folded into `wf.agent-voice`/`wf.agent`'s own Hotkey chips by
// `migrate_legacy_agent_triggers`.
//
// The Meeting segment (MeetingTab + the hotkey_meeting row) was retired the
// same way by Workbench P2 Task 7: meeting settings now live on the
// `meeting.default` widget's own WidgetForm PropsForm case (stt_widget +
// llm_widget refs, summary_prompt textarea — the dead `meeting_audio_source`
// field, never a real Rust config field, is simply gone), and its standalone
// hotkey was folded into `wf.meeting`'s own Hotkey chip by
// `migrate_legacy_meeting_triggers`.
//
// `modes`/`ModeEntry` are no longer threaded through to `SpeechTab` —
// Workbench P2 Task 10 removed its `listen_mode` picker (Listen now always
// resolves the built-in `llm.listen` widget).

import { useState } from "react";
import { t, useT } from "../../lib/i18n";
import type { AppConfig } from "../../types";
import { HotkeyRow } from "../../components/HotkeyInput";
import SpeechTab from "./SpeechTab";
import InsertionTab from "./InsertionTab";

type Sub = "speech" | "insertion";

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

export default function AdvancedTab({ config, onSave }: {
  config: AppConfig; onSave: (updates: Partial<AppConfig>) => void;
}) {
  useT();
  const [sub, setSub] = useState<Sub>("speech");

  return (
    <div className="flex flex-col">
      {/* Segmented control [Speech] [Insertion] */}
      <div className="inline-flex self-start bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] rounded-[9px] p-[3px] gap-[3px] mb-[18px]">
        <SegButton active={sub === "speech"} onClick={() => setSub("speech")}><span>{t("tab.speech")}</span></SegButton>
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
          <SpeechTab config={config} onSave={onSave} />
        </div>
      )}

      {/* ────────────── Insertion sub — per-app text-insertion overrides ── */}
      {sub === "insertion" && (
        <InsertionTab config={config} onSave={onSave} />
      )}
    </div>
  );
}
