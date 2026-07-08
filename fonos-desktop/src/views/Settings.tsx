// Settings view — shell that manages state and renders tab components.
// Tabbed layout: Workflows | General | Models | Speech | Vocab | Agent |
// Meeting | Hotkeys | Widgets | Scenarios (see TABS in settings/constants.ts,
// the canonical source of truth for the tab set).

import { useState, useEffect, useCallback } from "react";
import {
  getConfig,
  saveConfig,
  listModes,
} from "../lib/api";
import type { AppConfig, ModeEntry } from "../types";
import { t, useT } from "../lib/i18n";
import { TABS } from "./settings/constants";
import type { SettingsTab } from "./settings/constants";
import GeneralTab from "./settings/GeneralTab";
import ModelsTab from "./settings/ModelsTab";
import ScenariosTab from "./settings/ScenariosTab";
import WorkflowsTab from "./settings/WorkflowsTab";
import HotkeysTab from "./settings/HotkeysTab";
import WidgetsTab from "./settings/WidgetsTab";
import SpeechTab from "./settings/SpeechTab";
import VocabTab from "./settings/VocabTab";
import AgentTab from "./settings/AgentTab";
import SkillsTab from "./settings/SkillsTab";
import MeetingTab from "./settings/MeetingTab";

export default function Settings() {
  const tr = useT();
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [modes, setModes] = useState<ModeEntry[]>([]);
  const [saving, setSaving] = useState<boolean>(false);
  const [error, setError] = useState<string>("");

  // Tab state
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("workflows");

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

  // ── Loading state ───────────────────────────────────────────────────────────

  if (!config) {
    return (
      <div className="flex items-center justify-center h-full bg-[#1a1917]">
        <span className="text-[rgba(255,255,255,0.3)] text-sm">{t("settings.loading")}</span>
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
              {tr(t.label)}
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

        {/* ────────────── Scenarios tab (saved bundles + templates) ────────────── */}
        {settingsTab === "scenarios" && (
          <ScenariosTab
            config={config}
            modes={modes}
            onReload={loadAll}
            setError={setError}
          />
        )}

        {/* ────────────── Workflows tab (Workflow P1) ────────────── */}
        {settingsTab === "workflows" && (
          <WorkflowsTab />
        )}

        {/* ────────────── Speech tab (Listen + future STS) ────────────── */}
        {settingsTab === "speech" && (
          <SpeechTab config={config} modes={modes} onSave={handleSave} />
        )}

        {/* ────────────── Vocabulary tab ────────────── */}
        {settingsTab === "vocab" && (
          <VocabTab config={config} onSave={handleSave} />
        )}

        {/* ────────────── Agent tab (Agent config + Skills) ────────────── */}
        {settingsTab === "agent" && (
          <>
            <AgentTab config={config} onSave={handleSave} />
            <div style={{ borderTop: "1px solid rgba(255,255,255,0.04)", marginTop: 16, paddingTop: 8 }} />
            <SkillsTab />
          </>
        )}

        {/* ────────────── Meeting tab ────────────── */}
        {settingsTab === "meeting" && (
          <MeetingTab config={config} onSave={handleSave} />
        )}

        {/* ────────────── Hotkeys tab ────────────── */}
        {settingsTab === "hotkeys" && (
          <HotkeysTab config={config} onSave={handleSave} />
        )}

        {/* ────────────── Widgets tab (Workflow P1) ────────────── */}
        {settingsTab === "widgets" && (
          <WidgetsTab config={config} />
        )}

        {saving && (
          <div className="text-center text-[rgba(255,255,255,0.25)] text-[10px] mt-3">
            {t("settings.saving")}
          </div>
        )}
      </div>
    </div>
  );
}
