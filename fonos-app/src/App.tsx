import { useState, useEffect } from "react";
import Dictation from "./views/Dictation";
import Voice from "./views/Voice";
import Stats from "./views/Stats";
import Settings from "./views/Settings";
import Recent from "./views/Recent";
import Notes from "./views/Notes";
import Meetings from "./views/Meetings";

type Tab = "dictation" | "voice" | "recent" | "stats" | "settings" | "notes" | "meetings";

const NAV_ITEMS: { id: Tab; label: string; icon: React.ReactNode }[] = [
  {
    id: "dictation",
    label: "Dictation",
    icon: (
      <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" />
        <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
        <line x1="12" y1="19" x2="12" y2="23" />
      </svg>
    ),
  },
  {
    id: "recent",
    label: "Recent",
    icon: (
      <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 8v4l3 3" />
        <circle cx="12" cy="12" r="10" />
      </svg>
    ),
  },
  {
    id: "notes",
    label: "Notes",
    icon: (
      <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
        <polyline points="14 2 14 8 20 8" />
        <line x1="16" y1="13" x2="8" y2="13" />
        <line x1="16" y1="17" x2="8" y2="17" />
        <polyline points="10 9 9 9 8 9" />
      </svg>
    ),
  },
  {
    id: "meetings",
    label: "Meetings",
    icon: (
      <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
        <circle cx="9" cy="7" r="4" />
        <path d="M23 21v-2a4 4 0 0 0-3-3.87" />
        <path d="M16 3.13a4 4 0 0 1 0 7.75" />
      </svg>
    ),
  },
  {
    id: "voice",
    label: "Voice",
    icon: (
      <svg width={18} height={18} viewBox="0 0 24 24" fill="none" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
        <path d="M15.54 8.46a5 5 0 0 1 0 7.07" />
        <path d="M19.07 4.93a10 10 0 0 1 0 14.14" />
      </svg>
    ),
  },
  {
    id: "stats",
    label: "Stats",
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
  const [activeTab, setActiveTab] = useState<Tab>("dictation");
  const [collapsed, setCollapsed] = useState(false);

  // Listen for navigation events from float pill / tray
  useEffect(() => {
    const cleanup: (() => void)[] = [];
    (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        cleanup.push(await listen<string>("navigate-tab", (event) => {
          const tab = typeof event.payload === "string"
            ? event.payload.replace(/"/g, "") as Tab
            : null;
          if (tab && ["dictation", "voice", "recent", "stats", "settings", "notes", "meetings"].includes(tab)) {
            setActiveTab(tab);
          }
        }));
      } catch {
        // Not in Tauri environment
      }
    })();
    return () => { cleanup.forEach((fn) => fn()); };
  }, []);

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
          style={{ stroke: "rgba(255,255,255,0.25)", top: "6px", left: "88px" }}
          title={collapsed ? "Show sidebar" : "Hide sidebar"}
        >
          {SIDEBAR_ICON}
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
              title={item.label}
              className={[
                "w-[38px] h-[38px] rounded-[10px] flex items-center justify-center transition-colors flex-shrink-0",
                activeTab === item.id
                  ? "bg-[rgba(245,158,11,0.12)]"
                  : "hover:bg-[rgba(255,255,255,0.04)]",
              ].join(" ")}
              style={{
                stroke: activeTab === item.id ? "#fbbf24" : "rgba(255,255,255,0.22)",
              }}
            >
              {item.icon}
            </button>
          ))}
          </div>

          {/* Spacer */}
          <div className="flex-1" />

          {/* Settings */}
          <button
            onClick={() => setActiveTab("settings")}
            title="Settings"
            className={[
              "w-[38px] h-[38px] rounded-[10px] flex items-center justify-center transition-colors flex-shrink-0",
              activeTab === "settings"
                ? "bg-[rgba(245,158,11,0.12)]"
                : "hover:bg-[rgba(255,255,255,0.04)]",
            ].join(" ")}
            style={{
              stroke: activeTab === "settings" ? "#fbbf24" : "rgba(255,255,255,0.22)",
            }}
          >
            {SETTINGS_ICON}
          </button>
        </div>

        {/* Content area */}
        <div className="flex-1 overflow-hidden bg-[#1a1917]">
          {activeTab === "dictation" && <Dictation />}
          {activeTab === "voice" && <Voice />}
          {activeTab === "stats" && <Stats />}
          {activeTab === "settings" && <Settings />}
          {activeTab === "recent" && <Recent />}
          {activeTab === "notes" && <Notes />}
          {activeTab === "meetings" && <Meetings />}
        </div>
      </div>
    </div>
  );
}
