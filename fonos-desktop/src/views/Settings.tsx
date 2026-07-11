// Settings view — shell that manages state and renders tab components.
// Tabbed layout: General | Advanced | Scenarios (see TABS in
// settings/constants.ts, the canonical source of truth for the tab set).
// Advanced absorbs Speech/Agent/Meeting + their hotkeys. Models, Vocabulary,
// and Flows (widgets/recipes) have been promoted to top-level pages under the
// Workbench-centered IA (see App.tsx NAV_ITEMS + views/Workbench.tsx).

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
import ScenariosTab from "./settings/ScenariosTab";
import AdvancedTab from "./settings/AdvancedTab";

export default function Settings() {
  const tr = useT();
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

  // ── Loading state ───────────────────────────────────────────────────────────

  if (!config) {
    return (
      <div className="flex items-center justify-center h-full bg-[var(--bg)]">
        <span className="text-[rgba(255,255,255,0.3)] text-sm">{t("settings.loading")}</span>
      </div>
    );
  }

  // ── Render ──────────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-full overflow-auto bg-[var(--bg)]">
      <div className="flex flex-col p-4">
        {/* Error */}
        {error && (
          <div className="rounded-lg bg-red-500/10 border border-red-500/20 p-3 mb-3">
            <p className="text-red-400 text-xs">{error}</p>
          </div>
        )}

        {/* Tab bar */}
        <div className="flex gap-1 border-b border-[rgba(255,255,255,0.07)] mb-3">
          {TABS.map((t) => (
            <button
              key={t.key}
              onClick={() => setSettingsTab(t.key)}
              className={[
                "relative py-1.5 px-3 text-[11px] font-medium rounded-t-[8px] cursor-pointer transition-colors",
                settingsTab === t.key
                  ? "text-[var(--accent)] border-b-2 border-[var(--accent)] bg-[rgba(240,173,50,0.045)]"
                  : "text-[var(--text-muted)] hover:text-[var(--text-secondary)] hover:bg-[rgba(255,255,255,0.025)]",
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

        {/* ────────────── Advanced tab (Speech + Agent + Meeting, Flows UI redesign) ────────────── */}
        {settingsTab === "advanced" && (
          <AdvancedTab config={config} modes={modes} onSave={handleSave} />
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

        {saving && (
          <div className="text-center text-[rgba(255,255,255,0.25)] text-[10px] mt-3">
            {t("settings.saving")}
          </div>
        )}
      </div>
    </div>
  );
}
