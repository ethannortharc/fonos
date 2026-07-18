// Consistent SVG icon set matching the sidebar stroke style.
// All icons: 24x24 viewBox, stroke-based, round caps/joins.

import React from "react";

interface IconProps {
  size?: number;
  className?: string;
  strokeWidth?: number;
}

const defaults = { size: 14, strokeWidth: 1.8 };

function I({ size = defaults.size, className, strokeWidth = defaults.strokeWidth, children }: IconProps & { children: React.ReactNode }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" className={className}>
      {children}
    </svg>
  );
}

/**
 * Fonos signal mark — a compact voice source with two outward echoes.
 * Kept deliberately simple so it remains recognizable in the 16–32px sizes
 * used by the sidebar, floating surfaces, tray artwork and empty states.
 */
export function FonosMark({ size = 20, className, strokeWidth = 1.9 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" className={className}>
      <circle cx="7" cy="12" r="2.15" fill="currentColor" stroke="none" />
      <path d="M11.2 7.4a6.1 6.1 0 0 1 0 9.2" />
      <path d="M15 4.45a10 10 0 0 1 0 15.1" />
    </svg>
  );
}

/** Microphone — recording / STT activity */
export function MicIcon(p: IconProps) {
  return <I {...p}><path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z" /><path d="M19 10v2a7 7 0 0 1-14 0v-2" /><line x1="12" y1="19" x2="12" y2="23" /></I>;
}

/** Document with lines — transcript / memo */
export function TranscriptIcon(p: IconProps) {
  return <I {...p}><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><polyline points="14 2 14 8 20 8" /><line x1="16" y1="13" x2="8" y2="13" /><line x1="16" y1="17" x2="8" y2="17" /></I>;
}

/** Hourglass — processing / waiting */
export function HourglassIcon(p: IconProps) {
  return <I {...p}><path d="M5 22h14" /><path d="M5 2h14" /><path d="M17 22v-4.172a2 2 0 0 0-.586-1.414L12 12l-4.414 4.414A2 2 0 0 0 7 17.828V22" /><path d="M7 2v4.172a2 2 0 0 0 .586 1.414L12 12l4.414-4.414A2 2 0 0 0 17 6.172V2" /></I>;
}

/** Sparkles — AI result / magic */
export function SparklesIcon(p: IconProps) {
  return <I {...p}><path d="M12 3l1.912 5.813a2 2 0 0 0 1.275 1.275L21 12l-5.813 1.912a2 2 0 0 0-1.275 1.275L12 21l-1.912-5.813a2 2 0 0 0-1.275-1.275L3 12l5.813-1.912a2 2 0 0 0 1.275-1.275L12 3z" /></I>;
}

/** Triangle warning — error / alert */
export function AlertIcon(p: IconProps) {
  return <I {...p}><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" /><line x1="12" y1="9" x2="12" y2="13" /><line x1="12" y1="17" x2="12.01" y2="17" /></I>;
}

/** Pin — quick note / pinned item */
export function PinIcon(p: IconProps) {
  return <I {...p}><line x1="12" y1="17" x2="12" y2="22" /><path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V17z" /></I>;
}

/** Book / notebook */
export function NotebookIcon(p: IconProps) {
  return <I {...p}><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" /><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" /></I>;
}

/** Square checkbox — unchecked */
export function CheckboxIcon(p: IconProps) {
  return <I {...p}><rect x="3" y="3" width="18" height="18" rx="2" /></I>;
}

/** Square with checkmark — checked */
export function CheckboxCheckedIcon(p: IconProps) {
  return <I {...p}><rect x="3" y="3" width="18" height="18" rx="2" /><polyline points="9 11 12 14 22 4" /></I>;
}

/** Circle dot — bullet point */
export function BulletIcon(p: IconProps) {
  return <I {...p} strokeWidth={0}><circle cx="12" cy="12" r="4" fill="currentColor" /></I>;
}

/** Users / people group — meeting */
export function UsersIcon(p: IconProps) {
  return <I {...p}><path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" /><circle cx="9" cy="7" r="4" /><path d="M23 21v-2a4 4 0 0 0-3-3.87" /><path d="M16 3.13a4 4 0 0 1 0 7.75" /></I>;
}

/** Brain — LLM / AI processing */
export function BrainIcon(p: IconProps) {
  return <I {...p}><path d="M9.5 2A5.5 5.5 0 0 0 5 5.5v.08A5.49 5.49 0 0 0 3 10a5.5 5.5 0 0 0 2.83 4.81A4.5 4.5 0 0 0 8 22h1V2.05A5.52 5.52 0 0 0 9.5 2z" /><path d="M14.5 2A5.5 5.5 0 0 1 19 5.5v.08A5.49 5.49 0 0 1 21 10a5.5 5.5 0 0 1-2.83 4.81A4.5 4.5 0 0 1 16 22h-1V2.05A5.52 5.52 0 0 1 14.5 2z" /></I>;
}

/** Globe — translate / language */
export function GlobeIcon(p: IconProps) {
  return <I {...p}><circle cx="12" cy="12" r="10" /><line x1="2" y1="12" x2="22" y2="12" /><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" /></I>;
}

/** Briefcase — formal / business */
export function BriefcaseIcon(p: IconProps) {
  return <I {...p}><rect x="2" y="7" width="20" height="14" rx="2" /><path d="M16 7V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v2" /><line x1="2" y1="13" x2="22" y2="13" /></I>;
}

export function BotIcon(p: IconProps) {
  return <I {...p}><path d="M12 8V4H8" /><rect x="4" y="8" width="16" height="12" rx="2" /><path d="M2 14h2" /><path d="M20 14h2" /><line x1="9" y1="13" x2="9" y2="15" /><line x1="15" y1="13" x2="15" y2="15" /></I>;
}

export function BulbIcon(p: IconProps) {
  return <I {...p}><path d="M9 18h6" /><path d="M10 22h4" /><path d="M15.09 14c.18-.98.65-1.74 1.41-2.5A4.65 4.65 0 0 0 18 8 6 6 0 0 0 6 8c0 1 .23 2.23 1.5 3.5.76.76 1.23 1.52 1.41 2.5" /></I>;
}

export function HeadphonesIcon(p: IconProps) {
  return <I {...p}><path d="M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a9 9 0 0 1 18 0v7a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3" /></I>;
}

export function CalendarIcon(p: IconProps) {
  return <I {...p}><rect x="3" y="4" width="18" height="18" rx="2" /><line x1="16" y1="2" x2="16" y2="6" /><line x1="8" y1="2" x2="8" y2="6" /><line x1="3" y1="10" x2="21" y2="10" /></I>;
}

export function PhoneIcon(p: IconProps) {
  return <I {...p}><path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z" /></I>;
}

export function GearIcon(p: IconProps) {
  return <I {...p}><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" /></I>;
}

// ─── ModeIcon mapper ────────────────────────────────────────────────────────

const EMOJI_TO_SVG: Record<string, (p: IconProps) => React.ReactElement> = {
  "\uD83C\uDF10": GlobeIcon,       // 🌐 translate
  "\u2728": SparklesIcon,           // ✨ polish
  "\uD83D\uDC54": BriefcaseIcon,   // 👔 formal
  "\uD83D\uDCDD": TranscriptIcon,  // 📝 raw
  "\uD83D\uDCD3": NotebookIcon,    // 📓 note
  "\uD83C\uDFA4": MicIcon,         // 🎤 (alt mic)
  "\uD83D\uDCCC": PinIcon,         // 📌 pin
  "\uD83C\uDF99": MicIcon,         // 🎙 meeting mic (U+1F399 doesn't match — use variant)
};
// Also map the common single-codepoint forms
EMOJI_TO_SVG["🌐"] = GlobeIcon;
EMOJI_TO_SVG["✨"] = SparklesIcon;
EMOJI_TO_SVG["👔"] = BriefcaseIcon;
EMOJI_TO_SVG["📝"] = TranscriptIcon;
EMOJI_TO_SVG["📓"] = NotebookIcon;
EMOJI_TO_SVG["🎤"] = MicIcon;
EMOJI_TO_SVG["🎙"] = MicIcon;
EMOJI_TO_SVG["🎙️"] = MicIcon;
EMOJI_TO_SVG["📌"] = PinIcon;
// Builtin workflow icons (agent/explain/listen/meeting/call) — keep in sync
// with public/float.html's SVG_ICONS so the pill roller and the main app
// render the same glyph for the same workflow.
EMOJI_TO_SVG["🤖"] = BotIcon;
EMOJI_TO_SVG["💡"] = BulbIcon;
EMOJI_TO_SVG["🎧"] = HeadphonesIcon;
EMOJI_TO_SVG["🗓"] = CalendarIcon;
EMOJI_TO_SVG["🗓️"] = CalendarIcon;
EMOJI_TO_SVG["📞"] = PhoneIcon;
EMOJI_TO_SVG["⚙"] = GearIcon;
EMOJI_TO_SVG["⚙️"] = GearIcon;

/**
 * Render a mode icon: maps known emojis to SVG, passes unknown strings through as text.
 * Use this wherever a mode.icon string is displayed.
 */
export function ModeIcon({ icon, size = 14, className }: { icon: string; size?: number; className?: string }) {
  const Svg = EMOJI_TO_SVG[icon.trim()];
  if (Svg) return <Svg size={size} className={className} />;
  return <span style={{ fontSize: size }} className={className}>{icon}</span>;
}
