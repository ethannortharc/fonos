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
  stt: <><line x1="4" y1="10" x2="4" y2="14" /><line x1="8" y1="6" x2="8" y2="18" /><line x1="12" y1="9" x2="12" y2="15" /><line x1="16" y1="4" x2="16" y2="20" /><line x1="20" y1="10" x2="20" y2="14" /></>,
  llm: <><path d="M12 3l1.9 5.8a2 2 0 0 0 1.3 1.3L21 12l-5.8 1.9a2 2 0 0 0-1.3 1.3L12 21l-1.9-5.8a2 2 0 0 0-1.3-1.3L3 12l5.8-1.9a2 2 0 0 0 1.3-1.3L12 3z" /></>,
  uppercase: <><path d="M6 20V7a3 3 0 0 1 6 0v13" /><line x1="6" y1="13" x2="12" y2="13" /><line x1="16" y1="8" x2="16" y2="20" /><path d="M16 11a3 3 0 0 1 4 0" /></>,
  insert: <><path d="M4 7V5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v2" /><line x1="12" y1="3" x2="12" y2="21" /><line x1="8" y1="21" x2="16" y2="21" /></>,
  replace: <><polyline points="17 1 21 5 17 9" /><path d="M3 11V9a4 4 0 0 1 4-4h14" /><polyline points="7 23 3 19 7 15" /><path d="M21 13v2a4 4 0 0 1-4 4H3" /></>,
  clipboard: <><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" /><rect x="8" y="2" width="8" height="4" rx="1" /></>,
  notebook: <><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" /><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" /></>,
  speak: <><path d="M11 5 6 9H2v6h4l5 4V5z" /><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07" /></>,
  panel: <><rect x="3" y="4" width="18" height="16" rx="2" /><line x1="3" y1="9" x2="21" y2="9" /></>,
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
