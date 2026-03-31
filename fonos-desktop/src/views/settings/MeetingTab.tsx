// Meeting settings tab — audio source, STT model, LLM model, summary prompt, hotkey.
// Uses Tailwind classes (same as other settings tabs like AgentTab).

import type { AppConfig, ModelProfile } from "../../types";

const DEFAULT_SUMMARY_PROMPT =
  "You are a helpful meeting assistant. Summarize the meeting transcript below. " +
  "Include: key topics discussed, decisions made, and action items (as '- [ ] task' lines). " +
  "Be concise and structured.";

// ─── Section label (matches AgentTab) ─────────────────────────────────────────

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
      {children}
    </div>
  );
}

// ─── MeetingTab ───────────────────────────────────────────────────────────────

interface MeetingTabProps {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}

export default function MeetingTab({ config, onSave }: MeetingTabProps) {
  const llmProfiles: ModelProfile[] = (config.model_profiles ?? []).filter(
    (p) => p.capabilities && p.capabilities.includes("llm")
  );
  const sttProfiles: ModelProfile[] = (config.model_profiles ?? []).filter(
    (p) => p.capabilities && p.capabilities.includes("stt")
  );

  const summaryPrompt =
    config.meeting_summary_prompt ?? DEFAULT_SUMMARY_PROMPT;

  return (
    <div className="flex flex-col gap-4">

      {/* ── Audio Source ───────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>Audio Source</SectionLabel>
        <select
          value={config.meeting_audio_source ?? "auto"}
          onChange={(e) => onSave({ meeting_audio_source: e.target.value })}
          className="w-full rounded-lg px-3 py-2 text-[11px] text-[#fafaf9] cursor-pointer appearance-none focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
          style={{
            background: "rgba(255,255,255,0.03)",
            border: "1px solid rgba(255,255,255,0.06)",
          }}
        >
          <option value="auto">Auto (ScreenCaptureKit + mic)</option>
          <option value="mic_only">Mic only</option>
        </select>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          "Auto" captures system audio + microphone via ScreenCaptureKit (macOS 13+).
        </div>
      </div>

      {/* ── STT Model ──────────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>STT Model</SectionLabel>
        <select
          value={config.meeting_stt_profile ?? ""}
          onChange={(e) => onSave({ meeting_stt_profile: e.target.value })}
          className="w-full rounded-lg px-3 py-2 text-[11px] text-[#fafaf9] cursor-pointer appearance-none focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
          style={{
            background: "rgba(255,255,255,0.03)",
            border: "1px solid rgba(255,255,255,0.06)",
          }}
        >
          <option value="">Default (global STT profile)</option>
          {sttProfiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name} ({p.model})
            </option>
          ))}
        </select>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          STT model for transcribing meeting audio. Defaults to global STT profile if not set.
        </div>
      </div>

      {/* ── LLM Model for Summary ──────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>LLM Model for Summary</SectionLabel>
        <select
          value={config.meeting_llm_profile ?? ""}
          onChange={(e) => onSave({ meeting_llm_profile: e.target.value })}
          className="w-full rounded-lg px-3 py-2 text-[11px] text-[#fafaf9] cursor-pointer appearance-none focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
          style={{
            background: "rgba(255,255,255,0.03)",
            border: "1px solid rgba(255,255,255,0.06)",
          }}
        >
          <option value="">— none (no summary) —</option>
          {llmProfiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
              {p.provider === "openrouter" ? " (OpenRouter)" : ""}
            </option>
          ))}
        </select>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          LLM used to generate meeting summaries. OpenRouter models work well for long contexts.
        </div>
      </div>

      {/* ── Summary Prompt ─────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>Summary Prompt</SectionLabel>
        <textarea
          rows={4}
          value={summaryPrompt}
          onChange={(e) => onSave({ meeting_summary_prompt: e.target.value })}
          className="rounded-lg px-3 py-2 text-[11px] text-[#fafaf9] leading-relaxed resize-none font-mono focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
          style={{
            background: "rgba(255,255,255,0.03)",
            border: "1px solid rgba(255,255,255,0.06)",
          }}
          placeholder={DEFAULT_SUMMARY_PROMPT}
        />
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          System prompt sent to the LLM when generating a meeting summary.
          Use "- [ ] task" format to create action items.
        </div>
      </div>

      {/* ── Hotkey (display only) ──────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>Hotkey</SectionLabel>
        <div
          className="rounded-lg px-3 py-2 text-[11px] text-[rgba(255,255,255,0.4)]"
          style={{
            background: "rgba(255,255,255,0.02)",
            border: "1px solid rgba(255,255,255,0.04)",
          }}
        >
          {config.hotkey_meeting
            ? <span className="font-mono">{config.hotkey_meeting}</span>
            : <span className="italic text-[rgba(255,255,255,0.2)]">Not set — configure in Hotkeys tab</span>
          }
        </div>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          The meeting start/stop hotkey is configured in the Hotkeys tab.
        </div>
      </div>

    </div>
  );
}
