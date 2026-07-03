// Speech settings — voice-output pipelines. Currently the Listen queue
// (issue #23); the STS conversation profiles (issue #24) land here too, so
// all speech-output configuration lives on one tab.

import type { AppConfig, ModeEntry, ModelProfile } from "../../types";
import { HotkeyInput } from "./HotkeysTab";

export default function SpeechTab({
  config,
  modes,
  onSave,
}: {
  config: AppConfig;
  modes: ModeEntry[];
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  const profiles = (config.model_profiles ?? []) as ModelProfile[];
  const ttsProfiles = profiles.filter((p) => p.capabilities?.includes("tts"));
  const llmModes = modes.filter((m) => m.system || m.user_template);

  const select =
    "bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]";

  return (
    <div className="flex flex-col gap-5">
      {/* ── Listen queue ── */}
      <div className="flex flex-col gap-3">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">Listen queue</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            Select text anywhere, press the hotkey — it's summarized, synthesized,
            and lands in History › Listen as a playable item.
          </div>
        </div>

        <div className="flex flex-col gap-1">
          <label className="text-[10px] text-[rgba(255,255,255,0.35)]">Capture hotkey</label>
          <HotkeyInput
            value={config.hotkey_listen ?? "option+l"}
            onChange={(v) => onSave({ hotkey_listen: v })}
          />
        </div>

        <div className="flex flex-col gap-1">
          <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
            Processing mode
            <span className="ml-1 text-[rgba(255,255,255,0.15)]">
              (how captured text is rewritten before synthesis)
            </span>
          </label>
          <select
            value={config.listen_mode ?? "listen"}
            onChange={(e) => onSave({ listen_mode: e.target.value })}
            className={select}
          >
            {llmModes.map((m) => (
              <option key={m.id} value={m.id}>
                {m.icon} {m.name}
              </option>
            ))}
          </select>
        </div>

        <div className="flex flex-col gap-1">
          <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
            Voice model
            <span className="ml-1 text-[rgba(255,255,255,0.15)]">(empty = default TTS profile)</span>
          </label>
          <select
            value={config.listen_voice_profile ?? ""}
            onChange={(e) => onSave({ listen_voice_profile: e.target.value })}
            className={select}
          >
            <option value="">Default TTS profile</option>
            {ttsProfiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </div>

        <div className="flex flex-col gap-1">
          <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
            Voice
            <span className="ml-1 text-[rgba(255,255,255,0.15)]">(provider voice id, e.g. "default")</span>
          </label>
          <input
            type="text"
            defaultValue={config.listen_voice ?? "default"}
            onBlur={(e) => {
              if (e.target.value !== (config.listen_voice ?? "default"))
                onSave({ listen_voice: e.target.value });
            }}
            spellCheck={false}
            className={select}
          />
        </div>
      </div>

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Conversation (speech-to-speech) ── */}
      <div className="flex flex-col gap-3">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">Conversation</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            Hold the hotkey and talk — your words go through speech recognition,
            the conversation LLM, and are spoken back. Memory lasts for the
            configured number of turns.
          </div>
        </div>

        <div className="flex flex-col gap-1">
          <label className="text-[10px] text-[rgba(255,255,255,0.35)]">Hold-to-talk hotkey</label>
          <HotkeyInput
            value={config.hotkey_sts ?? "option+s"}
            onChange={(v) => onSave({ hotkey_sts: v })}
          />
        </div>

        <div className="flex flex-col gap-1">
          <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
            Persona
            <span className="ml-1 text-[rgba(255,255,255,0.15)]">(system prompt — replies are spoken, keep them short)</span>
          </label>
          <textarea
            defaultValue={config.sts_persona ?? ""}
            onBlur={(e) => {
              if (e.target.value !== (config.sts_persona ?? "")) onSave({ sts_persona: e.target.value });
            }}
            rows={3}
            spellCheck={false}
            className={`${select} resize-y leading-relaxed`}
          />
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
              LLM
              <span className="ml-1 text-[rgba(255,255,255,0.15)]">(empty = default)</span>
            </label>
            <select
              value={config.sts_llm_profile ?? ""}
              onChange={(e) => onSave({ sts_llm_profile: e.target.value })}
              className={select}
            >
              <option value="">Default LLM profile</option>
              {profiles.filter((p) => p.capabilities?.includes("llm")).map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
              Voice model
              <span className="ml-1 text-[rgba(255,255,255,0.15)]">(empty = default)</span>
            </label>
            <select
              value={config.sts_voice_profile ?? ""}
              onChange={(e) => onSave({ sts_voice_profile: e.target.value })}
              className={select}
            >
              <option value="">Default TTS profile</option>
              {ttsProfiles.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-[10px] text-[rgba(255,255,255,0.35)]">Voice</label>
            <input
              type="text"
              defaultValue={config.sts_voice ?? "default"}
              onBlur={(e) => {
                if (e.target.value !== (config.sts_voice ?? "default")) onSave({ sts_voice: e.target.value });
              }}
              spellCheck={false}
              className={select}
            />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-[10px] text-[rgba(255,255,255,0.35)]">Memory (turns)</label>
            <input
              type="number"
              min={0}
              max={50}
              defaultValue={config.sts_max_turns ?? 8}
              onBlur={(e) => {
                const v = Math.max(0, Math.min(50, parseInt(e.target.value) || 0));
                if (v !== (config.sts_max_turns ?? 8)) onSave({ sts_max_turns: v });
              }}
              className={select}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
