// Settings → General → Floating indicator.
//
// One config field (`float_indicator`) drives four idle shapes, with "off"
// folded in as a shape rather than split out into a separate toggle — the
// question a user is actually asking is "how much of this do I want on
// screen?", and hidden is just the far end of that scale.
//
// The preview renders the shapes at 1:1 logical size over a few lines of mock
// text, because the two things that make the indicator annoying are how much it
// covers and whether it blurs what is under it — neither of which a swatch
// would show. The pill is the only shape carrying a backdrop-filter, and the
// preview is where that difference becomes visible before you commit to it.

import { resetFloatAnchor } from "../../lib/api";
import type { AppConfig, FloatIndicator } from "../../types";
import { useT } from "../../lib/i18n";
import { useState } from "react";

/** Idle shapes in increasing order of how much screen they take. */
const SHAPES: FloatIndicator[] = ["hairline", "dot", "pill", "off"];

/** Dimming delays offered, in seconds. `0` is "never". */
const FADE_CHOICES = [0, 3, 8, 15] as const;

const PILL_BG = "rgba(22,21,18,0.9)";
const AMBER = "rgba(251,191,36,0.85)";

/**
 * One shape, drawn at the same logical size the real indicator uses, so the
 * preview is honest about footprint. `opacity` mirrors the live resting value.
 */
function ShapePreview({ shape }: { shape: FloatIndicator }) {
  if (shape === "off") return null;
  if (shape === "hairline") {
    return <div style={{ width: 44, height: 3, borderRadius: 1.5, background: AMBER, opacity: 0.55 }} />;
  }
  if (shape === "dot") {
    return (
      <svg width={14} height={14} viewBox="0 0 24 24" fill="none" stroke={AMBER} strokeWidth={1.9}
           strokeLinecap="round" style={{ opacity: 0.55 }}>
        <circle cx="7" cy="12" r="2.15" fill={AMBER} stroke="none" />
        <path d="M11.2 7.4a6.1 6.1 0 0 1 0 9.2" />
        <path d="M15 4.45a10 10 0 0 1 0 15.1" />
      </svg>
    );
  }
  return (
    <div
      className="flex items-center justify-center gap-1.5"
      style={{
        // 98x32 — the pill fills its whole native window (see applyChrome),
        // and the point of this preview is an honest footprint.
        width: 98, height: 32, borderRadius: 999, opacity: 0.55,
        background: PILL_BG,
        // The real pill's blur. Kept in the preview because "this one smudges
        // the text behind it and the others don't" is the whole trade-off.
        backdropFilter: "blur(22px) saturate(112%)",
        WebkitBackdropFilter: "blur(22px) saturate(112%)",
        border: "1px solid rgba(255,255,255,0.1)",
      }}
    >
      <svg width={15} height={15} viewBox="0 0 24 24" fill="none" stroke={AMBER} strokeWidth={1.9} strokeLinecap="round">
        <circle cx="7" cy="12" r="2.15" fill={AMBER} stroke="none" />
        <path d="M11.2 7.4a6.1 6.1 0 0 1 0 9.2" />
        <path d="M15 4.45a10 10 0 0 1 0 15.1" />
      </svg>
      <span style={{ fontSize: 10, fontWeight: 750, letterSpacing: 0.7, color: "rgba(251,191,36,0.78)" }}>
        FONOS
      </span>
    </div>
  );
}

/** Mock text for the shape to sit over, so the preview shows real occlusion. */
function PreviewBackdrop({ shape }: { shape: FloatIndicator }) {
  return (
    <div className="relative h-[64px] rounded-lg overflow-hidden bg-[rgba(255,255,255,0.025)] border border-[rgba(255,255,255,0.05)]">
      <div className="absolute inset-0 flex flex-col justify-center gap-[7px] px-4" aria-hidden>
        {[100, 82, 91].map((w, i) => (
          <div key={i} className="h-[5px] rounded-full bg-[rgba(255,255,255,0.13)]" style={{ width: `${w}%` }} />
        ))}
      </div>
      <div className="absolute inset-0 flex items-center justify-center">
        <ShapePreview shape={shape} />
      </div>
    </div>
  );
}

export default function FloatIndicatorSection({
  config,
  onSave,
}: {
  config: AppConfig;
  onSave: (updates: Partial<AppConfig>) => void;
}) {
  const t = useT();
  // Normalize the same way Rust does (`normalize_float_indicator`). A config
  // hand-edited to an unknown shape otherwise leaves no chip selected, renders
  // the wrong preview, and prints a raw i18n key as the description — while the
  // indicator itself is correctly showing the default.
  const raw = config.float_indicator;
  const shape: FloatIndicator = SHAPES.includes(raw as FloatIndicator)
    ? (raw as FloatIndicator)
    : "pill";
  const fade = config.float_idle_fade_secs ?? 8;
  const [resetting, setResetting] = useState(false);

  const chip = (active: boolean) =>
    [
      "px-3 py-1.5 rounded-lg text-[10.5px] border transition-all",
      active
        ? "bg-[rgba(242,184,75,0.12)] border-[rgba(242,184,75,0.3)] text-[var(--accent)]"
        : "bg-[rgba(255,255,255,0.02)] border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.7)]",
    ].join(" ");

  return (
    <div className="flex flex-col gap-3">
      <div>
        <div className="text-[12px] font-medium text-[#fafaf9] mb-0.5">{t("general.float.title")}</div>
        <div className="text-[10px] text-[rgba(255,255,255,0.3)]">{t("general.float.desc")}</div>
      </div>

      {/* ── Idle shape ── */}
      <div className="flex flex-col gap-2">
        <div className="text-[11px] text-[rgba(255,255,255,0.55)]">{t("general.float.shape")}</div>
        <div className="flex gap-1.5 flex-wrap">
          {SHAPES.map((s) => (
            <button key={s} onClick={() => onSave({ float_indicator: s })} className={chip(shape === s)}>
              {t(`general.float.shape.${s}` as Parameters<typeof t>[0])}
            </button>
          ))}
        </div>
        <PreviewBackdrop shape={shape} />
        <div className="text-[10px] text-[rgba(255,255,255,0.3)]">
          {t(`general.float.shape.${shape}.desc` as Parameters<typeof t>[0])}
        </div>
      </div>

      {/* ── Dormancy ──
          Hidden for "off": there is nothing on screen left to dim, so offering
          a delay would imply the indicator still shows up somehow. */}
      {shape !== "off" && (
        <div className="flex flex-col gap-2">
          <div className="text-[11px] text-[rgba(255,255,255,0.55)]">{t("general.float.fade")}</div>
          <div className="flex gap-1.5 flex-wrap">
            {FADE_CHOICES.map((secs) => (
              <button
                key={secs}
                onClick={() => onSave({ float_idle_fade_secs: secs })}
                className={chip(fade === secs)}
              >
                {secs === 0
                  ? t("general.float.fade.never")
                  : t("general.float.fade.secs").replace("{n}", String(secs))}
              </button>
            ))}
          </div>
          <div className="text-[10px] text-[rgba(255,255,255,0.3)]">{t("general.float.fade.desc")}</div>
        </div>
      )}

      {/* ── Position ──
          There is no coordinate input on purpose: the indicator is dragged
          directly, and this button only exists to undo a drag that went
          somewhere unreachable. */}
      {shape !== "off" && (
        <div className="flex items-center justify-between gap-4">
          <div>
            <div className="text-[11px] text-[rgba(255,255,255,0.55)] mb-0.5">{t("general.float.position")}</div>
            <div className="text-[10px] text-[rgba(255,255,255,0.3)]">{t("general.float.position.desc")}</div>
          </div>
          <button
            disabled={resetting}
            onClick={async () => {
              setResetting(true);
              try {
                await resetFloatAnchor();
              } finally {
                setResetting(false);
              }
            }}
            className="px-2.5 py-1.5 rounded-lg text-[10px] flex-shrink-0 transition-all bg-[rgba(255,255,255,0.02)] border border-[rgba(255,255,255,0.06)] text-[rgba(255,255,255,0.45)] hover:text-[rgba(255,255,255,0.75)] disabled:opacity-40"
          >
            {t("general.float.position.reset")}
          </button>
        </div>
      )}
    </div>
  );
}
