import { useState, useEffect } from "react";
import Dictation from "./views/Dictation";
import Conversation from "./views/Conversation";
import Stats from "./views/Stats";
import Settings from "./views/Settings";
import History from "./views/History";
import type { HistoryFilter } from "./views/History";
import Onboarding, { isSttConfigured } from "./views/Onboarding";
import { getConfig } from "./lib/api";
import { useT, setLocale, resolveLocale, type TKey } from "./lib/i18n";

type Tab = "dictation" | "voice" | "history" | "stats" | "settings";

const NAV_ITEMS: { id: Tab; label: TKey; icon: React.ReactNode }[] = [
  {
    id: "dictation",
    label: "nav.dictation",
    icon: (
      <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
        <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
        <line x1="12" y1="19" x2="12" y2="23" />
      </svg>
    ),
  },
  {
    id: "history",
    label: "nav.history",
    icon: (
      <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 8v4l3 3" />
        <circle cx="12" cy="12" r="10" />
      </svg>
    ),
  },
  {
    id: "voice",
    label: "nav.talk",
    icon: (
      <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z" />
        <line x1="9" y1="10" x2="9" y2="13" />
        <line x1="12" y1="8" x2="12" y2="15" />
        <line x1="15" y1="10" x2="15" y2="13" />
      </svg>
    ),
  },
  {
    id: "stats",
    label: "nav.stats",
    icon: (
      <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <path d="M18 20V10" />
        <path d="M12 20V4" />
        <path d="M6 20v-6" />
      </svg>
    ),
  },
];

// Gear icon for settings
const SETTINGS_ICON = (
  <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
    <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
    <circle cx="12" cy="12" r="3" />
  </svg>
);

// Sidebar toggle — soft rounded rectangle
const SIDEBAR_ICON = (
  <svg width={13} height={13} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
    <rect x="3" y="3" width="18" height="18" rx="4" />
    <path d="M9 3v18" />
  </svg>
);

export default function App() {
  const t = useT();
  const [activeTab, setActiveTab] = useState<Tab>("dictation");
  const [collapsed, setCollapsed] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  // First-run wizard gate. Stays false while loading and in non-Tauri/demo
  // environments (where getConfig throws) so the shell renders unchanged.
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [historyPreset, setHistoryPreset] = useState<{ filter: HistoryFilter; nonce: number }>();
  // Don't paint the shell until the gate is decided — otherwise a genuine
  // first run flashes the full app for a frame before the wizard mounts.
  const [gateReady, setGateReady] = useState(false);

  useEffect(() => {
    import("@tauri-apps/api/app").then((m) => m.getVersion()).then(setAppVersion).catch(() => {});
  }, []);

  useEffect(() => {
    getConfig()
      .then((cfg) => {
        setLocale(resolveLocale(cfg.ui_language));
        // Show the wizard only for genuinely-unconfigured first runs: the flag
        // is unset AND there's no usable STT config. Existing installs that
        // already configured models via Settings skip the wizard even with the
        // flag unset; skipping still persists the flag.
        if (!cfg.has_completed_onboarding && !isSttConfigured(cfg)) setShowOnboarding(true);
      })
      .catch(() => {})
      .finally(() => setGateReady(true));
  }, []);

  // Listen for navigation events from float pill / tray
  useEffect(() => {
    const cleanup: (() => void)[] = [];
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        cleanup.push(await listen<string>("navigate-tab", (event) => {
          const raw = typeof event.payload === "string" ? event.payload.replace(/"/g, "") : "";
          // Legacy tab names from the float pill / tray map into History filters.
          const historyMap: Record<string, HistoryFilter> = {
            recent: "all", search: "all", history: "all", notes: "note", meetings: "meeting",
          };
          if (raw in historyMap) {
            setHistoryPreset({ filter: historyMap[raw], nonce: Date.now() });
            setActiveTab("history");
          } else if (["dictation", "voice", "stats", "settings"].includes(raw)) {
            setActiveTab(raw as Tab);
          }
        }));
      } catch {
        // Not in Tauri environment
      }
    })();
    return () => { cleanup.forEach((fn) => fn()); };
  }, []);

  // First-run: render the wizard instead of the shell (after all hooks so the
  // hook order stays stable across the loading → onboarding transition).
  if (!gateReady) {
    return <div className="h-screen bg-[#1a1917]" />;
  }
  if (showOnboarding) {
    return <Onboarding onDone={() => setShowOnboarding(false)} />;
  }

  return (
    <div className="flex flex-col h-screen select-none bg-[#1a1917]">
      {/* macOS title bar — traffic lights at y≈7, 12px diameter, center y≈13 */}
      <div
        className="relative h-[38px] flex-shrink-0 bg-[#151413]"
        data-tauri-drag-region=""
      >
        <button
          onClick={() => setCollapsed(!collapsed)}
          className="absolute w-[20px] h-[20px] rounded-[5px] flex items-center justify-center hover:bg-[rgba(255,255,255,0.07)] transition-colors"
          style={{ stroke: "#ffffff", top: "6px", left: "88px" }}
          title={collapsed ? t("app.show-sidebar") : t("app.hide-sidebar")}
        >
          {/* Opaque stroke + wrapper alpha: overlapping strokes inside the
              glyph composite once, so joints don't render brighter. */}
          <span className="flex" style={{ opacity: 0.25 }}>{SIDEBAR_ICON}</span>
        </button>
      </div>

      {/* Main area: sidebar + content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <div
          className={[
            "flex-shrink-0 bg-[#151413] border-r border-[rgba(255,255,255,0.05)] flex flex-col items-center py-2 gap-0.5 transition-all duration-200",
            collapsed ? "w-0 overflow-hidden border-r-0 p-0" : "w-[54px]",
          ].join(" ")}
        >
          {/* App icon */}
          <div className="w-[30px] h-[30px] rounded-[9px] bg-gradient-to-br from-[#f59e0b] to-[#d97706] flex items-center justify-center mb-2 shadow-[0_2px_10px_rgba(245,158,11,0.25)] flex-shrink-0">
            <span className="text-[#1a1917] text-sm font-bold">f</span>
          </div>

          {/* Nav items */}
          <div role="tablist" data-testid="app-nav" className="flex flex-col items-center gap-0.5 w-full">
          {NAV_ITEMS.map((item) => (
            <button
              key={item.id}
              role="tab"
              aria-selected={activeTab === item.id}
              data-testid={`nav-${item.id}`}
              onClick={() => setActiveTab(item.id)}
              title={t(item.label)}
              className={[
                "w-[38px] h-[38px] rounded-[10px] flex items-center justify-center transition-colors flex-shrink-0",
                activeTab === item.id
                  ? "bg-[rgba(245,158,11,0.12)]"
                  : "hover:bg-[rgba(255,255,255,0.04)]",
              ].join(" ")}
              style={{
                stroke: activeTab === item.id ? "#fbbf24" : "#ffffff",
              }}
            >
              <span
                className="flex transition-opacity"
                style={{ opacity: activeTab === item.id ? 1 : 0.22 }}
              >
                {item.icon}
              </span>
            </button>
          ))}
          </div>

          {/* Spacer */}
          <div className="flex-1" />

          {/* Settings */}
          <button
            onClick={() => setActiveTab("settings")}
            title={t("nav.settings")}
            className={[
              "w-[38px] h-[38px] rounded-[10px] flex items-center justify-center transition-colors flex-shrink-0",
              activeTab === "settings"
                ? "bg-[rgba(245,158,11,0.12)]"
                : "hover:bg-[rgba(255,255,255,0.04)]",
            ].join(" ")}
            style={{
              stroke: activeTab === "settings" ? "#fbbf24" : "#ffffff",
            }}
          >
            <span
              className="flex transition-opacity"
              style={{ opacity: activeTab === "settings" ? 1 : 0.22 }}
            >
              {SETTINGS_ICON}
            </span>
          </button>

          {/* Version */}
          {appVersion && (
            <span className="text-[7px] text-[rgba(255,255,255,0.1)] mt-1 flex-shrink-0">v{appVersion}</span>
          )}
        </div>

        {/* Content area */}
        <div className="flex-1 overflow-hidden bg-[#1a1917]">
          {activeTab === "dictation" && <Dictation />}
          {activeTab === "voice" && <Conversation />}
          {activeTab === "stats" && <Stats />}
          {activeTab === "settings" && <Settings />}
          {activeTab === "history" && <History preset={historyPreset} />}
        </div>
      </div>
    </div>
  );
}
