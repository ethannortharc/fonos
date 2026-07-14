// General settings — microphone selection + per-app text-insertion overrides.
// STT language, translate target, and the default insertion strategy used to
// live here as globals; they moved onto per-widget props in P2 (see the stt
// and insert cases in WidgetForm.tsx) — only per-app overrides remain here.

import { useCallback, useEffect, useState, type ReactNode } from "react";
import type { Update, DownloadEvent } from "@tauri-apps/plugin-updater";
import { useT, td, setLocale, resolveLocale } from "../../lib/i18n";
import type { AppConfig } from "../../types";
import MicrophonePicker from "./MicrophonePicker";
import DoctorCard from "./DoctorCard";

// ── Updates (in-app auto-update) ──────────────────────────────────────────────
// Compact section: current version + a state-driven control. On mount it runs a
// silent check(); the plugin/IPC is absent in the browser demo, so every plugin
// call is dynamically imported and wrapped in try/catch — a caught failure just
// falls back to the version + a manual "Check for updates" button.
//
// "manual" is Linux deb/rpm: the updater plugin can find an update there just
// fine, but downloadAndInstall() can't swap a package-manager install in
// place (only an AppImage supports that — see tauri.linux.conf.json). The
// backend's update_supports_self_install() distinguishes the two cases so
// this only offers the one-click button when it'll actually work.
const RELEASES_URL = "https://github.com/ethannortharc/fonos/releases/latest";

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "none" }
  | { kind: "available"; version: string; update: Update }
  | { kind: "manual"; version: string }
  | { kind: "downloading"; percent: number | null }
  | { kind: "installing" }
  | { kind: "error" };

function UpdatesSection() {
  const t = useT();
  const [version, setVersion] = useState("");
  const [state, setState] = useState<UpdateState>({ kind: "idle" });

  // Current version — best-effort (absent in the browser demo without IPC).
  useEffect(() => {
    import("@tauri-apps/api/app")
      .then((m) => m.getVersion())
      .then(setVersion)
      .catch(() => setVersion(""));
  }, []);

  // One check(); `silent` swallows failures (browser demo / no plugin) so the
  // section still renders gracefully as version + a manual check button.
  const check = useCallback(async (silent: boolean) => {
    setState({ kind: "checking" });
    try {
      const { check: checkForUpdate } = await import("@tauri-apps/plugin-updater");
      const update = await checkForUpdate();
      if (!update) {
        setState({ kind: "none" });
        return;
      }
      // Default to self-installable (true) so macOS/Windows and the browser
      // demo (no IPC — invoke() throws) behave exactly as before this check
      // was added.
      let selfInstall = true;
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        selfInstall = await invoke<boolean>("update_supports_self_install");
      } catch {
        // ignore — fall back to true
      }
      setState(
        selfInstall
          ? { kind: "available", version: update.version, update }
          : { kind: "manual", version: update.version }
      );
    } catch {
      setState(silent ? { kind: "idle" } : { kind: "error" });
    }
  }, []);

  // Silent check on mount.
  useEffect(() => {
    void check(true);
  }, [check]);

  const install = useCallback(async (update: Update) => {
    setState({ kind: "downloading", percent: null });
    try {
      let total = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event: DownloadEvent) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            setState({ kind: "downloading", percent: total ? 0 : null });
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            setState({
              kind: "downloading",
              percent: total ? Math.round((downloaded / total) * 100) : null,
            });
            break;
          case "Finished":
            setState({ kind: "installing" });
            break;
        }
      });
      setState({ kind: "installing" });
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch {
      setState({ kind: "error" });
    }
  }, []);

  const checkButton = (
    <button
      onClick={() => void check(false)}
      className="text-[10px] px-2.5 py-1 rounded-lg border border-[rgba(255,255,255,0.07)] bg-[rgba(255,255,255,0.03)] text-[rgba(255,255,255,0.55)] hover:text-[rgba(255,255,255,0.8)] transition-colors"
    >
      {t("general.update.check")}
    </button>
  );

  let control: ReactNode;
  switch (state.kind) {
    case "checking":
      control = (
        <span className="text-[10.5px] text-[rgba(255,255,255,0.32)]">
          {t("general.update.checking")}
        </span>
      );
      break;
    case "available":
      control = (
        <>
          <span className="text-[10.5px] text-[var(--accent)]">
            {td("general.update.available", [state.version])}
          </span>
          <button
            onClick={() => void install(state.update)}
            className="text-[10px] px-2.5 py-1 rounded-lg border border-[rgba(242,184,75,0.3)] bg-[rgba(242,184,75,0.12)] text-[var(--accent)] hover:bg-[rgba(242,184,75,0.18)] transition-colors"
          >
            {t("general.update.update")}
          </button>
        </>
      );
      break;
    case "manual":
      control = (
        <>
          <span className="text-[10.5px] text-[var(--accent)]">
            {td("general.update.manual", [state.version])}
          </span>
          <a
            href={RELEASES_URL}
            target="_blank"
            rel="noreferrer noopener"
            className="text-[10px] px-2.5 py-1 rounded-lg border border-[rgba(242,184,75,0.3)] bg-[rgba(242,184,75,0.12)] text-[var(--accent)] hover:bg-[rgba(242,184,75,0.18)] transition-colors"
          >
            {t("general.update.download")}
          </a>
        </>
      );
      break;
    case "downloading":
      control = (
        <span className="text-[10.5px] text-[rgba(255,255,255,0.32)] tabular-nums">
          {t("general.update.downloading")}
          {state.percent !== null ? ` ${state.percent}%` : ""}
        </span>
      );
      break;
    case "installing":
      control = (
        <span className="text-[10.5px] text-[rgba(255,255,255,0.32)]">
          {t("general.update.installing")}
        </span>
      );
      break;
    case "none":
      control = (
        <>
          <span className="text-[10.5px] text-[rgba(255,255,255,0.32)]">
            {t("general.update.uptodate")}
          </span>
          {checkButton}
        </>
      );
      break;
    case "error":
      control = (
        <>
          <span className="text-[10.5px] text-[#f87171]">{t("general.update.error")}</span>
          {checkButton}
        </>
      );
      break;
    default:
      control = checkButton;
      break;
  }

  return (
    <div className="flex items-center justify-between gap-4">
      <div>
        <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">
          {t("general.update.title")}
        </div>
        <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
          {t("general.update.current")}
          {version ? `: ${version}` : ""}
        </div>
      </div>
      <div className="flex items-center gap-2 flex-shrink-0">{control}</div>
    </div>
  );
}

export default function GeneralTab({
  config,
  onSave,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  const t = useT();

  return (
    <div className="flex flex-col gap-5">
      {/* ── Setup Doctor (resident config-health card) ── */}
      <DoctorCard />

      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Updates (in-app auto-update) ── */}
      <UpdatesSection />

      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Interface language ── */}
      <div className="flex flex-col gap-2">
        <div className="text-[12px] font-medium text-[#fafaf9]">{t("general.language")}</div>
        <div className="flex gap-1.5">
          {([["auto", t("general.language.auto")], ["en", t("general.language.en")], ["zh", t("general.language.zh")]] as const).map(([val, label]) => (
            <button
              key={val}
              onClick={() => {
                onSave({ ui_language: val });
                setLocale(resolveLocale(val));
              }}
              className={[
                "px-3 py-1.5 rounded-lg text-[10.5px] border transition-all",
                (config.ui_language ?? "auto") === val
                  ? "bg-[rgba(242,184,75,0.12)] border-[rgba(242,184,75,0.3)] text-[var(--accent)]"
                  : "bg-[rgba(255,255,255,0.02)] border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.7)]",
              ].join(" ")}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Microphone ── */}
      <MicrophonePicker
        value={config.audio_input_device}
        onSelect={(name) => onSave({ audio_input_device: name })}
      />

      {/* Divider */}
      <div className="border-t border-[rgba(255,255,255,0.04)]" />

      {/* ── Model warm-up ── */}
      <div className="flex items-center justify-between gap-4">
        <div>
          <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">{t("general.warmup.title")}</div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
            {t("general.warmup.desc")}
          </div>
        </div>
        <button
          onClick={() => onSave({ warmup_enabled: !(config.warmup_enabled ?? true) })}
          className={[
            "px-2.5 py-1.5 rounded-lg text-[10px] transition-all flex-shrink-0",
            (config.warmup_enabled ?? true)
              ? "bg-[rgba(74,222,128,0.1)] border border-[rgba(74,222,128,0.2)] text-[#4ade80]"
              : "bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.3)]",
          ].join(" ")}
        >
          {(config.warmup_enabled ?? true) ? t("common.enabled") : t("common.disabled")}
        </button>
      </div>
    </div>
  );
}
