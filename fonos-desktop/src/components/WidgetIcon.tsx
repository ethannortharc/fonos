// type_tag → inline SVG icon, plus role → category color. Self-contained
// stroke icon set (same spec as components/Icons.tsx's I()) so widget/workflow
// UI never depends on emoji glyphs from the backend.

import React from "react";

type IP = { size?: number; className?: string };
function S({ size = 14, className, children }: IP & { children: React.ReactNode }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" className={className}>
      {children}
    </svg>
  );
}

// type_tag → svg path children
const ICONS: Record<string, React.ReactNode> = {
  microphone: <><path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" /><path d="M19 10v2a7 7 0 0 1-14 0v-2" /><line x1="12" y1="19" x2="12" y2="23" /></>,
  selection: <><path d="M9 4H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h2" /><path d="M15 4h2a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-2" /><line x1="12" y1="7" x2="12" y2="17" /></>,
  instant: <path d="M13 2 3 14h7l-1 8 11-14h-7z" />,
  stt: <><line x1="4" y1="10" x2="4" y2="14" /><line x1="8" y1="6" x2="8" y2="18" /><line x1="12" y1="9" x2="12" y2="15" /><line x1="16" y1="4" x2="16" y2="20" /><line x1="20" y1="10" x2="20" y2="14" /></>,
  llm: <><rect x="6" y="6" width="12" height="12" rx="2" /><rect x="9.5" y="9.5" width="5" height="5" /><line x1="3" y1="9" x2="6" y2="9" /><line x1="3" y1="15" x2="6" y2="15" /><line x1="18" y1="9" x2="21" y2="9" /><line x1="18" y1="15" x2="21" y2="15" /><line x1="9" y1="3" x2="9" y2="6" /><line x1="15" y1="3" x2="15" y2="6" /><line x1="9" y1="18" x2="9" y2="21" /><line x1="15" y1="18" x2="15" y2="21" /></>,
  uppercase: <><path d="M6 20V7a3 3 0 0 1 6 0v13" /><line x1="6" y1="13" x2="12" y2="13" /><line x1="16" y1="8" x2="16" y2="20" /><path d="M16 11a3 3 0 0 1 4 0" /></>,
  insert: <><path d="M4 7V5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v2" /><line x1="12" y1="3" x2="12" y2="21" /><line x1="8" y1="21" x2="16" y2="21" /></>,
  replace: <><polyline points="17 1 21 5 17 9" /><path d="M3 11V9a4 4 0 0 1 4-4h14" /><polyline points="7 23 3 19 7 15" /><path d="M21 13v2a4 4 0 0 1-4 4H3" /></>,
  clipboard: <><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" /><rect x="8" y="2" width="8" height="4" rx="1" /></>,
  notebook: <><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" /><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" /></>,
  speak: <><path d="M11 5 6 9H2v6h4l5 4V5z" /><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07" /></>,
  panel: <><rect x="3" y="4" width="18" height="16" rx="2" /><line x1="3" y1="9" x2="21" y2="9" /></>,
  dialog: <><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" /></>,
  // Reuses the hand-drawn handset silhouette shared with the call panel's
  // hang-up button (public/call-panel.html) — same viewBox and organic
  // arched-handle shape, just picking up this set's shared 1.8 stroke width
  // instead of that button's local 1.5.
  call: <path d="M3.6 16.1 C2.2 14.85 2 12.6 3.3 11.2 C5.2 8.6 8.4 7.2 12 7.2 C15.6 7.2 18.8 8.6 20.7 11.2 C22 12.6 21.8 14.85 20.4 16.1 C19.45 17 17.85 16.95 17.05 16 C16.45 15.25 16.35 14.2 16.9 13.4 C15.4 11.9 13.8 11.2 12 11.2 C10.2 11.2 8.6 11.9 7.1 13.4 C7.65 14.2 7.55 15.25 6.95 16 C6.15 16.95 4.55 17 3.6 16.1 Z" />,
  agent: <><rect x="5" y="8" width="14" height="11" rx="3" /><line x1="12" y1="8" x2="12" y2="4" /><circle cx="12" cy="3" r="1" /><circle cx="9.5" cy="13.5" r="1.1" /><circle cx="14.5" cy="13.5" r="1.1" /><line x1="8" y1="19" x2="8" y2="21" /><line x1="16" y1="19" x2="16" y2="21" /></>,
  meeting: <><circle cx="9" cy="8" r="3.2" /><path d="M3.5 20c0-3.4 2.5-6 5.5-6s5.5 2.6 5.5 6" /><circle cx="17.5" cy="8.6" r="2.4" /><path d="M15.2 14.3c2.6.4 4.5 2.7 4.5 5.7" /></>,
};

export function WidgetIcon({ typeTag, size = 14, className }: { typeTag: string; size?: number; className?: string }) {
  const body = ICONS[typeTag] ?? ICONS.panel;
  return <S size={size} className={className}>{body}</S>;
}

export function roleColor(role: "source" | "processor" | "output") {
  if (role === "source") return { rgb: "251,191,36", hex: "#fbbf24" };
  if (role === "processor") return { rgb: "134,239,172", hex: "#86efac" };
  return { rgb: "196,181,253", hex: "#c4b5fd" };
}
