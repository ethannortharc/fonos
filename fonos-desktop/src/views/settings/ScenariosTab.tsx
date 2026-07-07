// Scenarios tab (Settings) — the home for saved configuration bundles.
//
// Top → bottom:
//   a. Current configuration overview — the LIVE config's user-visible state
//      (role → model rows, dictation defaults, speech summary, vocab count).
//   b. Saved scenarios — the saved list + save-current (with section include
//      checkboxes) + import / export, moved here from the Scenarios view.
//   c. Setup templates — a button that opens the three preset cards overlay.
//
// The Scenarios view itself now keeps only the preset cards + step-2 flow.

import { useState } from "react";
import { t, td, useT, type TKey } from "../../lib/i18n";
import type {
  AppConfig,
  HotkeysSection,
  ModeEntry,
  ModelProfile,
  SavedScenario,
  ScenarioAssignments,
  VocabBook,
} from "../../types";
import {
  saveScenario,
  applySavedScenario,
  deleteSavedScenario,
  exportScenario,
  importScenario,
  importScenarioJson,
} from "../../lib/api";
import Scenarios, {
  PreviewRow,
  KEY_PROVIDERS,
  control,
  errStr,
  relDate,
  type RoleKey,
} from "../Scenarios";

// ── shared preview helpers ────────────────────────────────────────────────────

/** Build role → profile preview rows from a profile pool + role assignments,
 *  skipping any role whose profile can't be resolved. Mirrors how Apply pairs
 *  the conversation voice on sts_voice_profile (falling back to tts_profile). */
function buildRoleRows(
  profiles: ModelProfile[],
  a: ScenarioAssignments
): { role: RoleKey; profile: ModelProfile; voice?: string }[] {
  const byId = (id: string) => profiles.find((p) => p.id === id);
  const rows: { role: RoleKey; profile: ModelProfile; voice?: string }[] = [];
  const push = (role: RoleKey, id: string, voice?: string) => {
    const p = id ? byId(id) : undefined;
    if (p) rows.push({ role, profile: p, voice });
  };
  push("stt", a.stt_profile);
  push("llm", a.llm_profile);
  push("conv", a.sts_voice_profile || a.tts_profile, a.sts_voice);
  push("listen", a.listen_voice_profile, a.listen_voice);
  return rows;
}

function RolePreview({ rows }: { rows: { role: RoleKey; profile: ModelProfile; voice?: string }[] }) {
  if (rows.length === 0) return null;
  return (
    <div className="rounded-lg border border-[rgba(255,255,255,0.06)] divide-y divide-[rgba(255,255,255,0.04)]">
      {rows.map((r) => (
        <PreviewRow key={r.role} role={r.role} profile={r.profile} voice={r.voice} />
      ))}
    </div>
  );
}

/** One "Label → value" summary line, matching the muted preview language. */
function SummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center gap-2 px-3 py-2">
      <span className="w-[92px] flex-none text-[10px] text-[rgba(255,255,255,0.4)]">{label}</span>
      <span className="min-w-0 text-[11px] text-[#fafaf9] truncate">{value}</span>
    </div>
  );
}

function SummaryCard({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-[rgba(255,255,255,0.06)] divide-y divide-[rgba(255,255,255,0.04)]">
      {children}
    </div>
  );
}

const SECTION_LABEL: Record<string, TKey> = {
  models: "scen.section.models",
  dictation: "scen.section.dictation",
  speech: "scen.section.speech",
  vocab: "scen.section.vocab",
  hotkeys: "scen.section.hotkeys",
};

// ── hotkey combo formatting ───────────────────────────────────────────────────

const MOD_SYMBOL: Record<string, string> = {
  cmd: "⌘", command: "⌘", meta: "⌘",
  ctrl: "⌃", control: "⌃",
  alt: "⌥", option: "⌥", opt: "⌥",
  shift: "⇧",
};

/** Render a stored combo like "cmd+shift+space" as "⌘⇧Space". */
function formatCombo(combo: string): string {
  return combo
    .split("+")
    .map((p) => p.trim().toLowerCase())
    .filter(Boolean)
    .map((p) => {
      if (MOD_SYMBOL[p]) return MOD_SYMBOL[p];
      if (p === "space") return "Space";
      return p.length === 1 ? p.toUpperCase() : p.charAt(0).toUpperCase() + p.slice(1);
    })
    .join("");
}

/** The (label, combo) pairs to preview for a hotkeys section, non-empty only. */
function hotkeyItems(h: HotkeysSection): { label: string; combo: string }[] {
  const raw: [string, string][] = [
    [t("hotkeys.dictation"), h.hotkey_dictation],
    [t("hotkeys.dictationtoggle"), h.hotkey_dictation_toggle],
    ["TTS", h.hotkey_tts],
    [t("hotkeys.agentspeak"), h.hotkey_agent],
    [t("hotkeys.agentpanel"), h.hotkey_agent_panel],
    [t("hotkeys.notepanel"), h.hotkey_note],
    [`${t("hotkeys.shortcut")} 1`, h.hotkey_note_1],
    [`${t("hotkeys.shortcut")} 2`, h.hotkey_note_2],
    [`${t("hotkeys.shortcut")} 3`, h.hotkey_note_3],
    [t("hotkeys.meeting"), h.hotkey_meeting],
    // "hotkeys.transform" i18n key was removed with the Quick Transform UI (superseded by
    // text actions); this label is legacy scenario read-compat only, so it's hardcoded.
    ["Transform", h.hotkey_transform],
    [t("scen.sum.listen"), h.hotkey_listen],
    [t("scen.sum.convo"), h.hotkey_sts],
  ];
  return raw.filter(([, c]) => c && c.trim()).map(([label, combo]) => ({ label, combo }));
}

function SectionBadges({ sections }: { sections: string[] }) {
  return (
    <>
      {sections.map((s) => (
        <span
          key={s}
          className="text-[8.5px] px-1.5 py-0.5 rounded-full bg-[rgba(251,191,36,0.1)] text-[rgba(251,191,36,0.7)]"
        >
          {t(SECTION_LABEL[s])}
        </span>
      ))}
    </>
  );
}

const personaSnippet = (s?: string) => {
  const p = (s ?? "").trim();
  return p.length > 60 ? `${p.slice(0, 60)}…` : p;
};

// ── dictation / speech summaries (reused by overview + saved rows) ────────────

function DictationSummary({
  modeName,
  customCount,
  translateTarget,
}: {
  modeName: string;
  customCount: number;
  translateTarget: string;
}) {
  return (
    <SummaryCard>
      <SummaryRow label={t("scen.sum.mode")} value={modeName} />
      <SummaryRow label={t("scen.sum.custom")} value={td("scen.sum.custommodes", [String(customCount)])} />
      {translateTarget && <SummaryRow label={t("scen.sum.translate")} value={translateTarget} />}
    </SummaryCard>
  );
}

function SpeechSummary({
  listenMode,
  listenVoice,
  persona,
  convVoice,
  turns,
}: {
  listenMode: string;
  listenVoice: string;
  persona: string;
  convVoice: string;
  turns: number;
}) {
  const listen = [listenMode, listenVoice && listenVoice !== "default" ? listenVoice : ""]
    .filter(Boolean)
    .join(" · ");
  return (
    <SummaryCard>
      {listen && <SummaryRow label={t("scen.sum.listen")} value={listen} />}
      {persona && <SummaryRow label={t("scen.sum.persona")} value={persona} />}
      <SummaryRow
        label={t("scen.sum.convo")}
        value={[convVoice && convVoice !== "default" ? convVoice : t("scen.sum.defaultvoice"), td("scen.sum.turns", [String(turns)])].join(" · ")}
      />
    </SummaryCard>
  );
}

function VocabSummary({ books }: { books: VocabBook[] }) {
  const names = books.map((b) => b.name).filter(Boolean);
  const shown = names.slice(0, 3).join(", ");
  const more = names.length > 3 ? ", …" : "";
  const value =
    books.length === 0
      ? t("scen.sum.vocabnone")
      : names.length
        ? `${td("scen.sum.vocabbooks", [String(books.length)])} · ${shown}${more}`
        : td("scen.sum.vocabbooks", [String(books.length)]);
  return (
    <SummaryCard>
      <SummaryRow label={t("scen.section.vocab")} value={value} />
    </SummaryCard>
  );
}

function HotkeysSummary({ hotkeys }: { hotkeys: HotkeysSection }) {
  const items = hotkeyItems(hotkeys);
  // `undefined`/`null` = scenario predates text actions (no row at all);
  // present (even `[]`) = show the count, same as every other section.
  const textActionsCount = hotkeys.text_actions?.length;
  const hasTextActions = textActionsCount !== undefined;
  if (items.length === 0 && !hasTextActions) {
    return (
      <SummaryCard>
        <SummaryRow label={t("scen.section.hotkeys")} value={t("scen.sum.hotkeysnone")} />
      </SummaryCard>
    );
  }
  return (
    <div className="rounded-lg border border-[rgba(255,255,255,0.06)] px-3 py-2 flex flex-wrap gap-x-2.5 gap-y-1">
      {hasTextActions && (
        <span className="text-[10px] text-[rgba(255,255,255,0.4)] whitespace-nowrap">
          {t("hotkeys.section.textactions")} <span className="font-mono text-[#fafaf9]">({textActionsCount})</span>
        </span>
      )}
      {items.map((it) => (
        <span key={it.label} className="text-[10px] text-[rgba(255,255,255,0.4)] whitespace-nowrap">
          {it.label} <span className="font-mono text-[#fafaf9]">{formatCombo(it.combo)}</span>
        </span>
      ))}
    </div>
  );
}

// ── overview card (live config) ───────────────────────────────────────────────

function OverviewCard({ config, modes }: { config: AppConfig; modes: ModeEntry[] }) {
  const liveAssign: ScenarioAssignments = {
    stt_profile: config.stt_profile,
    llm_profile: config.llm_profile,
    tts_profile: config.tts_profile,
    sts_voice_profile: config.sts_voice_profile ?? "",
    listen_voice_profile: config.listen_voice_profile ?? "",
    sts_voice: config.sts_voice ?? "default",
    listen_voice: config.listen_voice ?? "default",
  };
  const rows = buildRoleRows(config.model_profiles ?? [], liveAssign);

  const defaultMode = config.dictation_mode || "raw";
  const modeName = modes.find((m) => m.id === defaultMode)?.name ?? defaultMode;
  const customCount = modes.filter((m) => !m.builtin).length;

  const liveHotkeys: HotkeysSection = {
    hotkey_dictation: config.hotkey_dictation ?? "",
    hotkey_dictation_toggle: config.hotkey_dictation_toggle ?? "",
    hotkey_tts: config.hotkey_tts ?? "",
    hotkey_agent: config.hotkey_agent ?? "",
    hotkey_agent_panel: config.hotkey_agent_panel ?? "",
    hotkey_note: config.hotkey_note ?? "",
    hotkey_note_1: config.hotkey_note_1 ?? "",
    hotkey_note_2: config.hotkey_note_2 ?? "",
    hotkey_note_3: config.hotkey_note_3 ?? "",
    notebook_hotkey_1: config.notebook_hotkey_1 ?? 0,
    notebook_hotkey_2: config.notebook_hotkey_2 ?? 0,
    notebook_hotkey_3: config.notebook_hotkey_3 ?? 0,
    hotkey_meeting: config.hotkey_meeting ?? "",
    hotkey_transform: config.hotkey_transform ?? "",
    hotkey_listen: config.hotkey_listen ?? "",
    hotkey_sts: config.hotkey_sts ?? "",
    text_actions: config.text_actions ?? [],
  };

  return (
    <div className="rounded-xl border border-[rgba(255,255,255,0.07)] bg-[rgba(255,255,255,0.02)] p-4 flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <span className="text-[11px] uppercase tracking-wider text-[rgba(255,255,255,0.35)]">
          {t("scen.tab.current")}
        </span>
      </div>

      {rows.length > 0 ? (
        <RolePreview rows={rows} />
      ) : (
        <div className="text-[10.5px] text-[rgba(255,255,255,0.3)]">{t("scen.tab.nomodels")}</div>
      )}

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
        <div className="flex flex-col gap-1.5">
          <span className="text-[9px] uppercase tracking-wider text-[rgba(255,255,255,0.25)]">
            {t("scen.section.dictation")}
          </span>
          <DictationSummary
            modeName={modeName}
            customCount={customCount}
            translateTarget={config.translate_target}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <span className="text-[9px] uppercase tracking-wider text-[rgba(255,255,255,0.25)]">
            {t("scen.section.speech")}
          </span>
          <SpeechSummary
            listenMode={config.listen_mode ?? "listen"}
            listenVoice={config.listen_voice ?? "default"}
            persona={personaSnippet(config.sts_persona)}
            convVoice={config.sts_voice ?? "default"}
            turns={config.sts_max_turns ?? 0}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <span className="text-[9px] uppercase tracking-wider text-[rgba(255,255,255,0.25)]">
            {t("scen.section.vocab")}
          </span>
          <VocabSummary books={config.vocab_books ?? []} />
        </div>
      </div>

      <div className="flex flex-col gap-1.5">
        <span className="text-[9px] uppercase tracking-wider text-[rgba(255,255,255,0.25)]">
          {t("scen.section.hotkeys")}
        </span>
        <HotkeysSummary hotkeys={liveHotkeys} />
      </div>
    </div>
  );
}

// ── saved scenarios ────────────────────────────────────────────────────────────

function SavedRow({
  scenario,
  onApply,
  onDelete,
}: {
  scenario: SavedScenario;
  onApply: (id: string, name: string) => void;
  onDelete: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [includeKeys, setIncludeKeys] = useState(false);
  const [exportedPath, setExportedPath] = useState("");
  const [copied, setCopied] = useState(false);
  const [err, setErr] = useState("");

  const sections: string[] = [];
  if (scenario.models) sections.push("models");
  if (scenario.dictation) sections.push("dictation");
  if (scenario.speech) sections.push("speech");
  if (scenario.vocab) sections.push("vocab");
  if (scenario.hotkeys) sections.push("hotkeys");

  const modelProfiles = scenario.models?.profiles ?? [];
  const roleRows = scenario.models
    ? buildRoleRows(modelProfiles, scenario.models.assignments)
    : [];

  const dict = scenario.dictation;
  const dictCustomCount = dict ? Object.keys(dict.user_modes ?? {}).length : 0;
  const speech = scenario.speech;

  const doExport = async () => {
    setErr("");
    try {
      // Key-stripping walks the models-section profiles.
      const payload: SavedScenario = scenario.models
        ? {
            ...scenario,
            models: {
              ...scenario.models,
              profiles: includeKeys
                ? scenario.models.profiles
                : scenario.models.profiles.map((p) => ({ ...p, api_key: "" })),
            },
          }
        : scenario;
      const path = await exportScenario(JSON.stringify(payload, null, 2), scenario.name);
      setExportedPath(path);
    } catch (e) {
      setErr(errStr(e));
    }
  };

  return (
    <div className="rounded-lg border border-[rgba(255,255,255,0.06)] bg-[rgba(255,255,255,0.02)] overflow-hidden">
      <div
        onClick={() => setExpanded((v) => !v)}
        className="flex items-center gap-2 px-3 py-2.5 cursor-pointer select-none"
      >
        <span className="text-[11.5px] font-medium text-[#fafaf9] truncate">{scenario.name || "—"}</span>
        <SectionBadges sections={sections} />
        <span className="flex-1" />
        <svg
          width="10"
          height="10"
          viewBox="0 0 24 24"
          fill="none"
          stroke="rgba(255,255,255,0.25)"
          strokeWidth="2"
          strokeLinecap="round"
          className={`flex-none transition-transform duration-200 ${expanded ? "rotate-90" : ""}`}
        >
          <path d="M9 18l6-6-6-6" />
        </svg>
      </div>

      {expanded && (
        <div className="px-3 pb-3 pt-2.5 border-t border-[rgba(255,255,255,0.04)] flex flex-col gap-2.5">
          {roleRows.length > 0 && <RolePreview rows={roleRows} />}

          {dict && (
            <DictationSummary
              modeName={dict.dictation_mode || "raw"}
              customCount={dictCustomCount}
              translateTarget={dict.translate_target}
            />
          )}

          {speech && (
            <SpeechSummary
              listenMode={speech.listen_mode || "listen"}
              listenVoice={speech.listen_voice || "default"}
              persona={personaSnippet(speech.sts_persona)}
              convVoice={speech.sts_voice || "default"}
              turns={speech.sts_max_turns}
            />
          )}

          {scenario.vocab && <VocabSummary books={scenario.vocab.vocab_books ?? []} />}

          {scenario.hotkeys && <HotkeysSummary hotkeys={scenario.hotkeys} />}

          <div className="text-[9.5px] text-[rgba(255,255,255,0.3)]">
            {scenario.models && `${td("scen.saved.profiles", [String(modelProfiles.length)])} · `}
            {td("scen.saved.savedat", [relDate(scenario.created_at)])}
          </div>

          <div className="flex items-center gap-1.5">
            <button
              onClick={() => onApply(scenario.id, scenario.name)}
              className="text-[10px] px-2.5 py-1 rounded-md bg-[rgba(251,191,36,0.12)] border border-[rgba(251,191,36,0.3)] text-[#fbbf24]"
            >
              {t("scen.saved.apply")}
            </button>
            <button
              onClick={() => setExporting((v) => !v)}
              className="text-[10px] px-2 py-1 rounded-md border border-[rgba(255,255,255,0.1)] text-[rgba(255,255,255,0.5)] hover:text-[#fafaf9] transition-colors"
            >
              {t("scen.share.export")}
            </button>
            <button
              onClick={() => onDelete(scenario.id)}
              className="text-[10px] px-2 py-1 rounded-md border border-[rgba(255,255,255,0.08)] text-[rgba(255,255,255,0.35)] hover:text-[#f87171] transition-colors"
            >
              {t("scen.saved.delete")}
            </button>
          </div>

          {exporting && (
            <div className="flex flex-col gap-2 pt-1">
              {scenario.models && (
                <label className="flex items-center gap-2 text-[10px] text-[rgba(255,255,255,0.5)]">
                  <input type="checkbox" checked={includeKeys} onChange={(e) => setIncludeKeys(e.target.checked)} />
                  {t("scen.share.includekeys")}
                </label>
              )}
              <div className="flex items-center gap-2">
                <button
                  onClick={doExport}
                  className="text-[10px] px-2.5 py-1 rounded-md bg-[rgba(255,255,255,0.05)] border border-[rgba(255,255,255,0.1)] text-[rgba(255,255,255,0.7)]"
                >
                  {t("scen.share.export")}
                </button>
                {exportedPath && (
                  <>
                    <span className="text-[10px] text-[#4ade80] truncate max-w-[300px]">
                      {td("scen.share.exported", [exportedPath])}
                    </span>
                    <button
                      onClick={() => {
                        navigator.clipboard?.writeText(exportedPath).then(
                          () => {
                            setCopied(true);
                            setTimeout(() => setCopied(false), 1500);
                          },
                          () => {}
                        );
                      }}
                      className="text-[10px] px-2 py-1 rounded-md border border-[rgba(255,255,255,0.1)] text-[rgba(255,255,255,0.5)]"
                    >
                      {copied ? t("scen.share.copied") : t("scen.share.copy")}
                    </button>
                  </>
                )}
              </div>
              {err && <span className="text-[10px] text-[#f87171]">{err}</span>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function ImportZone({ onImported }: { onImported: () => void }) {
  const [path, setPath] = useState("");
  const [drag, setDrag] = useState(false);
  const [msg, setMsg] = useState("");
  const [err, setErr] = useState("");
  const [needKeys, setNeedKeys] = useState<string[]>([]);

  const report = (s: SavedScenario) => {
    setErr("");
    setMsg(td("scen.share.imported", [s.name || "—"]));
    const needing = (s.models?.profiles ?? [])
      .filter((p) => KEY_PROVIDERS.has(p.provider) && !(p.api_key ?? ""))
      .map((p) => p.name || p.model);
    setNeedKeys(needing);
    onImported();
  };
  const fail = () => {
    setMsg("");
    setNeedKeys([]);
    setErr(t("scen.share.invalid"));
  };

  const importFromText = async (text: string) => {
    try {
      report(await importScenarioJson(text));
    } catch {
      fail();
    }
  };
  const importFromPath = async () => {
    if (!path.trim()) return;
    try {
      report(await importScenario(path.trim()));
    } catch {
      fail();
    }
  };

  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDrag(false);
    const file = e.dataTransfer.files?.[0];
    if (!file) return;
    file.text().then(importFromText, fail);
  };

  return (
    <div className="mt-2 flex flex-col gap-2">
      <div
        onDragOver={(e) => {
          e.preventDefault();
          setDrag(true);
        }}
        onDragLeave={() => setDrag(false)}
        onDrop={onDrop}
        className={[
          "rounded-lg border border-dashed px-3 py-3 text-center transition-colors",
          drag ? "border-[rgba(251,191,36,0.5)] bg-[rgba(251,191,36,0.05)]" : "border-[rgba(255,255,255,0.1)]",
        ].join(" ")}
      >
        <div className="text-[10.5px] text-[rgba(255,255,255,0.4)]">{t("scen.share.importhint")}</div>
        <div className="flex items-center gap-1.5 mt-2 max-w-[440px] mx-auto">
          <input
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder={t("scen.share.pathph")}
            className={`${control} font-mono flex-1 min-w-0`}
          />
          <button
            onClick={importFromPath}
            className="text-[10px] px-2.5 py-1 rounded-md border border-[rgba(255,255,255,0.1)] text-[rgba(255,255,255,0.6)] hover:text-[#fafaf9] transition-colors"
          >
            {t("scen.share.importbtn")}
          </button>
        </div>
      </div>
      {msg && <div className="text-[10px] text-[#4ade80]">{msg}</div>}
      {needKeys.length > 0 && (
        <div className="text-[10px] text-[#fbbf24]">{td("scen.share.needkeys", [needKeys.join(", ")])}</div>
      )}
      {err && <div className="text-[10px] text-[#f87171]">{err}</div>}
    </div>
  );
}

// ── save current (with section include checkboxes) ────────────────────────────

function IncludeCheck({ v, set, label }: { v: boolean; set: (b: boolean) => void; label: string }) {
  return (
    <label className="flex items-center gap-1.5 text-[10px] text-[rgba(255,255,255,0.55)] cursor-pointer">
      <input type="checkbox" checked={v} onChange={(e) => set(e.target.checked)} className="accent-[#fbbf24]" />
      {label}
    </label>
  );
}

function SaveCurrent({
  onSave,
}: {
  onSave: (name: string, m: boolean, d: boolean, s: boolean, v: boolean, h: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [incModels, setIncModels] = useState(true);
  const [incDictation, setIncDictation] = useState(true);
  const [incSpeech, setIncSpeech] = useState(true);
  const [incVocab, setIncVocab] = useState(true);
  const [incHotkeys, setIncHotkeys] = useState(true);

  const canSave =
    name.trim().length > 0 && (incModels || incDictation || incSpeech || incVocab || incHotkeys);

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="text-[10px] px-2.5 py-1 rounded-md border border-[rgba(255,255,255,0.1)] text-[rgba(255,255,255,0.55)] hover:text-[#fafaf9] transition-colors"
      >
        {t("scen.saved.savecurrent")}
      </button>
    );
  }

  return (
    <div className="flex flex-col gap-2 items-end">
      <div className="flex items-center gap-1.5">
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t("scen.saved.nameph")}
          className={`${control} w-[168px]`}
        />
        <button
          disabled={!canSave}
          onClick={() => {
            onSave(name.trim(), incModels, incDictation, incSpeech, incVocab, incHotkeys);
            setName("");
            setOpen(false);
          }}
          className="text-[10px] px-2.5 py-1 rounded-md bg-[rgba(251,191,36,0.12)] border border-[rgba(251,191,36,0.3)] text-[#fbbf24] disabled:opacity-40"
        >
          {t("scen.saved.save")}
        </button>
      </div>
      <div className="flex flex-wrap items-center justify-end gap-x-3 gap-y-1.5">
        <IncludeCheck v={incModels} set={setIncModels} label={t("scen.section.models")} />
        <IncludeCheck v={incDictation} set={setIncDictation} label={t("scen.section.dictation")} />
        <IncludeCheck v={incSpeech} set={setIncSpeech} label={t("scen.section.speech")} />
        <IncludeCheck v={incVocab} set={setIncVocab} label={t("scen.section.vocab")} />
        <IncludeCheck v={incHotkeys} set={setIncHotkeys} label={t("scen.section.hotkeys")} />
      </div>
    </div>
  );
}

// ── main tab ───────────────────────────────────────────────────────────────────

export default function ScenariosTab({
  config,
  modes,
  onReload,
  setError,
}: {
  config: AppConfig;
  modes: ModeEntry[];
  onReload: () => void;
  setError: (e: string) => void;
}) {
  useT();
  const [showTemplates, setShowTemplates] = useState(false);
  const [applied, setApplied] = useState("");

  const savedScenarios = config.saved_scenarios ?? [];

  const onApply = async (id: string, name: string) => {
    setError("");
    setApplied("");
    try {
      await applySavedScenario(id);
      // Refetch config + modes so dictation-dependent UI reflects the new modes.
      onReload();
      setApplied(td("scen.saved.applied", [name]));
    } catch (e) {
      setError(errStr(e));
    }
  };

  return (
    <div className="flex flex-col gap-5">
      {showTemplates && (
        <Scenarios
          mode="overlay"
          onDone={() => {
            setShowTemplates(false);
            onReload();
          }}
        />
      )}

      {/* a. Current configuration overview */}
      <OverviewCard config={config} modes={modes} />

      {/* b. Saved scenarios */}
      <div className="flex flex-col gap-2.5">
        <div className="flex items-center gap-2">
          <span className="text-[11px] uppercase tracking-wider text-[rgba(255,255,255,0.35)]">
            {t("scen.saved.title")}
          </span>
          <div className="ml-auto">
            <SaveCurrent
              onSave={async (name, m, d, s, v, h) => {
                setError("");
                try {
                  await saveScenario(name, m, d, s, v, h);
                  onReload();
                } catch (e) {
                  setError(errStr(e));
                }
              }}
            />
          </div>
        </div>

        {applied && <div className="text-[10px] text-[#4ade80]">{applied}</div>}

        {savedScenarios.length === 0 ? (
          <div className="text-[10.5px] text-[rgba(255,255,255,0.3)]">{t("scen.saved.empty")}</div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {savedScenarios.map((s) => (
              <SavedRow
                key={s.id}
                scenario={s}
                onApply={onApply}
                onDelete={async (id) => {
                  setError("");
                  try {
                    await deleteSavedScenario(id);
                    onReload();
                  } catch (e) {
                    setError(errStr(e));
                  }
                }}
              />
            ))}
          </div>
        )}

        <ImportZone onImported={onReload} />
      </div>

      {/* c. Setup templates */}
      <div className="pt-4 border-t border-[rgba(255,255,255,0.06)] flex flex-col gap-2">
        <span className="text-[11px] uppercase tracking-wider text-[rgba(255,255,255,0.35)]">
          {t("scen.tab.templates")}
        </span>
        <p className="text-[10.5px] text-[rgba(255,255,255,0.35)]">{t("scen.tab.template.desc")}</p>
        <button
          onClick={() => setShowTemplates(true)}
          className="self-start text-[11px] px-3 py-1.5 rounded-md border border-[rgba(251,191,36,0.25)] bg-[rgba(251,191,36,0.08)] text-[#fbbf24] hover:bg-[rgba(251,191,36,0.14)] transition-colors"
        >
          {t("scen.tab.template.btn")}
        </button>
      </div>
    </div>
  );
}
