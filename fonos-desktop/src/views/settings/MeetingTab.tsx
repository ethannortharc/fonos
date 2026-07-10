// Meeting settings tab — audio source, STT model, LLM model, summary prompt.
// Uses Tailwind classes (same as other settings tabs like AgentTab).

import type { AppConfig, ModelProfile } from "../../types";
import { t, useT } from "../../lib/i18n";

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
  useT();
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
        <SectionLabel>{t("mtab.audio-source")}</SectionLabel>
        <select
          value={config.meeting_audio_source ?? "auto"}
          onChange={(e) => onSave({ meeting_audio_source: e.target.value })}
          className="w-full rounded-lg px-3 py-2 text-[11px] text-[#fafaf9] cursor-pointer appearance-none focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
          style={{
            background: "rgba(255,255,255,0.03)",
            border: "1px solid rgba(255,255,255,0.06)",
          }}
        >
          <option value="auto">{t("mtab.audio-auto")}</option>
          <option value="mic_only">{t("mtab.audio-mic")}</option>
        </select>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          {t("mtab.audio-hint")}
        </div>
      </div>

      {/* ── STT Model ──────────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("mtab.stt-model")}</SectionLabel>
        <select
          value={config.meeting_stt_profile ?? ""}
          onChange={(e) => onSave({ meeting_stt_profile: e.target.value })}
          className="w-full rounded-lg px-3 py-2 text-[11px] text-[#fafaf9] cursor-pointer appearance-none focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
          style={{
            background: "rgba(255,255,255,0.03)",
            border: "1px solid rgba(255,255,255,0.06)",
          }}
        >
          <option value="">{t("mtab.stt-default")}</option>
          {sttProfiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name} ({p.model})
            </option>
          ))}
        </select>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          {t("mtab.stt-hint")}
        </div>
      </div>

      {/* ── LLM Model for Summary ──────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("mtab.llm-model")}</SectionLabel>
        <select
          value={config.meeting_llm_profile ?? ""}
          onChange={(e) => onSave({ meeting_llm_profile: e.target.value })}
          className="w-full rounded-lg px-3 py-2 text-[11px] text-[#fafaf9] cursor-pointer appearance-none focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
          style={{
            background: "rgba(255,255,255,0.03)",
            border: "1px solid rgba(255,255,255,0.06)",
          }}
        >
          <option value="">{t("mtab.llm-none")}</option>
          {llmProfiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
              {p.provider === "openrouter" ? " (OpenRouter)" : ""}
            </option>
          ))}
        </select>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          {t("mtab.llm-hint")}
        </div>
      </div>

      {/* ── Summary Prompt ─────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("mtab.summary-prompt")}</SectionLabel>
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
          {t("mtab.summary-hint")}
        </div>
      </div>

    </div>
  );
}
