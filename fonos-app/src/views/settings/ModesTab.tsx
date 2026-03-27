// Modes tab — pipeline view, mode list, mode edit form.

import { useState } from "react";
import type { AppConfig, ModeEntry, ModelProfile } from "../../types";
import { EMPTY_MODE } from "./constants";
import type { ModeForm } from "./constants";

// ─── Pipeline Step Component ─────────────────────────────────────────────────

function PipelineStep({
  dotColor,
  label,
  isLast,
  children,
}: {
  dotColor: string;
  label: string;
  isLast: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="flex">
      {/* Left rail: dot + line */}
      <div className="flex flex-col items-center mr-3 flex-shrink-0" style={{ width: 8 }}>
        <div
          className="rounded-full flex-shrink-0"
          style={{ width: 8, height: 8, backgroundColor: dotColor, marginTop: 2 }}
        />
        {!isLast && (
          <div
            className="flex-1"
            style={{ width: 1, backgroundColor: "rgba(255,255,255,0.08)", minHeight: 16 }}
          />
        )}
      </div>
      {/* Right content */}
      <div className="flex-1 pb-3">
        <div className="text-[9px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] mb-1">
          {label}
        </div>
        {children}
      </div>
    </div>
  );
}

// ─── Model Selector Dropdown ─────────────────────────────────────────────────

function ModelSelector({
  capKey,
  value,
  profiles,
  allowDefault,
  onChange,
}: {
  capKey: string;
  value: string;
  profiles: ModelProfile[];
  allowDefault: boolean;
  onChange: (v: string) => void;
}) {
  const filtered = profiles.filter(
    (p) => p.capabilities && p.capabilities.includes(capKey)
  );
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2 py-1.5 text-[11px] text-[#fafaf9] focus:outline-none focus:border-[rgba(245,158,11,0.3)] cursor-pointer appearance-none"
      style={{ backgroundImage: "none" }}
    >
      {allowDefault && <option value="">Use default</option>}
      {capKey === "stt" && <option value="apple-speech">Apple Speech (on-device)</option>}
      {filtered.map((p) => (
        <option key={p.id} value={p.id}>
          {p.name} ({p.model})
        </option>
      ))}
      {filtered.length === 0 && !((capKey === "stt")) && (
        <option disabled value="">No {capKey.toUpperCase()} models</option>
      )}
    </select>
  );
}

// ─── Mode Pipeline Card ──────────────────────────────────────────────────────

function ModePipelineCard({
  mode,
  config,
  profiles,
  expanded,
  onToggle,
  onEdit,
  onDelete,
  onSaveMode,
}: {
  mode: ModeEntry;
  config: AppConfig;
  profiles: ModelProfile[];
  expanded: boolean;
  onToggle: () => void;
  onEdit: (form: ModeForm) => void;
  onDelete: (id: string) => void;
  onSaveMode: (form: ModeForm) => Promise<void>;
}) {
  const hasLlm = !!(mode.system || mode.user_template);
  const defaultStt = profiles.find((p) => p.id === config.stt_profile);
  const defaultLlm = profiles.find((p) => p.id === config.llm_profile);
  const llmOverridden = mode.builtin && !!mode.model;
  const sttOverridden = !!mode.stt_model;

  const sttModelName = mode.stt_model === "apple-speech"
    ? "Apple Speech"
    : mode.stt_model
      ? (profiles.find((p) => p.id === mode.stt_model)?.name ?? mode.stt_model)
      : (defaultStt?.name ?? "Not set");
  const llmModelName = mode.model
    ? (profiles.find((p) => p.id === mode.model)?.name ?? mode.model)
    : (defaultLlm?.name ?? "Not set");

  const handleResetOverride = async () => {
    if (!mode.builtin) return;
    await onSaveMode({
      id: mode.id, name: mode.name, description: mode.description, icon: mode.icon,
      system: mode.system ?? "", user_template: mode.user_template ?? "",
      temperature: mode.temperature, model: "", stt_model: "", stt_prompt: "", stt_temperature: 0,
      max_tokens: mode.max_tokens ?? 4096, output_language: mode.output_language ?? "auto",
      auto_paste: mode.auto_paste, auto_press_enter: mode.auto_press_enter,
    });
  };

  // Compact summary: icon + name + brief pipeline badges
  const summaryBadges: string[] = [];
  summaryBadges.push(`STT: ${sttModelName}`);
  if (hasLlm) summaryBadges.push(`LLM: ${llmModelName}`);

  return (
    <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] hover:border-[rgba(255,255,255,0.08)] transition-colors">
      {/* Compact header — always visible */}
      <button
        onClick={onToggle}
        className="w-full flex items-center gap-2.5 px-3.5 py-2.5 text-left"
      >
        <span className="text-[15px] flex-shrink-0">{mode.icon}</span>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-[#fafaf9] text-[12px] font-medium">{mode.name}</span>
            {mode.builtin && (
              <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded">Built-in</span>
            )}
          </div>
          {/* Compact pipeline summary */}
          <div className="text-[9px] text-[rgba(255,255,255,0.2)] mt-0.5 truncate">
            {summaryBadges.join(" → ")}
            {mode.auto_paste ? " → Insert" : ""}
          </div>
        </div>
        {/* Expand chevron */}
        <svg
          width="12" height="12" viewBox="0 0 24 24" fill="none"
          stroke="rgba(255,255,255,0.15)" strokeWidth="2" strokeLinecap="round"
          className={`flex-shrink-0 transition-transform duration-200 ${expanded ? "rotate-180" : ""}`}
        >
          <path d="M6 9l6 6 6-6" />
        </svg>
        {/* Action buttons (non-built-in) */}
        {!mode.builtin && (
          <div className="flex items-center gap-1 flex-shrink-0" onClick={(e) => e.stopPropagation()}>
            <button
              onClick={() => onEdit({
                id: mode.id, name: mode.name, description: mode.description, icon: mode.icon,
                system: mode.system ?? "", user_template: mode.user_template ?? "",
                temperature: mode.temperature, model: mode.model ?? "", stt_model: mode.stt_model ?? "", stt_prompt: mode.stt_prompt ?? "", stt_temperature: mode.stt_temperature ?? 0,
                max_tokens: mode.max_tokens ?? 4096, output_language: mode.output_language ?? "auto",
                auto_paste: mode.auto_paste, auto_press_enter: mode.auto_press_enter,
              })}
              className="text-[rgba(255,255,255,0.2)] hover:text-[rgba(255,255,255,0.5)] text-[10px] px-1.5 transition-colors"
            >
              Edit
            </button>
            <button
              onClick={() => onDelete(mode.id)}
              className="text-[rgba(255,255,255,0.12)] hover:text-[#ef4444] text-[10px] px-1 transition-colors"
            >{"\u2715"}</button>
          </div>
        )}
      </button>

      {/* Expanded pipeline detail */}
      {expanded && (
        <div className="px-3.5 pb-3 pt-0">
          <div className="border-t border-[rgba(255,255,255,0.04)] pt-3 pl-1">
            {/* Step 1: STT */}
            <PipelineStep dotColor="#fbbf24" label="Step 1: Speech to Text" isLast={!hasLlm}>
              <div className="flex items-center gap-2">
                <span className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] text-[11px] text-[rgba(255,255,255,0.6)]">
                  {"\uD83C\uDFA4"} {sttModelName}
                </span>
                <span className={["px-1.5 py-0.5 rounded text-[8px] font-medium",
                  mode.builtin && !sttOverridden ? "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.2)]" : "bg-[rgba(245,158,11,0.1)] text-[rgba(251,191,36,0.6)]",
                ].join(" ")}>{mode.builtin && !sttOverridden ? "Default" : "Custom"}</span>
              </div>
            </PipelineStep>

            {/* Step 2: LLM */}
            {hasLlm && (
              <PipelineStep dotColor="#86efac" label="Step 2: LLM Processing" isLast={false}>
                <div className="flex flex-col gap-1.5">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] text-[11px] text-[rgba(255,255,255,0.6)]">
                      {"\uD83E\uDDE0"} {llmModelName}
                    </span>
                    <span className={["px-1.5 py-0.5 rounded text-[8px] font-medium",
                      mode.builtin && !llmOverridden ? "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.2)]" : "bg-[rgba(245,158,11,0.1)] text-[rgba(251,191,36,0.6)]",
                    ].join(" ")}>{mode.builtin && !llmOverridden ? "Default" : "Custom"}</span>
                    {mode.builtin && llmOverridden && (
                      <button onClick={handleResetOverride} className="text-[9px] text-[rgba(251,191,36,0.4)] hover:text-[#fbbf24] transition-colors">Reset</button>
                    )}
                  </div>
                  {(mode.system || mode.user_template) && (
                    <div className="px-2.5 py-1.5 rounded-md bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] text-[10px] text-[rgba(255,255,255,0.2)] font-mono leading-relaxed line-clamp-3">
                      {mode.system && <><span className="text-[rgba(255,255,255,0.3)]">System:</span> {mode.system}</>}
                      {mode.system && mode.user_template && <br/>}
                      {mode.user_template && <><span className="text-[rgba(255,255,255,0.3)]">User:</span> {mode.user_template.slice(0, 100)}{mode.user_template.length > 100 ? "..." : ""}</>}
                    </div>
                  )}
                </div>
              </PipelineStep>
            )}

            {/* Output */}
            <PipelineStep dotColor="rgba(255,255,255,0.3)" label="Output" isLast={true}>
              <div className="flex items-center gap-3 text-[10px] text-[rgba(255,255,255,0.3)]">
                {mode.auto_paste && <span>{"\u2713"} Insert at cursor</span>}
                {mode.auto_press_enter && <span>{"\u2713"} Press Enter</span>}
                {!mode.auto_paste && !mode.auto_press_enter && <span className="italic">Copy to clipboard</span>}
              </div>
            </PipelineStep>
          </div>

          {/* Customize link for built-in modes */}
          {mode.builtin && (
            <div className="mt-1 pt-2 border-t border-[rgba(255,255,255,0.04)]">
              <button
                onClick={() => onEdit({
                  id: mode.id, name: mode.name, description: mode.description, icon: mode.icon,
                  system: mode.system ?? "", user_template: mode.user_template ?? "",
                  temperature: mode.temperature, model: mode.model ?? "", stt_model: mode.stt_model ?? "", stt_prompt: mode.stt_prompt ?? "", stt_temperature: mode.stt_temperature ?? 0,
                  max_tokens: mode.max_tokens ?? 4096, output_language: mode.output_language ?? "auto",
                  auto_paste: mode.auto_paste, auto_press_enter: mode.auto_press_enter,
                })}
                className="text-[10px] text-[rgba(251,191,36,0.4)] hover:text-[#fbbf24] transition-colors"
              >Customize...</button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── ModesTab ────────────────────────────────────────────────────────────────

export default function ModesTab({
  config,
  modes,
  onSaveMode,
  onDeleteMode,
}: {
  config: AppConfig;
  modes: ModeEntry[];
  onSaveMode: (form: ModeForm) => Promise<void>;
  onDeleteMode: (id: string) => Promise<void>;
}) {
  const [editingMode, setEditingMode] = useState<ModeForm | null>(null);
  const [editingModeIsBuiltin, setEditingModeIsBuiltin] = useState<boolean>(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [showSttAdvanced, setShowSttAdvanced] = useState(false);
  const [showLlmAdvanced, setShowLlmAdvanced] = useState(false);
  const [llmEnabled, setLlmEnabled] = useState(true);

  const handleSaveModeLocal = async (form: ModeForm) => {
    await onSaveMode(form);
    setEditingMode(null);
    setEditingModeIsBuiltin(false);
  };

  return (
    <div className="flex flex-col gap-3">
      {editingMode ? (
        /* ── Mode edit form -- pipeline-organized ── */
        <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-4">
          <div className="text-[12px] font-medium text-[#fafaf9]">
            {editingMode.id && modes.some((m) => m.id === editingMode.id) ? "Edit Mode" : "New Mode"}
          </div>

          {/* Section: Identity */}
          <div className="flex flex-col gap-2">
            <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
              Identity
            </div>
            {!editingModeIsBuiltin && (
              <>
                <div className="grid grid-cols-[48px_1fr_1fr] gap-2">
                  <input
                    type="text"
                    value={editingMode.icon}
                    onChange={(e) =>
                      setEditingMode({ ...editingMode, icon: e.target.value })
                    }
                    className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2 py-2 text-center text-[16px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                    title="Emoji icon"
                  />
                  <input
                    type="text"
                    value={editingMode.id}
                    onChange={(e) =>
                      setEditingMode({ ...editingMode, id: e.target.value })
                    }
                    placeholder="mode-id"
                    className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono"
                  />
                  <input
                    type="text"
                    value={editingMode.name}
                    onChange={(e) =>
                      setEditingMode({ ...editingMode, name: e.target.value })
                    }
                    placeholder="Display Name"
                    className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                  />
                </div>
                <input
                  type="text"
                  value={editingMode.description}
                  onChange={(e) =>
                    setEditingMode({ ...editingMode, description: e.target.value })
                  }
                  placeholder="Short description of what this mode does"
                  className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                />
              </>
            )}
            {editingModeIsBuiltin && (
              <div className="flex items-center gap-2.5 px-1">
                <span className="text-[16px]">{editingMode.icon}</span>
                <div>
                  <div className="text-[12px] font-medium text-[#fafaf9]">{editingMode.name}</div>
                  <div className="text-[10px] text-[rgba(255,255,255,0.25)]">{editingMode.description}</div>
                </div>
              </div>
            )}
          </div>

          {/* Section: Step 1 -- STT */}
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <div className="rounded-full flex-shrink-0" style={{ width: 6, height: 6, backgroundColor: "#fbbf24" }} />
              <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
                Step 1: Speech to Text
              </div>
            </div>
            <div className="flex flex-col gap-2 pl-4">
              {/* Basic: STT model selector */}
              <div className="flex flex-col gap-1">
                <label className="text-[10px] text-[rgba(255,255,255,0.35)]">STT Model</label>
                <ModelSelector
                  capKey="stt"
                  value={editingMode.stt_model}
                  profiles={config.model_profiles}
                  allowDefault={true}
                  onChange={(v) => setEditingMode({ ...editingMode, stt_model: v })}
                />
              </div>
              {/* Advanced: language hint */}
              <button
                onClick={() => setShowSttAdvanced(!showSttAdvanced)}
                className="text-[9px] text-[rgba(255,255,255,0.2)] hover:text-[rgba(255,255,255,0.4)] transition-colors self-start flex items-center gap-1"
              >
                <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className={`transition-transform duration-200 ${showSttAdvanced ? "rotate-90" : ""}`}><path d="M9 18l6-6-6-6"/></svg>
                Advanced
              </button>
              {showSttAdvanced && (
                <div className="flex flex-col gap-2 pt-1 border-t border-[rgba(255,255,255,0.03)]">
                  <div className="text-[9px] text-[rgba(255,255,255,0.15)] italic mb-1">
                    Passed to the Whisper-compatible API. Unsupported params are ignored.
                  </div>
                  <div className="flex flex-col gap-1">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      Prompt hint
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">(guide vocabulary/style recognition)</span>
                    </label>
                    <input
                      type="text"
                      value={editingMode.stt_prompt}
                      onChange={(e) => setEditingMode({ ...editingMode, stt_prompt: e.target.value })}
                      placeholder="e.g. Fonos, TypeScript, React, API..."
                      className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                    />
                  </div>
                  <div className="flex flex-col gap-1">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      Temperature
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">(0 = deterministic, higher = more creative)</span>
                    </label>
                    <input
                      type="number" min={0} max={1} step={0.1}
                      value={editingMode.stt_temperature}
                      onChange={(e) => setEditingMode({ ...editingMode, stt_temperature: parseFloat(e.target.value) || 0 })}
                      className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                    />
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Section: Step 2 -- LLM (optional) */}
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <div className="rounded-full flex-shrink-0" style={{ width: 6, height: 6, backgroundColor: llmEnabled ? "#86efac" : "rgba(255,255,255,0.1)" }} />
              <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
                Step 2: LLM Processing
              </div>
              <label className="flex items-center gap-1.5 cursor-pointer ml-auto">
                <input
                  type="checkbox"
                  checked={llmEnabled}
                  onChange={(e) => {
                    if (!e.target.checked) {
                      setEditingMode({ ...editingMode, system: "", user_template: "" });
                      setLlmEnabled(false);
                    } else {
                      setEditingMode({ ...editingMode, user_template: editingMode.user_template || "{text}" });
                      setLlmEnabled(true);
                    }
                  }}
                  className="accent-[#fbbf24]"
                />
                <span className="text-[10px] text-[rgba(255,255,255,0.35)]">Enable</span>
              </label>
            </div>
            {llmEnabled && <div className="flex flex-col gap-2 pl-4">
              {/* Basic: model + prompt */}
              <div className="flex flex-col gap-1">
                <label className="text-[10px] text-[rgba(255,255,255,0.35)]">Model</label>
                <ModelSelector
                  capKey="llm"
                  value={editingMode.model}
                  profiles={config.model_profiles}
                  allowDefault={true}
                  onChange={(v) => setEditingMode({ ...editingMode, model: v })}
                />
              </div>
              <div className="flex flex-col gap-1">
                <label className="text-[10px] text-[rgba(255,255,255,0.35)]">System prompt</label>
                <textarea
                  value={editingMode.system}
                  onChange={(e) => setEditingMode({ ...editingMode, system: e.target.value })}
                  placeholder="You are a helpful assistant that..."
                  rows={3}
                  className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] leading-relaxed focus:outline-none focus:border-[rgba(245,158,11,0.3)] resize-none font-mono"
                />
              </div>
              {/* Advanced toggle */}
              <button
                onClick={() => setShowLlmAdvanced(!showLlmAdvanced)}
                className="text-[9px] text-[rgba(255,255,255,0.2)] hover:text-[rgba(255,255,255,0.4)] transition-colors self-start flex items-center gap-1"
              >
                <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className={`transition-transform duration-200 ${showLlmAdvanced ? "rotate-90" : ""}`}><path d="M9 18l6-6-6-6"/></svg>
                Advanced
              </button>
              {showLlmAdvanced && (
                <div className="flex flex-col gap-2 pt-1 border-t border-[rgba(255,255,255,0.03)]">
                  <div className="flex flex-col gap-1">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      User template
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">(use {"{text}"} for transcribed speech)</span>
                    </label>
                    <input
                      type="text"
                      value={editingMode.user_template}
                      onChange={(e) => setEditingMode({ ...editingMode, user_template: e.target.value })}
                      placeholder="{text}"
                      className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)] font-mono"
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-2">
                    <div className="flex flex-col gap-1">
                      <label className="text-[10px] text-[rgba(255,255,255,0.35)]">Temperature</label>
                      <input
                        type="number" min={0} max={2} step={0.1}
                        value={editingMode.temperature}
                        onChange={(e) => setEditingMode({ ...editingMode, temperature: parseFloat(e.target.value) || 0 })}
                        className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                      />
                    </div>
                    <div className="flex flex-col gap-1">
                      <label className="text-[10px] text-[rgba(255,255,255,0.35)]">Max tokens</label>
                      <input
                        type="number" min={1} max={128000} step={256}
                        value={editingMode.max_tokens}
                        onChange={(e) => setEditingMode({ ...editingMode, max_tokens: parseInt(e.target.value) || 4096 })}
                        className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                      />
                    </div>
                  </div>
                </div>
              )}
            </div>}
          </div>

          {/* Section: Output */}
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <div className="rounded-full flex-shrink-0" style={{ width: 6, height: 6, backgroundColor: "rgba(255,255,255,0.3)" }} />
              <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">Output</div>
            </div>
            <div className="flex flex-col gap-2 pl-4">
              {/* Basic: checkboxes */}
              <div className="flex gap-4 text-[12px] text-[rgba(255,255,255,0.5)]">
                <label className="flex items-center gap-1.5 cursor-pointer">
                  <input type="checkbox" checked={editingMode.auto_paste} onChange={(e) => setEditingMode({ ...editingMode, auto_paste: e.target.checked })} className="accent-[#fbbf24]" />
                  Insert at cursor
                </label>
                <label className="flex items-center gap-1.5 cursor-pointer">
                  <input type="checkbox" checked={editingMode.auto_press_enter} onChange={(e) => setEditingMode({ ...editingMode, auto_press_enter: e.target.checked })} className="accent-[#fbbf24]" />
                  Press Enter after
                </label>
              </div>
              <div className="flex flex-col gap-1">
                <label className="text-[10px] text-[rgba(255,255,255,0.35)]">Output language</label>
                <input
                  type="text"
                  value={editingMode.output_language}
                  onChange={(e) => setEditingMode({ ...editingMode, output_language: e.target.value })}
                  placeholder="auto (follow input language)"
                  className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                />
              </div>
            </div>
          </div>

          {/* Actions */}
          <div className="flex gap-2 pt-1">
            <button
              onClick={() => handleSaveModeLocal(editingMode)}
              className="flex-1 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-[#1a1917] text-[12px] font-semibold hover:opacity-90 transition-opacity"
            >
              {editingMode.id && modes.some((m) => m.id === editingMode.id) ? "Save Changes" : "Create Mode"}
            </button>
            <button
              onClick={() => { setEditingMode(null); setEditingModeIsBuiltin(false); }}
              className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        /* ── Pipeline list ── */
        <div className="flex flex-col gap-2">
          {modes.map((m) => (
            <ModePipelineCard
              key={m.id}
              mode={m}
              config={config}
              profiles={config.model_profiles}
              expanded={expandedId === m.id}
              onToggle={() => setExpandedId(expandedId === m.id ? null : m.id)}
              onEdit={(form) => {
                setEditingMode(form);
                setEditingModeIsBuiltin(m.builtin);
                setLlmEnabled(!!(form.system || form.user_template));
              }}
              onDelete={(id) => { onDeleteMode(id); }}
              onSaveMode={handleSaveModeLocal}
            />
          ))}

          {/* Add mode button */}
          <button
            onClick={() => { setEditingMode({ ...EMPTY_MODE }); setEditingModeIsBuiltin(false); setLlmEnabled(true); }}
            className="w-full py-2 rounded-[10px] border border-dashed border-[rgba(245,158,11,0.12)] text-[rgba(251,191,36,0.6)] text-[12px] hover:border-[rgba(245,158,11,0.25)] transition-colors"
          >
            + Create Mode
          </button>
        </div>
      )}
    </div>
  );
}
