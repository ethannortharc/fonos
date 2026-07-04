// Speech settings — voice-output pipelines: the Listen queue (issue #23) and
// the STS conversation (issue #24). Layout follows the settings conventions:
// section header + description, aligned label/control rows, compact controls.

import { useEffect, useState } from "react";
import type { AppConfig, ModeEntry, ModelProfile, VoiceEntry } from "../../types";
import { listVoices, listModelVoices, generateAndPlay } from "../../lib/api";
import { HotkeyInput } from "./HotkeysTab";
import { t, useT } from "../../lib/i18n";

const PREVIEW_TEXT = "你好，这是这个音色的试听效果。Hello, this is a preview of this voice.";

const control =
  "bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-1.5 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] transition-colors";

/** Known built-in speakers per TTS model family (no discovery endpoint in
 *  OMLX yet — curated from the model configs). */
function modelSpeakers(model: string): { group: string; names: string[] }[] {
  const m = model.toLowerCase();
  if (m.includes("customvoice"))
    return [
      {
        group: "Qwen3-TTS speakers",
        names: ["vivian", "serena", "ryan", "aiden", "uncle_fu", "ono_anna", "sohee", "eric", "dylan"],
      },
    ];
  if (m.includes("kokoro"))
    return [
      {
        group: "Kokoro · 中文",
        names: ["zf_xiaoxiao", "zf_xiaobei", "zf_xiaoni", "zf_xiaoyi", "zm_yunjian", "zm_yunxi", "zm_yunxia", "zm_yunyang"],
      },
      {
        group: "Kokoro · English",
        names: ["af_heart", "af_bella", "af_nicole", "af_sky", "am_adam", "am_michael", "am_puck", "bf_emma", "bm_george"],
      },
    ];
  return [];
}

/** One aligned settings row: fixed label column + control column. */
function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[130px_1fr] gap-x-3 items-start">
      <div className="pt-1.5 text-right">
        <div className="text-[10.5px] text-[rgba(255,255,255,0.45)] leading-tight">{label}</div>
        {hint && <div className="text-[8.5px] text-[rgba(255,255,255,0.18)] leading-tight mt-0.5">{hint}</div>}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

function SectionHeader({ icon, title, desc }: { icon: string; title: string; desc: string }) {
  return (
    <div className="flex items-start gap-2.5 mb-1">
      <div className="w-7 h-7 rounded-lg bg-[rgba(251,191,36,0.07)] flex items-center justify-center text-[13px] shrink-0">
        {icon}
      </div>
      <div>
        <div className="text-[12px] font-medium text-[#fafaf9]">{title}</div>
        <div className="text-[10px] text-[rgba(255,255,255,0.3)] leading-snug">{desc}</div>
      </div>
    </div>
  );
}

/** Voice selector: cloned voices + built-in model speakers + free-form,
 *  with inline preview. */
function VoicePicker({
  value,
  voices,
  modelName,
  serverVoices,
  onChange,
}: {
  value: string;
  voices: VoiceEntry[];
  modelName: string;
  serverVoices: string[];
  onChange: (v: string) => void;
}) {
  // Server-reported speakers (OMLX voices endpoint) beat the curated lists.
  const speakerGroups =
    serverVoices.length > 0
      ? [{ group: t("speech.voices.model"), names: serverVoices }]
      : modelSpeakers(modelName);
  const known =
    value === "default" ||
    voices.some((v) => v.name === value) ||
    speakerGroups.some((g) => g.names.includes(value));
  const [custom, setCustom] = useState(!known);
  const [previewing, setPreviewing] = useState(false);

  const preview = async () => {
    if (previewing) return;
    setPreviewing(true);
    try {
      await generateAndPlay(PREVIEW_TEXT, value || "default", 1.0);
    } catch {
      /* backend logs the failure */
    } finally {
      setPreviewing(false);
    }
  };

  return (
    <div className="flex gap-1.5 items-center">
      {custom ? (
        <input
          type="text"
          defaultValue={value === "default" ? "" : value}
          placeholder={t("speech.voices.placeholder")}
          onBlur={(e) => {
            const v = e.target.value.trim() || "default";
            if (v !== value) onChange(v);
          }}
          spellCheck={false}
          className={`${control} w-44`}
        />
      ) : (
        <select
          value={value}
          onChange={(e) => {
            if (e.target.value === "__custom__") setCustom(true);
            else onChange(e.target.value);
          }}
          className={`${control} w-44`}
        >
          <option value="default">{t("common.default")}</option>
          {voices.length > 0 && (
            <optgroup label={t("speech.voices.cloned")}>
              {voices.map((v) => (
                <option key={v.voice_id} value={v.name}>
                  {v.name}
                </option>
              ))}
            </optgroup>
          )}
          {speakerGroups.map((g) => (
            <optgroup key={g.group} label={g.group}>
              {g.names.map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </optgroup>
          ))}
          <option value="__custom__">{t("common.custom")}</option>
        </select>
      )}
      {custom && (
        <button
          onClick={() => setCustom(false)}
          className="text-[9px] px-2 py-1.5 rounded-md bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] transition-colors"
        >
          {t("common.list")}
        </button>
      )}
      <button
        onClick={preview}
        disabled={previewing}
        title={t("speech.preview-title")}
        className="text-[9px] px-2.5 py-1.5 rounded-md bg-[rgba(251,191,36,0.08)] text-[#fbbf24] hover:bg-[rgba(251,191,36,0.16)] disabled:opacity-40 transition-colors shrink-0"
      >
        {previewing ? t("common.playing") : t("common.preview")}
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
  useT();
  const profiles = (config.model_profiles ?? []) as ModelProfile[];
  const ttsProfiles = profiles.filter((p) => p.capabilities?.includes("tts"));
  const llmProfiles = profiles.filter((p) => p.capabilities?.includes("llm"));
  const llmModes = modes.filter((m) => m.system || m.user_template);
  const [voices, setVoices] = useState<VoiceEntry[]>([]);
  const [serverVoices, setServerVoices] = useState<Record<string, string[]>>({});

  useEffect(() => {
    listVoices()
      .then((l) => setVoices(l.voices.filter((v) => v.status === "ready")))
      .catch(() => {});
  }, []);

  // Ask the server which speakers each selected voice profile's model has.
  const listenProfileId = config.listen_voice_profile ?? "";
  const stsProfileId = config.sts_voice_profile ?? "";
  useEffect(() => {
    for (const id of new Set([listenProfileId, stsProfileId])) {
      listModelVoices(id)
        .then((v) => setServerVoices((m) => ({ ...m, [id]: v })))
        .catch(() => {});
    }
  }, [listenProfileId, stsProfileId]);

  const effectiveTtsModel = (profileId: string) => {
    const p =
      profiles.find((x) => x.id === profileId) ??
      profiles.find((x) => x.id === config.tts_profile);
    return p?.model ?? "";
  };

  return (
    <div className="flex flex-col gap-5">
      {/* ── Listen queue ── */}
      <div className="flex flex-col gap-2.5">
        <SectionHeader
          icon="🎧"
          title={t("speech.listen.title")}
          desc={t("speech.listen.desc")}
        />
        <Row label={t("speech.listen.hotkey")}>
          <div className="max-w-[240px]">
            <HotkeyInput
              value={config.hotkey_listen ?? "option+l"}
              onChange={(v) => onSave({ hotkey_listen: v })}
            />
          </div>
        </Row>
        <Row label={t("speech.processing")} hint={t("speech.processing.hint")}>
          <select
            value={config.listen_mode ?? "listen"}
            onChange={(e) => onSave({ listen_mode: e.target.value })}
            className={`${control} w-44`}
          >
            {llmModes.map((m) => (
              <option key={m.id} value={m.id}>
                {m.icon} {m.name}
              </option>
            ))}
          </select>
        </Row>
        <Row label={t("speech.voicemodel")} hint={t("speech.voicemodel.hint")}>
          <select
            value={config.listen_voice_profile ?? ""}
            onChange={(e) => onSave({ listen_voice_profile: e.target.value })}
            className={`${control} w-44`}
          >
            <option value="">{t("speech.default-tts-profile")}</option>
            {ttsProfiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </Row>
        <Row label={t("speech.voice")}>
          <VoicePicker
            value={config.listen_voice ?? "default"}
            voices={voices}
            modelName={effectiveTtsModel(listenProfileId)}
            serverVoices={serverVoices[listenProfileId] ?? []}
            onChange={(v) => onSave({ listen_voice: v })}
          />
        </Row>
      </div>

      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Conversation ── */}
      <div className="flex flex-col gap-2.5">
        <SectionHeader
          icon="💬"
          title={t("speech.conv.title")}
          desc={t("speech.conv.desc")}
        />
        <Row label={t("speech.conv.hotkey")}>
          <div className="max-w-[240px]">
            <HotkeyInput
              value={config.hotkey_sts ?? "option+s"}
              onChange={(v) => onSave({ hotkey_sts: v })}
            />
          </div>
        </Row>
        <Row label={t("speech.persona")} hint={t("speech.persona.hint")}>
          <textarea
            defaultValue={config.sts_persona ?? ""}
            onBlur={(e) => {
              if (e.target.value !== (config.sts_persona ?? "")) onSave({ sts_persona: e.target.value });
            }}
            rows={3}
            spellCheck={false}
            className={`${control} w-full max-w-[420px] resize-y leading-relaxed`}
          />
        </Row>
        <Row label={t("speech.llm")} hint={t("speech.llm.hint")}>
          <select
            value={config.sts_llm_profile ?? ""}
            onChange={(e) => onSave({ sts_llm_profile: e.target.value })}
            className={`${control} w-44`}
          >
            <option value="">{t("speech.default-llm-profile")}</option>
            {llmProfiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </Row>
        <Row label={t("speech.voicemodel")} hint={t("speech.voicemodel.hint")}>
          <select
            value={config.sts_voice_profile ?? ""}
            onChange={(e) => onSave({ sts_voice_profile: e.target.value })}
            className={`${control} w-44`}
          >
            <option value="">{t("speech.default-tts-profile")}</option>
            {ttsProfiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </Row>
        <Row label={t("speech.voice")}>
          <VoicePicker
            value={config.sts_voice ?? "default"}
            voices={voices}
            modelName={effectiveTtsModel(stsProfileId)}
            serverVoices={serverVoices[stsProfileId] ?? []}
            onChange={(v) => onSave({ sts_voice: v })}
          />
        </Row>
        <Row label={t("speech.memory")} hint={t("speech.memory.hint")}>
          <input
            type="number"
            min={0}
            max={50}
            defaultValue={config.sts_max_turns ?? 8}
            onBlur={(e) => {
              const v = Math.max(0, Math.min(50, parseInt(e.target.value) || 0));
              if (v !== (config.sts_max_turns ?? 8)) onSave({ sts_max_turns: v });
            }}
            className={`${control} w-20`}
          />
        </Row>
        <Row label={t("speech.sensitivity")} hint={t("speech.sensitivity.hint")}>
          <SensitivityPicker
            value={config.call_vad_sensitivity ?? 0.5}
            onChange={(v) => onSave({ call_vad_sensitivity: v })}
          />
        </Row>
      </div>
    </div>
  );
}

/** Three-step Low / Medium / High selector for the call-mode VAD sensitivity
 *  (0.0–1.0). Buckets map Low → 0.25, Medium → 0.5, High → 0.8; the button
 *  nearest the stored value is highlighted. */
function SensitivityPicker({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  const steps = [
    { val: 0.25, label: t("speech.sensitivity.low") },
    { val: 0.5, label: t("speech.sensitivity.med") },
    { val: 0.8, label: t("speech.sensitivity.high") },
  ];
  const activeIdx = steps.reduce(
    (best, s, i) => (Math.abs(s.val - value) < Math.abs(steps[best].val - value) ? i : best),
    0,
  );
  return (
    <div className="inline-flex rounded-lg overflow-hidden border border-[rgba(255,255,255,0.06)]">
      {steps.map((s, i) => (
        <button
          key={s.val}
          onClick={() => onChange(s.val)}
          className={`px-3 py-1.5 text-[10.5px] transition-colors ${
            i > 0 ? "border-l border-[rgba(255,255,255,0.06)]" : ""
          } ${
            i === activeIdx
              ? "bg-[rgba(251,191,36,0.14)] text-[#fbbf24]"
              : "bg-[rgba(255,255,255,0.03)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)]"
          }`}
        >
          {s.label}
        </button>
      ))}
    </div>
  );
}
