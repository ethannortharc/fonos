// Speech settings — voice-output pipelines. Currently the Listen queue
// (issue #23); the STS conversation profiles (issue #24) land here too, so
// all speech-output configuration lives on one tab.

import { useEffect, useState } from "react";
import type { AppConfig, ModeEntry, ModelProfile, VoiceEntry } from "../../types";
import { listVoices, generateAndPlay } from "../../lib/api";
import { HotkeyInput } from "./HotkeysTab";

const PREVIEW_TEXT = "你好，这是这个音色的试听效果。Hello, this is a preview of this voice.";

/** Voice selector: cloned voices from the voice store + free-form entry for
 *  model speaker names, with an inline preview button. */
function VoicePicker({
  value,
  voices,
  onChange,
}: {
  value: string;
  voices: VoiceEntry[];
  onChange: (v: string) => void;
}) {
  const known = value === "default" || voices.some((v) => v.name === value);
  const [custom, setCustom] = useState(!known);
  const [previewing, setPreviewing] = useState(false);

  const preview = async () => {
    if (previewing) return;
    setPreviewing(true);
    try {
      await generateAndPlay(PREVIEW_TEXT, value || "default", 1.0);
    } catch {
      /* surfaced by backend logs; keep UI quiet */
    } finally {
      setPreviewing(false);
    }
  };

  const select =
    "flex-1 bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]";

  return (
    <div className="flex gap-1.5">
      {custom ? (
        <input
          type="text"
          defaultValue={value}
          placeholder="model speaker name, e.g. Cherry / af_heart"
          onBlur={(e) => {
            const v = e.target.value.trim() || "default";
            if (v !== value) onChange(v);
          }}
          spellCheck={false}
          className={select}
        />
      ) : (
        <select
          value={value}
          onChange={(e) => {
            if (e.target.value === "__custom__") setCustom(true);
            else onChange(e.target.value);
          }}
          className={select}
        >
          <option value="default">default (model's own)</option>
          {voices.map((v) => (
            <option key={v.voice_id} value={v.name}>
              🎙 {v.name} (cloned)
            </option>
          ))}
          <option value="__custom__">Custom name…</option>
        </select>
      )}
      {custom && (
        <button
          onClick={() => setCustom(false)}
          className="text-[9px] px-2 rounded-md bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] transition-colors"
          title="Back to list"
        >
          List
        </button>
      )}
      <button
        onClick={preview}
        disabled={previewing}
        className="text-[9px] px-2.5 rounded-md bg-[rgba(251,191,36,0.1)] text-[#fbbf24] hover:bg-[rgba(251,191,36,0.18)] disabled:opacity-40 transition-colors"
        title="Synthesize a short sample with this voice"
      >
        {previewing ? "…" : "▶ Preview"}
      </button>
    </div>
  );
}

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
  const [voices, setVoices] = useState<VoiceEntry[]>([]);
  useEffect(() => {
    listVoices()
      .then((l) => setVoices(l.voices.filter((v) => v.status === "ready")))
      .catch(() => {});
  }, []);

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
            <span className="ml-1 text-[rgba(255,255,255,0.15)]">(cloned voice or model speaker)</span>
          </label>
          <VoicePicker
            value={config.listen_voice ?? "default"}
            voices={voices}
            onChange={(v) => onSave({ listen_voice: v })}
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
          <div className="flex flex-col gap-1 col-span-2">
            <label className="text-[10px] text-[rgba(255,255,255,0.35)]">Voice</label>
            <VoicePicker
              value={config.sts_voice ?? "default"}
              voices={voices}
              onChange={(v) => onSave({ sts_voice: v })}
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
