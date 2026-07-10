// Agent settings tab — LLM model, system prompt, safety rules, execution, TTS toggle.

import { useState, KeyboardEvent } from "react";
import type { AppConfig, ModelProfile } from "../../types";
import { t, useT } from "../../lib/i18n";
import { selectClass } from "./constants";

// ─── Section label style (shared settings-tab look) ──────────────────────────

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
      {children}
    </div>
  );
}

// ─── Chip Component ───────────────────────────────────────────────────────────

function Chip({
  value,
  color,
  onRemove,
}: {
  value: string;
  color: "green" | "red";
  onRemove: () => void;
}) {
  const colorStyle =
    color === "green"
      ? {
          bg: "rgba(134,239,172,0.06)",
          text: "rgba(134,239,172,0.6)",
          xText: "rgba(134,239,172,0.3)",
          xHover: "rgba(134,239,172,0.6)",
        }
      : {
          bg: "rgba(239,68,68,0.06)",
          text: "rgba(239,68,68,0.6)",
          xText: "rgba(239,68,68,0.3)",
          xHover: "rgba(239,68,68,0.6)",
        };

  return (
    <span
      className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[9px]"
      style={{ background: colorStyle.bg, color: colorStyle.text }}
    >
      {value}
      <button
        onClick={onRemove}
        className="ml-0.5 transition-colors"
        style={{ color: colorStyle.xText }}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLButtonElement).style.color = colorStyle.xHover;
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.color = colorStyle.xText;
        }}
        aria-label={`${t("agent.remove")} ${value}`}
      >
        &#10005;
      </button>
    </span>
  );
}

// ─── Chip List Editor ─────────────────────────────────────────────────────────

function ChipListEditor({
  label,
  color,
  values,
  onChange,
}: {
  label: string;
  color: "green" | "red";
  values: string[];
  onChange: (next: string[]) => void;
}) {
  const [input, setInput] = useState("");

  const labelColor =
    color === "green" ? "rgba(134,239,172,0.5)" : "rgba(239,68,68,0.5)";

  const addItem = () => {
    const trimmed = input.trim();
    if (trimmed && !values.includes(trimmed)) {
      onChange([...values, trimmed]);
    }
    setInput("");
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      addItem();
    }
  };

  const removeItem = (item: string) => {
    onChange(values.filter((v) => v !== item));
  };

  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-[10px]" style={{ color: labelColor }}>
        {label}
      </div>
      <div className="flex flex-wrap gap-1">
        {values.map((v) => (
          <Chip key={v} value={v} color={color} onRemove={() => removeItem(v)} />
        ))}
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          onBlur={addItem}
          placeholder={t("agent.add")}
          className="px-2 py-0.5 rounded text-[9px] text-[#fafaf9] w-14 focus:outline-none focus:border-[rgba(255,255,255,0.1)]"
          style={{
            background: "rgba(255,255,255,0.02)",
            border: "1px solid rgba(255,255,255,0.04)",
          }}
        />
      </div>
    </div>
  );
}

// ─── Slider Component (matches Voice.tsx speed slider style) ─────────────────

function TimeoutSlider({
  value,
  onChange,
}: {
  value: number;
  onChange: (v: number) => void;
}) {
  const min = 5;
  const max = 120;
  const percent = ((value - min) / (max - min)) * 100;

  return (
    <div className="flex items-center gap-2.5">
      <span className="text-[10px] text-[rgba(255,255,255,0.3)] w-16">{t("agent.timeout")}</span>
      <div className="flex-1 h-1 bg-[rgba(255,255,255,0.06)] rounded-full relative cursor-pointer">
        {/* Filled portion */}
        <div
          className="absolute left-0 top-0 h-full rounded-full bg-gradient-to-r from-[rgba(242,184,75,0.3)] to-[var(--accent)]"
          style={{ width: `${percent}%` }}
        />
        {/* Knob */}
        <div
          className="absolute top-1/2 -translate-y-1/2 w-3 h-3 rounded-full bg-[var(--accent)] shadow-[0_2px_6px_rgba(242,184,75,0.3)]"
          style={{ left: `${percent}%`, marginLeft: "-6px" }}
        />
        {/* Hidden native range for interaction */}
        <input
          type="range"
          min={min}
          max={max}
          step={1}
          value={value}
          onChange={(e) => onChange(parseInt(e.target.value, 10))}
          className="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
        />
      </div>
      <span className="text-[10px] text-[rgba(255,255,255,0.4)] w-8 text-right font-mono">
        {value}s
      </span>
    </div>
  );
}

// ─── AgentTab ─────────────────────────────────────────────────────────────────

interface AgentTabProps {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}

export default function AgentTab({ config, onSave }: AgentTabProps) {
  useT();
  // Model profiles by capability
  const llmProfiles: ModelProfile[] = (config.model_profiles ?? []).filter(
    (p) => p.capabilities && p.capabilities.includes("llm")
  );
  const sttProfiles: ModelProfile[] = (config.model_profiles ?? []).filter(
    (p) => p.capabilities && p.capabilities.includes("stt")
  );

  // Execution state (local sliders / inputs before blur/commit)
  const [timeoutSecs, setTimeoutSecs] = useState<number>(
    config.agent_timeout_secs ?? 30
  );
  const [maxTurns, setMaxTurns] = useState<number>(
    config.agent_max_turns ?? 20
  );

  return (
    <div className="flex flex-col gap-4">

      {/* ── LLM Model ─────────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("agent.llm-model")}</SectionLabel>
        <select
          value={config.agent_llm_profile ?? ""}
          onChange={(e) => onSave({ agent_llm_profile: e.target.value })}
          className={selectClass}
        >
          <option value="">{t("agent.none")}</option>
          {llmProfiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          {t("agent.llm-hint")}
        </div>
      </div>

      {/* ── STT Model ──────────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("agent.stt-model")}</SectionLabel>
        <select
          value={config.agent_stt_profile ?? ""}
          onChange={(e) => onSave({ agent_stt_profile: e.target.value })}
          className={selectClass}
        >
          <option value="">{t("agent.stt-default")}</option>
          {sttProfiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name} ({p.model})
            </option>
          ))}
        </select>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          {t("agent.stt-hint")}
        </div>
      </div>

      {/* ── System Prompt ─────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("agent.system-prompt")}</SectionLabel>
        <textarea
          rows={4}
          value={config.agent_system_prompt ?? ""}
          onChange={(e) => onSave({ agent_system_prompt: e.target.value })}
          className="rounded-lg px-3 py-2 text-[11px] text-[#fafaf9] leading-relaxed resize-none font-mono focus:outline-none focus:border-[rgba(242,184,75,0.3)]"
          style={{
            background: "rgba(255,255,255,0.03)",
            border: "1px solid rgba(255,255,255,0.06)",
          }}
          placeholder={t("agent.prompt-ph")}
        />
      </div>

      {/* ── Safety Rules ──────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("agent.safety")}</SectionLabel>
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic mb-0.5">
          {t("agent.safety-hint")}
        </div>

        {/* Allowed commands — green chips */}
        <ChipListEditor
          label={t("agent.allowed")}
          color="green"
          values={config.agent_safety_allowlist ?? []}
          onChange={(next) => onSave({ agent_safety_allowlist: next })}
        />

        {/* Blocked patterns — red chips */}
        <div className="mt-1">
          <ChipListEditor
            label={t("agent.blocked")}
            color="red"
            values={config.agent_safety_blocklist ?? []}
            onChange={(next) => onSave({ agent_safety_blocklist: next })}
          />
        </div>
      </div>

      {/* ── Execution ─────────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("agent.execution")}</SectionLabel>

        {/* Timeout slider */}
        <TimeoutSlider
          value={timeoutSecs}
          onChange={(v) => {
            setTimeoutSecs(v);
            onSave({ agent_timeout_secs: v });
          }}
        />

        {/* Max turns */}
        <div className="flex items-center gap-2.5">
          <span className="text-[10px] text-[rgba(255,255,255,0.3)] w-16">{t("agent.max-turns")}</span>
          <input
            type="number"
            min={1}
            max={100}
            value={maxTurns}
            onChange={(e) => {
              const v = parseInt(e.target.value, 10);
              if (!isNaN(v) && v > 0) {
                setMaxTurns(v);
                onSave({ agent_max_turns: v });
              }
            }}
            className="rounded-lg px-3 py-1.5 text-[11px] text-[#fafaf9] w-16 focus:outline-none focus:border-[rgba(242,184,75,0.3)]"
            style={{
              background: "rgba(255,255,255,0.03)",
              border: "1px solid rgba(255,255,255,0.06)",
            }}
          />
        </div>
      </div>

      {/* ── Response ──────────────────────────────────────────────────────── */}
      <div className="flex flex-col gap-2">
        <SectionLabel>{t("agent.response")}</SectionLabel>
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={config.agent_tts_enabled ?? false}
            onChange={(e) => onSave({ agent_tts_enabled: e.target.checked })}
            className="accent-[var(--accent)]"
          />
          <span className="text-[12px] text-[rgba(255,255,255,0.5)]">
            {t("agent.tts")}
          </span>
        </label>
      </div>

    </div>
  );
}
