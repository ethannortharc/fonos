// Modes tab — pipeline view, mode list, mode edit form.

import { useState } from "react";
import { t, useT } from "../../lib/i18n";
import type { AppConfig, ModeEntry, ModelProfile } from "../../types";
import { EMPTY_MODE } from "./constants";
import { ModeIcon, MicIcon, BrainIcon } from "../../components/Icons";
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
      {allowDefault && <option value="">{t("modes.use-default")}</option>}
      {capKey === "stt" && <option value="apple-speech">{t("modes.stt.apple")}</option>}
      {filtered.map((p) => (
        <option key={p.id} value={p.id}>
          {p.name} ({p.model})
        </option>
      ))}
      {filtered.length === 0 && !((capKey === "stt")) && (
        <option disabled value="">{t("modes.no-models").replace("{cap}", capKey.toUpperCase())}</option>
      )}
    </select>
  );
}

// ─── Mode Pipeline Card ──────────────────────────────────────────────────────

function modeToForm(m: ModeEntry): ModeForm {
  return {
    id: m.id, name: m.name, description: m.description, icon: m.icon,
    system: m.system ?? "", user_template: m.user_template ?? "",
    temperature: m.temperature, model: m.model ?? "", stt_model: m.stt_model ?? "",
    stt_prompt: m.stt_prompt ?? "", stt_temperature: m.stt_temperature ?? 0,
    vocab_books: m.vocab_books ?? [],
    max_tokens: m.max_tokens ?? 4096, output_language: m.output_language ?? "auto",
    auto_paste: m.auto_paste, auto_press_enter: m.auto_press_enter,
  };
}

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
      : (defaultStt?.name ?? t("models.notset"));
  const llmModelName = mode.model
    ? (profiles.find((p) => p.id === mode.model)?.name ?? mode.model)
    : (defaultLlm?.name ?? t("models.notset"));

  const handleResetOverride = async () => {
    if (!mode.builtin) return;
    await onSaveMode({
      id: mode.id, name: mode.name, description: mode.description, icon: mode.icon,
      system: mode.system ?? "", user_template: mode.user_template ?? "",
      temperature: mode.temperature, model: "", stt_model: "", stt_prompt: "", stt_temperature: 0, vocab_books: mode.vocab_books ?? [],
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
      {/* Compact header — always visible. A div (not a button) so the Edit /
          Delete action buttons can nest inside without invalid button-in-button
          markup; keyboard support is added explicitly. */}
      <div
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        onClick={onToggle}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggle();
          }
        }}
        className="w-full flex items-center gap-2.5 px-3.5 py-2.5 text-left cursor-pointer"
      >
        <span className="flex-shrink-0"><ModeIcon icon={mode.icon} size={15} /></span>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-[#fafaf9] text-[12px] font-medium">{mode.name}</span>
            {mode.builtin && (
              <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded">{t("common.builtin")}</span>
            )}
          </div>
          {/* Compact pipeline summary */}
          <div className="text-[9px] text-[rgba(255,255,255,0.2)] mt-0.5 truncate">
            {summaryBadges.join(" → ")}
            {mode.auto_paste ? ` → ${t("modes.insert-short")}` : ""}
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
                temperature: mode.temperature, model: mode.model ?? "", stt_model: mode.stt_model ?? "", stt_prompt: mode.stt_prompt ?? "", stt_temperature: mode.stt_temperature ?? 0, vocab_books: mode.vocab_books ?? [],
                max_tokens: mode.max_tokens ?? 4096, output_language: mode.output_language ?? "auto",
                auto_paste: mode.auto_paste, auto_press_enter: mode.auto_press_enter,
              })}
              className="text-[rgba(255,255,255,0.2)] hover:text-[rgba(255,255,255,0.5)] text-[10px] px-1.5 transition-colors"
            >
              {t("common.edit")}
            </button>
            <button
              onClick={() => onDelete(mode.id)}
              className="text-[rgba(255,255,255,0.12)] hover:text-[#ef4444] text-[10px] px-1 transition-colors"
            >{"\u2715"}</button>
          </div>
        )}
      </div>

      {/* Expanded pipeline detail */}
      {expanded && (
        <div className="px-3.5 pb-3 pt-0">
          <div className="border-t border-[rgba(255,255,255,0.04)] pt-3 pl-1">
            {/* Step 1: STT */}
            <PipelineStep dotColor="#fbbf24" label={t("modes.step.stt")} isLast={!hasLlm && (config.vocab_books ?? []).length === 0}>
              <div className="flex items-center gap-2">
                <span className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] text-[11px] text-[rgba(255,255,255,0.6)]">
                  <MicIcon size={11} /> {sttModelName}
                </span>
                <span className={["px-1.5 py-0.5 rounded text-[8px] font-medium",
                  mode.builtin && !sttOverridden ? "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.2)]" : "bg-[rgba(245,158,11,0.1)] text-[rgba(251,191,36,0.6)]",
                ].join(" ")}>{mode.builtin && !sttOverridden ? t("modes.badge.default") : t("modes.badge.custom")}</span>
              </div>
            </PipelineStep>

            {/* Vocabulary books mounted on this mode (in addition to Global) */}
            {(config.vocab_books ?? []).length > 0 && (
              <PipelineStep dotColor="#4ade80" label={t("modes.vocabulary")} isLast={false}>
                <div className="flex flex-wrap items-center gap-1.5">
                  {(config.vocab_books ?? []).map((b) => {
                    const isGlobal = (config.global_vocab_books ?? []).includes(b.id);
                    const mounted = (mode.vocab_books ?? []).includes(b.id);
                    if (isGlobal) {
                      return (
                        <span
                          key={b.id}
                          title={t("modes.vocab.global-title")}
                          className="px-2 py-0.5 rounded-full text-[9px] bg-[rgba(74,222,128,0.08)] border border-[rgba(74,222,128,0.15)] text-[rgba(74,222,128,0.5)]"
                        >
                          {b.name} · {t("modes.vocab.global-tag")}
                        </span>
                      );
                    }
                    return (
                      <button
                        key={b.id}
                        onClick={() => {
                          const current = mode.vocab_books ?? [];
                          const next = mounted
                            ? current.filter((id) => id !== b.id)
                            : [...current, b.id];
                          void onSaveMode({ ...modeToForm(mode), vocab_books: next });
                        }}
                        title={mounted ? t("modes.vocab.unmount").replace("{name}", b.name) : t("modes.vocab.mount").replace("{name}", b.name)}
                        className={[
                          "px-2 py-0.5 rounded-full text-[9px] transition-all",
                          mounted
                            ? "bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.3)] text-[#fbbf24]"
                            : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.35)] hover:border-[rgba(255,255,255,0.12)]",
                        ].join(" ")}
                      >
                        {b.name}
                      </button>
                    );
                  })}
                </div>
              </PipelineStep>
            )}

            {/* Step 2: LLM */}
            {hasLlm && (
              <PipelineStep dotColor="#86efac" label={t("modes.step.llm")} isLast={false}>
                <div className="flex flex-col gap-1.5">
                  <div className="flex items-center gap-2">
                    <span className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] text-[11px] text-[rgba(255,255,255,0.6)]">
                      <BrainIcon size={11} /> {llmModelName}
                    </span>
                    <span className={["px-1.5 py-0.5 rounded text-[8px] font-medium",
                      mode.builtin && !llmOverridden ? "bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.2)]" : "bg-[rgba(245,158,11,0.1)] text-[rgba(251,191,36,0.6)]",
                    ].join(" ")}>{mode.builtin && !llmOverridden ? t("modes.badge.default") : t("modes.badge.custom")}</span>
                    {mode.builtin && llmOverridden && (
                      <button onClick={handleResetOverride} className="text-[9px] text-[rgba(251,191,36,0.4)] hover:text-[#fbbf24] transition-colors">{t("modes.reset")}</button>
                    )}
                  </div>
                  {(mode.system || mode.user_template) && (
                    <div className="px-2.5 py-1.5 rounded-md bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.04)] text-[10px] text-[rgba(255,255,255,0.2)] font-mono leading-relaxed line-clamp-3">
                      {mode.system && <><span className="text-[rgba(255,255,255,0.3)]">{t("modes.preview.system")}</span> {mode.system}</>}
                      {mode.system && mode.user_template && <br/>}
                      {mode.user_template && <><span className="text-[rgba(255,255,255,0.3)]">{t("modes.preview.user")}</span> {mode.user_template.slice(0, 100)}{mode.user_template.length > 100 ? "..." : ""}</>}
                    </div>
                  )}
                </div>
              </PipelineStep>
            )}

            {/* Output */}
            <PipelineStep dotColor="rgba(255,255,255,0.3)" label={t("modes.output")} isLast={true}>
              <div className="flex items-center gap-3 text-[10px] text-[rgba(255,255,255,0.3)]">
                {mode.auto_paste && <span>{"\u2713"} {t("modes.insert-cursor")}</span>}
                {mode.auto_press_enter && <span>{"\u2713"} {t("modes.press-enter")}</span>}
                {!mode.auto_paste && !mode.auto_press_enter && <span className="italic">{t("modes.copy-clipboard")}</span>}
              </div>
            </PipelineStep>
          </div>

          {/* Customize link for built-in modes */}
          {mode.builtin && (
            <div className="mt-1 pt-2 border-t border-[rgba(255,255,255,0.04)]">
              <button
                onClick={() => onEdit(modeToForm(mode))}
                className="text-[10px] text-[rgba(251,191,36,0.4)] hover:text-[#fbbf24] transition-colors"
              >{t("modes.customize")}</button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── ModesTab ────────────────────────────────────────────────────────────────

// ─── Collapsible Mode Section — default mode pinned to top ────────────────────

function ModeSection({
  modes, config, expandedId, onToggle, onEdit, onDelete, onSaveMode, onCreateNew,
}: {
  modes: ModeEntry[];
  config: AppConfig;
  expandedId: string | null;
  onToggle: (id: string) => void;
  onEdit: (m: ModeEntry, form: ModeForm) => void;
  onDelete: (id: string) => void;
  onSaveMode: (form: ModeForm) => Promise<void>;
  onCreateNew: () => void;
}) {
  const [collapsed, setCollapsed] = useState(false);

  // Sort: default mode first, then built-in, then custom
  const defaultMode = config.dictation_mode || "raw";
  const sorted = [...modes].sort((a, b) => {
    if (a.id === defaultMode) return -1;
    if (b.id === defaultMode) return 1;
    if (a.builtin && !b.builtin) return -1;
    if (!a.builtin && b.builtin) return 1;
    return 0;
  });

  return (
    <div className="flex flex-col gap-2">
      {/* Section header — click to collapse */}
      <button
        onClick={() => setCollapsed(!collapsed)}
        className="flex items-center gap-2 py-1"
      >
        <svg
          width="10" height="10" viewBox="0 0 24 24" fill="none"
          stroke="rgba(255,255,255,0.25)" strokeWidth="2" strokeLinecap="round"
          className={`transition-transform duration-200 ${collapsed ? "" : "rotate-90"}`}
        >
          <path d="M9 18l6-6-6-6" />
        </svg>
        <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] font-semibold">
          {t("modes.heading")}
        </span>
        <span className="text-[9px] text-[rgba(255,255,255,0.15)]">
          ({modes.length})
        </span>
      </button>

      {!collapsed && (
        <div className="flex flex-col gap-2">
          {sorted.map((m) => (
            <div key={m.id} className="relative">
              {m.id === defaultMode && (
                <div className="absolute -left-1 top-2 bottom-2 w-[2px] rounded bg-[#fbbf24]" />
              )}
              <ModePipelineCard
                mode={m}
                config={config}
                profiles={config.model_profiles}
                expanded={expandedId === m.id}
                onToggle={() => onToggle(m.id)}
                onEdit={(form) => onEdit(m, form)}
                onDelete={(id) => onDelete(id)}
                onSaveMode={onSaveMode}
              />
            </div>
          ))}

          <button
            onClick={onCreateNew}
            className="w-full py-2 rounded-[10px] border border-dashed border-[rgba(245,158,11,0.12)] text-[rgba(251,191,36,0.6)] text-[12px] hover:border-[rgba(245,158,11,0.25)] transition-colors"
          >
            {t("modes.create")}
          </button>
        </div>
      )}
    </div>
  );
}

// ─── Main ModesTab ───────────────────────────────────────────────────────────

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
  useT();
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
            {editingMode.id && modes.some((m) => m.id === editingMode.id) ? t("modes.edit-mode") : t("modes.new-mode")}
          </div>

          {/* Section: Identity */}
          <div className="flex flex-col gap-2">
            <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">
              {t("modes.identity")}
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
                    title={t("modes.emoji-title")}
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
                    placeholder={t("modes.ph.name")}
                    className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                  />
                </div>
                <input
                  type="text"
                  value={editingMode.description}
                  onChange={(e) =>
                    setEditingMode({ ...editingMode, description: e.target.value })
                  }
                  placeholder={t("modes.ph.desc")}
                  className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                />
              </>
            )}
            {editingModeIsBuiltin && (
              <div className="flex items-center gap-2.5 px-1">
                <span className="flex-shrink-0"><ModeIcon icon={editingMode.icon} size={16} /></span>
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
                {t("modes.step.stt")}
              </div>
            </div>
            <div className="flex flex-col gap-2 pl-4">
              {/* Basic: STT model selector */}
              <div className="flex flex-col gap-1">
                <label className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("modes.stt-model")}</label>
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
                {t("modes.advanced")}
              </button>
              {showSttAdvanced && (
                <div className="flex flex-col gap-2 pt-1 border-t border-[rgba(255,255,255,0.03)]">
                  <div className="text-[9px] text-[rgba(255,255,255,0.15)] italic mb-1">
                    {t("modes.stt.advanced-note")}
                  </div>
                  <div className="flex flex-col gap-1">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      {t("modes.stt.prompt-label")}
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">{t("modes.stt.prompt-hint")}</span>
                    </label>
                    <input
                      type="text"
                      value={editingMode.stt_prompt}
                      onChange={(e) => setEditingMode({ ...editingMode, stt_prompt: e.target.value })}
                      placeholder={t("modes.stt.prompt-ph")}
                      className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                    />
                  </div>
                  {(config.vocab_books ?? []).length > 0 && (
                    <div className="flex flex-col gap-1">
                      <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                        {t("modes.vocab-books-label")}
                        <span className="ml-1 text-[rgba(255,255,255,0.15)]">{t("modes.vocab-books-hint")}</span>
                      </label>
                      <div className="flex flex-wrap gap-1.5">
                        {(config.vocab_books ?? []).map((b) => {
                          const selected = editingMode.vocab_books.includes(b.id);
                          return (
                            <button
                              key={b.id}
                              onClick={() =>
                                setEditingMode({
                                  ...editingMode,
                                  vocab_books: selected
                                    ? editingMode.vocab_books.filter((id) => id !== b.id)
                                    : [...editingMode.vocab_books, b.id],
                                })
                              }
                              className={[
                                "px-2.5 py-1 rounded-full text-[10px] transition-all",
                                selected
                                  ? "bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.3)] text-[#fbbf24]"
                                  : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] hover:border-[rgba(255,255,255,0.12)]",
                              ].join(" ")}
                            >
                              {b.name}
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  )}
                  <div className="flex flex-col gap-1">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      {t("modes.temperature")}
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">{t("modes.temp-hint")}</span>
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
                {t("modes.step.llm")}
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
                <span className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("modes.enable")}</span>
              </label>
            </div>
            {llmEnabled && <div className="flex flex-col gap-2 pl-4">
              {/* Basic: model + prompt */}
              <div className="flex flex-col gap-1">
                <label className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("modes.model")}</label>
                <ModelSelector
                  capKey="llm"
                  value={editingMode.model}
                  profiles={config.model_profiles}
                  allowDefault={true}
                  onChange={(v) => setEditingMode({ ...editingMode, model: v })}
                />
              </div>
              <div className="flex flex-col gap-1">
                <label className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("modes.system-prompt")}</label>
                <textarea
                  value={editingMode.system}
                  onChange={(e) => setEditingMode({ ...editingMode, system: e.target.value })}
                  placeholder={t("modes.system-ph")}
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
                {t("modes.advanced")}
              </button>
              {showLlmAdvanced && (
                <div className="flex flex-col gap-2 pt-1 border-t border-[rgba(255,255,255,0.03)]">
                  <div className="flex flex-col gap-1">
                    <label className="text-[10px] text-[rgba(255,255,255,0.35)]">
                      {t("modes.user-template")}
                      <span className="ml-1 text-[rgba(255,255,255,0.15)]">{t("modes.user-template-hint")}</span>
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
                      <label className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("modes.temperature")}</label>
                      <input
                        type="number" min={0} max={2} step={0.1}
                        value={editingMode.temperature}
                        onChange={(e) => setEditingMode({ ...editingMode, temperature: parseFloat(e.target.value) || 0 })}
                        className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
                      />
                    </div>
                    <div className="flex flex-col gap-1">
                      <label className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("modes.max-tokens")}</label>
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
              <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{t("modes.output")}</div>
            </div>
            <div className="flex flex-col gap-2 pl-4">
              {/* Basic: checkboxes */}
              <div className="flex gap-4 text-[12px] text-[rgba(255,255,255,0.5)]">
                <label className="flex items-center gap-1.5 cursor-pointer">
                  <input type="checkbox" checked={editingMode.auto_paste} onChange={(e) => setEditingMode({ ...editingMode, auto_paste: e.target.checked })} className="accent-[#fbbf24]" />
                  {t("modes.insert-cursor")}
                </label>
                <label className="flex items-center gap-1.5 cursor-pointer">
                  <input type="checkbox" checked={editingMode.auto_press_enter} onChange={(e) => setEditingMode({ ...editingMode, auto_press_enter: e.target.checked })} className="accent-[#fbbf24]" />
                  {t("modes.press-enter-after")}
                </label>
              </div>
              <div className="flex flex-col gap-1">
                <label className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("modes.output-language")}</label>
                <input
                  type="text"
                  value={editingMode.output_language}
                  onChange={(e) => setEditingMode({ ...editingMode, output_language: e.target.value })}
                  placeholder={t("modes.output-lang-ph")}
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
              {editingMode.id && modes.some((m) => m.id === editingMode.id) ? t("modes.save-changes") : t("modes.create-mode")}
            </button>
            <button
              onClick={() => { setEditingMode(null); setEditingModeIsBuiltin(false); }}
              className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
            >
              {t("common.cancel")}
            </button>
          </div>
        </div>
      ) : (
        /* ── Mode list in collapsible section, default mode first ── */
        <ModeSection
          modes={modes}
          config={config}
          expandedId={expandedId}
          onToggle={(id) => setExpandedId(expandedId === id ? null : id)}
          onEdit={(m, form) => {
            setEditingMode(form);
            setEditingModeIsBuiltin(m.builtin);
            setLlmEnabled(!!(form.system || form.user_template));
          }}
          onDelete={(id) => { onDeleteMode(id); }}
          onSaveMode={handleSaveModeLocal}
          onCreateNew={() => { setEditingMode({ ...EMPTY_MODE }); setEditingModeIsBuiltin(false); setLlmEnabled(true); }}
        />
      )}
    </div>
  );
}
