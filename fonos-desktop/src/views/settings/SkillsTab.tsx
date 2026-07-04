// Skills management tab — list, toggle, create, test, and import skills.

import { useState, useEffect, useCallback } from "react";
import { t, useT } from "../../lib/i18n";
import type { SkillInfo } from "../../types";
import {
  listSkills,
  toggleSkill,
  saveCustomSkill,
  getCustomSkill,
  deleteCustomSkill,
  testSkill,
} from "../../lib/api";

// ─── Type badge ───────────────────────────────────────────────────────────────

function TypeBadge({ skillType }: { skillType: string }) {
  let bg: string;
  let text: string;
  if (skillType === "shell" || skillType === "native") {
    bg = "rgba(134,239,172,0.08)";
    text = "rgba(134,239,172,0.6)";
  } else if (skillType === "http") {
    bg = "rgba(245,158,11,0.1)";
    text = "rgba(251,191,36,0.7)";
  } else {
    // script or unknown
    bg = "rgba(196,181,253,0.08)";
    text = "rgba(196,181,253,0.7)";
  }
  return (
    <span
      className="text-[8px] px-1.5 py-0.5 rounded font-medium"
      style={{ background: bg, color: text }}
    >
      {skillType}
    </span>
  );
}

// ─── Toggle switch ────────────────────────────────────────────────────────────

function Toggle({
  enabled,
  onChange,
}: {
  enabled: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div
      onClick={() => onChange(!enabled)}
      className="w-8 h-[18px] rounded-full relative cursor-pointer flex-shrink-0 transition-colors"
      style={{
        background: enabled ? "rgba(245,158,11,0.3)" : "rgba(255,255,255,0.06)",
      }}
    >
      <div
        className="absolute top-[2px] w-[14px] h-[14px] rounded-full transition-all duration-150"
        style={{
          background: enabled ? "#fbbf24" : "rgba(255,255,255,0.2)",
          left: enabled ? undefined : "2px",
          right: enabled ? "2px" : undefined,
        }}
      />
    </div>
  );
}

// ─── Skill icon from type ─────────────────────────────────────────────────────

function skillIcon(skillType: string, name: string): string {
  const n = name.toLowerCase();
  if (n.includes("shell") || skillType === "shell") return "\u{1F4BB}";
  if (n.includes("apple") || n.includes("script")) return "\u{1F34E}";
  if (n.includes("app") || n.includes("control")) return "\u{1F4F1}";
  if (n.includes("clipboard")) return "\u{1F4CB}";
  if (n.includes("system") || n.includes("info")) return "\u{1F5A5}";
  if (skillType === "http") return "\u{1F310}";
  if (skillType === "script") return "\u{1F4DC}";
  return "\u{2699}\u{FE0F}";
}

// ─── Parameter row form ───────────────────────────────────────────────────────

interface ParamRow {
  name: string;
  description: string;
  defaultVal: string;
}

// ─── Skill creation form ──────────────────────────────────────────────────────

interface SkillForm {
  icon: string;
  id: string;
  name: string;
  description: string;
  skillType: "shell" | "http" | "script";
  command: string;
  params: ParamRow[];
  responseTemplate: string;
}

const EMPTY_SKILL_FORM: SkillForm = {
  icon: "\u2728",
  id: "",
  name: "",
  description: "",
  skillType: "shell",
  command: "",
  params: [],
  responseTemplate: "{output}",
};

function commandLabel(skillType: string): string {
  if (skillType === "http") return t("skills.cmd.url");
  if (skillType === "script") return t("skills.cmd.script");
  return t("skills.cmd.command");
}

function commandPlaceholder(skillType: string): string {
  if (skillType === "http") return "https://example.com/{param}";
  if (skillType === "script") return "/path/to/script.py";
  return "echo {input}";
}

// ─── Create/Edit Form ─────────────────────────────────────────────────────────

function SkillForm({
  initial,
  onSave,
  onCancel,
}: {
  initial: SkillForm;
  onSave: (form: SkillForm) => Promise<void>;
  onCancel: () => void;
}) {
  const [form, setForm] = useState<SkillForm>(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const handleSave = async () => {
    if (!form.id.trim()) { setError(t("skills.err-id")); return; }
    if (!form.name.trim()) { setError(t("skills.err-name")); return; }
    setError("");
    setSaving(true);
    try {
      await onSave(form);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const addParam = () => {
    setForm({ ...form, params: [...form.params, { name: "", description: "", defaultVal: "" }] });
  };

  const updateParam = (idx: number, field: keyof ParamRow, val: string) => {
    const params = form.params.map((p, i) => (i === idx ? { ...p, [field]: val } : p));
    setForm({ ...form, params });
  };

  const removeParam = (idx: number) => {
    setForm({ ...form, params: form.params.filter((_, i) => i !== idx) });
  };

  return (
    <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-3">
      <div className="text-[12px] font-medium text-[#fafaf9]">{t("skills.new")}</div>

      {error && (
        <div className="text-[10px] text-red-400 bg-red-500/10 border border-red-500/20 rounded-lg px-3 py-2">
          {error}
        </div>
      )}

      {/* Identity */}
      <div className="flex flex-col gap-2">
        <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{t("skills.identity")}</div>
        <div className="grid grid-cols-[48px_1fr] gap-2">
          <input
            type="text"
            value={form.icon}
            onChange={(e) => setForm({ ...form, icon: e.target.value })}
            className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2 py-2 text-center text-[16px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
            title={t("skills.emoji-title")}
          />
          <input
            type="text"
            value={form.id}
            onChange={(e) => setForm({ ...form, id: e.target.value })}
            placeholder="skill-id"
            className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] font-mono focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
          />
        </div>
        <input
          type="text"
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          placeholder={t("skills.ph.name")}
          className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
        />
        <input
          type="text"
          value={form.description}
          onChange={(e) => setForm({ ...form, description: e.target.value })}
          placeholder={t("skills.ph.desc")}
          className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[12px] focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
        />
      </div>

      {/* Type picker */}
      <div className="flex flex-col gap-2">
        <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{t("skills.type")}</div>
        <div className="flex gap-1.5">
          {(["shell", "http", "script"] as const).map((t) => (
            <button
              key={t}
              onClick={() => setForm({ ...form, skillType: t })}
              className="px-3 py-1.5 rounded-lg text-[11px] transition-colors"
              style={
                form.skillType === t
                  ? { background: "rgba(245,158,11,0.08)", border: "1px solid rgba(245,158,11,0.15)", color: "#fbbf24", fontWeight: 500 }
                  : { background: "rgba(255,255,255,0.03)", border: "1px solid rgba(255,255,255,0.06)", color: "rgba(255,255,255,0.3)" }
              }
            >
              {t}
            </button>
          ))}
        </div>
      </div>

      {/* Command / URL / Script */}
      <div className="flex flex-col gap-2">
        <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{commandLabel(form.skillType)}</div>
        <input
          type="text"
          value={form.command}
          onChange={(e) => setForm({ ...form, command: e.target.value })}
          placeholder={commandPlaceholder(form.skillType)}
          className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] font-mono focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
        />
      </div>

      {/* Parameters */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{t("skills.parameters")}</div>
          <button
            onClick={addParam}
            className="text-[9px] text-[rgba(251,191,36,0.4)] hover:text-[#fbbf24] transition-colors"
          >
            {t("skills.add")}
          </button>
        </div>
        {form.params.length > 0 && (
          <div className="rounded-lg p-2.5 flex flex-col gap-1.5" style={{ background: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.03)" }}>
            {form.params.map((param, idx) => (
              <div key={idx} className="grid grid-cols-[1fr_1fr_1fr_auto] gap-1.5 items-center">
                <input
                  type="text"
                  value={param.name}
                  onChange={(e) => updateParam(idx, "name", e.target.value)}
                  placeholder={t("skills.ph.param-name")}
                  className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.05)] rounded px-2 py-1 text-[10px] text-[#fbbf24] font-mono focus:outline-none"
                />
                <input
                  type="text"
                  value={param.description}
                  onChange={(e) => updateParam(idx, "description", e.target.value)}
                  placeholder={t("skills.ph.param-desc")}
                  className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.05)] rounded px-2 py-1 text-[10px] text-[rgba(255,255,255,0.4)] focus:outline-none"
                />
                <input
                  type="text"
                  value={param.defaultVal}
                  onChange={(e) => updateParam(idx, "defaultVal", e.target.value)}
                  placeholder={t("skills.ph.param-default")}
                  className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.05)] rounded px-2 py-1 text-[10px] text-[rgba(255,255,255,0.3)] font-mono focus:outline-none"
                />
                <button
                  onClick={() => removeParam(idx)}
                  className="text-[rgba(255,255,255,0.1)] hover:text-[#ef4444] text-[9px] transition-colors px-1"
                >
                  {"\u2715"}
                </button>
              </div>
            ))}
          </div>
        )}
        {form.params.length === 0 && (
          <div className="text-[9px] text-[rgba(255,255,255,0.15)] italic">{t("skills.no-params")}</div>
        )}
      </div>

      {/* Response template */}
      <div className="flex flex-col gap-2">
        <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{t("skills.response-template")}</div>
        <input
          type="text"
          value={form.responseTemplate}
          onChange={(e) => setForm({ ...form, responseTemplate: e.target.value })}
          placeholder={t("skills.ph.response")}
          className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[11px] font-mono focus:outline-none focus:border-[rgba(245,158,11,0.3)]"
        />
        <div className="text-[9px] text-[rgba(255,255,255,0.12)] italic">
          {t("skills.response-hint")}
        </div>
      </div>

      {/* Actions */}
      <div className="flex gap-2 pt-1">
        <button
          onClick={handleSave}
          disabled={saving}
          className="flex-1 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-[#1a1917] text-[12px] font-semibold hover:opacity-90 transition-opacity disabled:opacity-50"
        >
          {saving ? t("skills.creating") : t("skills.create")}
        </button>
        <button
          onClick={onCancel}
          className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
        >
          {t("common.cancel")}
        </button>
      </div>
    </div>
  );
}

// ─── Test panel ───────────────────────────────────────────────────────────────

function TestPanel({ skillId }: { skillId: string }) {
  const [input, setInput] = useState("");
  const [output, setOutput] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [err, setErr] = useState("");

  const run = async () => {
    setRunning(true);
    setErr("");
    setOutput(null);
    try {
      const result = await testSkill(skillId, input);
      setOutput(result);
    } catch (e: unknown) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="flex flex-col gap-2 pt-2 mt-1" style={{ borderTop: "1px solid rgba(255,255,255,0.03)" }}>
      <div className="flex items-center gap-2">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") run(); }}
          placeholder={t("skills.ph.test-input")}
          className="flex-1 bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.05)] rounded px-2.5 py-1.5 text-[10px] text-[#fafaf9] focus:outline-none focus:border-[rgba(245,158,11,0.2)]"
        />
        <button
          onClick={run}
          disabled={running}
          className="px-2.5 py-1.5 rounded text-[10px] text-[rgba(255,255,255,0.5)] bg-[rgba(255,255,255,0.04)] border border-[rgba(255,255,255,0.06)] hover:text-[rgba(255,255,255,0.7)] transition-colors disabled:opacity-50"
        >
          {running ? "..." : t("skills.run")}
        </button>
      </div>
      {err && (
        <div className="text-[9px] text-red-400 font-mono px-1">{err}</div>
      )}
      {output !== null && (
        <div className="rounded px-2.5 py-2 text-[10px] text-[rgba(255,255,255,0.5)] font-mono leading-relaxed break-all"
             style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.03)" }}>
          {output || <span className="text-[rgba(255,255,255,0.2)] italic">{t("skills.empty-output")}</span>}
        </div>
      )}
    </div>
  );
}

// ─── Import panel ─────────────────────────────────────────────────────────────

function ImportPanel({
  onImported,
  onCancel,
}: {
  onImported: () => void;
  onCancel: () => void;
}) {
  const [json, setJson] = useState("");
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState("");
  const [preview, setPreview] = useState<Record<string, unknown> | null>(null);

  const handlePreview = () => {
    setError("");
    setPreview(null);
    try {
      const parsed = JSON.parse(json) as Record<string, unknown>;
      setPreview(parsed);
    } catch {
      setError(t("skills.err-json"));
    }
  };

  const handleImport = async () => {
    setError("");
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(json) as Record<string, unknown>;
    } catch {
      setError(t("skills.err-json-short"));
      return;
    }
    if (!parsed.id || !parsed.name) {
      setError(t("skills.err-json-fields"));
      return;
    }
    setImporting(true);
    try {
      await saveCustomSkill(JSON.stringify(parsed));
      onImported();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setImporting(false);
    }
  };

  return (
    <div className="rounded-[10px] bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] p-4 flex flex-col gap-3">
      <div className="text-[12px] font-medium text-[#fafaf9]">{t("skills.import-title")}</div>

      {error && (
        <div className="text-[10px] text-red-400 bg-red-500/10 border border-red-500/20 rounded-lg px-3 py-2">
          {error}
        </div>
      )}

      <div className="flex flex-col gap-1">
        <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{t("skills.json-label")}</div>
        <textarea
          value={json}
          onChange={(e) => { setJson(e.target.value); setPreview(null); }}
          placeholder={'{\n  "id": "my_skill",\n  "name": "My Skill",\n  ...\n}'}
          rows={6}
          className="bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-3 py-2 text-[#fafaf9] text-[10px] font-mono leading-relaxed focus:outline-none focus:border-[rgba(245,158,11,0.3)] resize-none"
        />
      </div>

      {preview && (
        <div className="rounded-lg p-2.5 flex flex-col gap-1" style={{ background: "rgba(134,239,172,0.04)", border: "1px solid rgba(134,239,172,0.08)" }}>
          <div className="text-[9px] uppercase tracking-wider text-[rgba(134,239,172,0.4)]">{t("skills.preview")}</div>
          <div className="text-[11px] text-[#fafaf9] font-medium">{String(preview.name ?? "")}</div>
          {preview.description != null && (
            <div className="text-[10px] text-[rgba(255,255,255,0.3)]">{String(preview.description)}</div>
          )}
          {preview.type != null && (
            <div><TypeBadge skillType={String(preview.type)} /></div>
          )}
        </div>
      )}

      <div className="flex gap-2">
        <button
          onClick={handlePreview}
          className="px-3 py-2 rounded-lg text-[11px] text-[rgba(255,255,255,0.4)] bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] hover:border-[rgba(255,255,255,0.1)] transition-colors"
        >
          {t("skills.validate")}
        </button>
        <button
          onClick={handleImport}
          disabled={importing}
          className="flex-1 py-2 rounded-lg bg-gradient-to-r from-[#f59e0b] to-[#d97706] text-[#1a1917] text-[12px] font-semibold hover:opacity-90 transition-opacity disabled:opacity-50"
        >
          {importing ? t("skills.importing") : t("skills.import")}
        </button>
        <button
          onClick={onCancel}
          className="px-4 py-2 rounded-lg bg-transparent border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.4)] text-[12px] hover:border-[rgba(255,255,255,0.1)] transition-colors"
        >
          {t("common.cancel")}
        </button>
      </div>
    </div>
  );
}

// ─── Skill card ───────────────────────────────────────────────────────────────

function SkillCard({
  skill,
  onToggle,
  onEdit,
  onDelete,
}: {
  skill: SkillInfo;
  onToggle: (enabled: boolean) => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const [showTest, setShowTest] = useState(false);

  return (
    <div
      className="rounded-[10px] p-3 flex flex-col gap-2 transition-colors"
      style={{
        background: "rgba(255,255,255,0.02)",
        border: "1px solid rgba(255,255,255,0.04)",
        opacity: skill.enabled ? 1 : 0.65,
      }}
    >
      <div className="flex items-center gap-2">
        {/* Icon */}
        <span className="text-[14px] flex-shrink-0" style={{ opacity: skill.enabled ? 1 : 0.4 }}>
          {skillIcon(skill.skill_type, skill.name)}
        </span>

        {/* Info */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span
              className="text-[12px] font-medium"
              style={{ color: skill.enabled ? "#fafaf9" : "rgba(255,255,255,0.35)" }}
            >
              {skill.name}
            </span>
            {skill.builtin && (
              <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded">
                {t("common.builtin")}
              </span>
            )}
            {skill.enabled && <TypeBadge skillType={skill.skill_type} />}
          </div>
          <div
            className="text-[10px] mt-0.5 truncate"
            style={{ color: skill.enabled ? "rgba(255,255,255,0.2)" : "rgba(255,255,255,0.12)" }}
          >
            {skill.description}
          </div>
        </div>

        {/* Skill actions */}
        <div className="flex items-center gap-1 flex-shrink-0" onClick={(e) => e.stopPropagation()}>
          {skill.builtin ? (
            <button
              onClick={onEdit}
              className="text-[rgba(255,255,255,0.2)] hover:text-[rgba(255,255,255,0.5)] text-[10px] px-1.5 transition-colors"
            >
              {t("skills.details")}
            </button>
          ) : (
            <>
              <button
                onClick={onEdit}
                className="text-[rgba(255,255,255,0.2)] hover:text-[rgba(255,255,255,0.5)] text-[10px] px-1.5 transition-colors"
              >
                {t("common.edit")}
              </button>
              <button
                onClick={onDelete}
                className="text-[rgba(255,255,255,0.12)] hover:text-[#ef4444] text-[10px] px-1 transition-colors"
              >
                {"\u2715"}
              </button>
            </>
          )}
        </div>

        {/* Toggle */}
        <Toggle enabled={skill.enabled} onChange={onToggle} />
      </div>

      {/* Test row */}
      <div style={{ borderTop: "1px solid rgba(255,255,255,0.03)" }} className="pt-1">
        <button
          onClick={() => setShowTest(!showTest)}
          className="text-[9px] text-[rgba(255,255,255,0.2)] hover:text-[rgba(255,255,255,0.4)] transition-colors"
        >
          {showTest ? t("skills.hide-test") : t("skills.test")}
        </button>
        {showTest && <TestPanel skillId={skill.id} />}
      </div>
    </div>
  );
}

// ─── SkillsTab ────────────────────────────────────────────────────────────────

export default function SkillsTab() {
  useT();
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [view, setView] = useState<"list" | "create" | "import">("list");
  const [editingSkill, setEditingSkill] = useState<SkillForm | null>(null);
  const [detailSkill, setDetailSkill] = useState<SkillInfo | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const list = await listSkills();
      setSkills(list);
    } catch (e: unknown) {
      // If backend isn't available, show empty list gracefully
      setSkills([]);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleToggle = async (id: string, enabled: boolean) => {
    // Optimistic update
    setSkills((prev) => prev.map((s) => (s.id === id ? { ...s, enabled } : s)));
    try {
      await toggleSkill(id, enabled);
    } catch (e: unknown) {
      // Revert on failure
      setSkills((prev) => prev.map((s) => (s.id === id ? { ...s, enabled: !enabled } : s)));
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteCustomSkill(id);
      setSkills((prev) => prev.filter((s) => s.id !== id));
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleCreate = async (form: SkillForm) => {
    // Build the skill JSON
    const params: Record<string, { description: string; default?: string }> = {};
    for (const p of form.params) {
      if (p.name.trim()) {
        params[p.name.trim()] = {
          description: p.description,
          ...(p.defaultVal ? { default: p.defaultVal } : {}),
        };
      }
    }
    const skillJson: Record<string, unknown> = {
      id: form.id.trim(),
      name: form.name.trim(),
      description: form.description,
      type: form.skillType,
      parameters: params,
      response_template: form.responseTemplate,
    };
    if (form.skillType === "shell") skillJson.command = form.command;
    else if (form.skillType === "http") skillJson.url = form.command;
    else skillJson.script = form.command;

    await saveCustomSkill(JSON.stringify(skillJson));
    setView("list");
    setEditingSkill(null);
    await load();
  };

  const handleImported = async () => {
    setView("list");
    await load();
  };

  // ── Render create / import forms ────────────────────────────────────────────

  // ── Built-in skill detail view ─────────────────────────────────────────────
  if (detailSkill) {
    return (
      <div className="flex flex-col gap-3">
        <div className="rounded-[10px] p-4 flex flex-col gap-3"
          style={{ background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.06)" }}>
          <div className="flex items-center gap-2.5">
            <span className="text-[18px]">{skillIcon(detailSkill.skill_type, detailSkill.name)}</span>
            <div>
              <div className="text-[13px] font-medium text-[#fafaf9]">{detailSkill.name}</div>
              <div className="flex items-center gap-2 mt-0.5">
                <TypeBadge skillType={detailSkill.skill_type} />
                <span className="text-[8px] text-[rgba(255,255,255,0.15)] bg-[rgba(255,255,255,0.04)] px-1.5 py-0.5 rounded">{t("common.builtin")}</span>
              </div>
            </div>
          </div>
          <div className="flex flex-col gap-2 pt-1" style={{ borderTop: "1px solid rgba(255,255,255,0.04)" }}>
            <div>
              <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] mb-1">{t("skills.desc-label")}</div>
              <div className="text-[11px] text-[rgba(255,255,255,0.5)] leading-relaxed">{detailSkill.description}</div>
            </div>
            <div>
              <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] mb-1">ID</div>
              <div className="text-[11px] text-[rgba(255,255,255,0.35)] font-mono">{detailSkill.id}</div>
            </div>
            <div>
              <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] mb-1">{t("skills.status-label")}</div>
              <div className="text-[11px]" style={{ color: detailSkill.enabled ? "rgba(134,239,172,0.7)" : "rgba(255,255,255,0.25)" }}>
                {detailSkill.enabled ? t("common.enabled") : t("common.disabled")}
              </div>
            </div>
            {detailSkill.parameters && detailSkill.parameters.length > 0 && (
              <div>
                <div className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)] mb-1">{t("skills.parameters")}</div>
                <div className="flex flex-col gap-1.5">
                  {detailSkill.parameters.map((p) => (
                    <div key={p.name} className="flex items-baseline gap-2 text-[10px]">
                      <span className="text-[#fbbf24] font-mono">{p.name}</span>
                      <span className="text-[rgba(255,255,255,0.25)]">{p.description}</span>
                      {p.required && <span className="text-[8px] text-[rgba(239,68,68,0.5)]">{t("skills.required")}</span>}
                      {p.default_value && <span className="text-[8px] text-[rgba(255,255,255,0.15)] font-mono">= {p.default_value}</span>}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
          <button
            onClick={() => setDetailSkill(null)}
            className="self-start mt-1 px-4 py-1.5 rounded-lg text-[11px] text-[rgba(255,255,255,0.4)] transition-colors hover:text-[rgba(255,255,255,0.6)]"
            style={{ border: "1px solid rgba(255,255,255,0.06)" }}
          >
            {t("common.back")}
          </button>
        </div>
      </div>
    );
  }

  if (view === "create" || editingSkill !== null) {
    return (
      <div className="flex flex-col gap-3">
        <SkillForm
          initial={editingSkill ?? EMPTY_SKILL_FORM}
          onSave={handleCreate}
          onCancel={() => { setView("list"); setEditingSkill(null); }}
        />
      </div>
    );
  }

  if (view === "import") {
    return (
      <div className="flex flex-col gap-3">
        <ImportPanel
          onImported={handleImported}
          onCancel={() => setView("list")}
        />
      </div>
    );
  }

  // ── Skill list ──────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col gap-2">
      {error && (
        <div className="rounded-lg bg-red-500/10 border border-red-500/20 p-3">
          <p className="text-red-400 text-[10px]">{error}</p>
        </div>
      )}

      {loading && (
        <div className="text-center text-[rgba(255,255,255,0.2)] text-[11px] py-4">
          {t("skills.loading")}
        </div>
      )}

      {!loading && skills.length === 0 && !error && (
        <div className="text-center text-[rgba(255,255,255,0.15)] text-[11px] py-6">
          {t("skills.empty")}
        </div>
      )}

      {skills.map((skill) => (
        <SkillCard
          key={skill.id}
          skill={skill}
          onToggle={(enabled) => handleToggle(skill.id, enabled)}
          onEdit={async () => {
            if (skill.builtin) {
              // Show read-only detail view for built-in skills
              setDetailSkill(skill);
              return;
            }
            // Load the full stored definition so the edit form keeps the real
            // command / params / template instead of overwriting them with blanks.
            try {
              const cfg = await getCustomSkill(skill.id);
              const skillType: SkillForm["skillType"] =
                cfg.skill_type === "http" || cfg.skill_type === "script"
                  ? cfg.skill_type
                  : "shell";
              const command =
                skillType === "http"
                  ? cfg.url ?? ""
                  : skillType === "script"
                  ? cfg.script ?? ""
                  : cfg.command ?? "";
              const params: ParamRow[] = Object.entries(cfg.parameters ?? {}).map(
                ([name, def]) => ({
                  name,
                  description: def?.description ?? "",
                  defaultVal: def?.default ?? "",
                })
              );
              setEditingSkill({
                icon: cfg.icon || skillIcon(cfg.skill_type, cfg.name),
                id: cfg.name,
                name: cfg.name,
                description: cfg.description,
                skillType,
                command,
                params,
                responseTemplate: cfg.response_template ?? "{output}",
              });
            } catch (e: unknown) {
              setError(e instanceof Error ? e.message : String(e));
            }
          }}
          onDelete={() => handleDelete(skill.id)}
        />
      ))}

      {/* Action buttons */}
      <div className="flex gap-2 mt-1">
        <button
          onClick={() => setView("create")}
          className="flex-1 py-2 rounded-[10px] border border-dashed border-[rgba(245,158,11,0.12)] text-[rgba(251,191,36,0.6)] text-[12px] hover:border-[rgba(245,158,11,0.25)] hover:text-[rgba(251,191,36,0.8)] transition-colors"
        >
          {t("skills.create-skill")}
        </button>
        <button
          onClick={() => setView("import")}
          className="px-4 py-2 rounded-[10px] border border-dashed border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.25)] text-[12px] hover:border-[rgba(255,255,255,0.1)] hover:text-[rgba(255,255,255,0.45)] transition-colors"
        >
          {t("skills.import")}
        </button>
      </div>
    </div>
  );
}
