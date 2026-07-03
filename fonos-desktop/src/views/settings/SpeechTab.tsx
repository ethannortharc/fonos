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

      {/* Divider — STS conversation profiles (issue #24) join this tab next */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />
      <div className="text-[10px] text-[rgba(255,255,255,0.2)] italic">
        Conversation profiles (speech-to-speech) will appear here.
      </div>
    </div>
  );
}
