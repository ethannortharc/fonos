// Settings view — shell that manages state and renders tab components.
// Tabbed layout: Models | Modes | Hotkeys | Language

import { useState, useEffect, useCallback } from "react";
import {
  getConfig,
  saveConfig,
  listModes,
  saveCustomMode,
  deleteCustomMode,
} from "../lib/api";
import type { AppConfig, ModeEntry } from "../types";
import { TABS } from "./settings/constants";
import type { SettingsTab, ModeForm } from "./settings/constants";
import GeneralTab from "./settings/GeneralTab";
import ModelsTab from "./settings/ModelsTab";
import ModesTab from "./settings/ModesTab";
import HotkeysTab from "./settings/HotkeysTab";
import AgentTab from "./settings/AgentTab";
import SkillsTab from "./settings/SkillsTab";
import NotesTab from "./settings/NotesTab";
import MeetingTab from "./settings/MeetingTab";

export default function Settings() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [modes, setModes] = useState<ModeEntry[]>([]);
  const [saving, setSaving] = useState<boolean>(false);
  const [error, setError] = useState<string>("");

  // Tab state
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("general");

  const loadAll = useCallback(async () => {
    try {
      const [cfg, modeList] = await Promise.all([getConfig(), listModes()]);
      setConfig(cfg);
      setModes(modeList);
    } catch (e: unknown) {
      // Fall back to defaults so the UI still renders without a Tauri backend
      setConfig((prev) => prev ?? {
        hotkey_dictation: "cmd+shift+space",
        hotkey_tts: "cmd+shift+s",
        dictation_mode: "raw",
        default_voice: "default",
        tts_speed: 1.0,
        audio_input_device: "default",
        audio_output_device: "default",
        show_floating_indicator: true,
        stt_language: "auto",
        model_profiles: [],
        stt_profile: "",
        tts_profile: "",
        llm_profile: "",
        clean_prompt: "",
        translate_source: "auto",
        translate_target: "English",
      });
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    loadAll();
  }, [loadAll]);

  const handleSave = useCallback(
    async (updates: Partial<AppConfig>) => {
      if (!config) return;
      setSaving(true);
      setError("");
      try {
        await saveConfig(JSON.stringify(updates));
        setConfig({ ...config, ...updates });
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setSaving(false);
      }
    },
    [config]
  );

  const handleSaveMode = useCallback(
    async (form: ModeForm) => {
      if (!form.id || !form.name) {
        setError("Mode ID and name are required");
        return;
      }
      setError("");
      try {
        await saveCustomMode({
          id: form.id,
          name: form.name,
          description: form.description,
          icon: form.icon,
          system: form.system,
          user_template: form.user_template,
          temperature: form.temperature,
          model: form.model,
          stt_model: form.stt_model,
          stt_prompt: form.stt_prompt,
          stt_temperature: form.stt_temperature,
          max_tokens: form.max_tokens,
          output_language: form.output_language,
          auto_paste: form.auto_paste,
          auto_press_enter: form.auto_press_enter,
        });
        loadAll();
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [loadAll]
  );

  const handleDeleteMode = useCallback(
    async (id: string) => {
      try {
        await deleteCustomMode(id);
        loadAll();
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [loadAll]
  );

  // ── Loading state ───────────────────────────────────────────────────────────

  if (!config) {
    return (
      <div className="flex items-center justify-center h-full bg-[#1a1917]">
        <span className="text-[rgba(255,255,255,0.3)] text-sm">Loading...</span>
      </div>
    );
  }

  // ── Render ──────────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-full overflow-auto bg-[#1a1917]">
      <div className="flex flex-col p-4">
        {/* Error */}
        {error && (
          <div className="rounded-lg bg-red-500/10 border border-red-500/20 p-3 mb-3">
            <p className="text-red-400 text-xs">{error}</p>
          </div>
        )}

        {/* Tab bar */}
        <div className="flex gap-0 border-b border-[rgba(255,255,255,0.06)] mb-3">
          {TABS.map((t) => (
            <button
              key={t.key}
              onClick={() => setSettingsTab(t.key)}
              className={[
                "py-2 px-3.5 text-[11px] font-medium cursor-pointer transition-colors",
                settingsTab === t.key
                  ? "text-[#fbbf24] border-b-2 border-[#fbbf24]"
                  : "text-[rgba(255,255,255,0.3)]",
              ].join(" ")}
            >
              {t.label}
            </button>
          ))}
        </div>

        {/* ────────────── General tab ────────────── */}
        {settingsTab === "general" && (
          <GeneralTab config={config} onSave={handleSave} />
        )}

        {/* ────────────── Models tab ────────────── */}
        {settingsTab === "models" && (
          <ModelsTab
            config={config}
            onSave={handleSave}
            setError={setError}
          />
        )}

        {/* ────────────── Dictation tab (Modes) ────────────── */}
        {settingsTab === "dictation" && (
          <ModesTab
            config={config}
            modes={modes}
            onSaveMode={handleSaveMode}
            onDeleteMode={handleDeleteMode}
          />
        )}

        {/* ────────────── Agent tab (Agent config + Skills) ────────────── */}
        {settingsTab === "agent" && (
          <>
            <AgentTab config={config} onSave={handleSave} />
            <div style={{ borderTop: "1px solid rgba(255,255,255,0.04)", marginTop: 16, paddingTop: 8 }} />
            <SkillsTab />
          </>
        )}

        {/* ────────────── Notes tab ────────────── */}
        {settingsTab === "notes" && (
          <NotesTab config={config} onSave={handleSave} />
        )}

        {/* ────────────── Meeting tab ────────────── */}
        {settingsTab === "meeting" && (
          <MeetingTab config={config} onSave={handleSave} />
        )}

        {/* ────────────── Hotkeys tab ────────────── */}
        {settingsTab === "hotkeys" && (
          <HotkeysTab config={config} onSave={handleSave} />
        )}

        {saving && (
          <div className="text-center text-[rgba(255,255,255,0.25)] text-[10px] mt-3">
            Saving...
          </div>
        )}
      </div>
    </div>
  );
}
