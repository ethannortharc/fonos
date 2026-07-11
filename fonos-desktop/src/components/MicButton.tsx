// MicButton — the fonos mic/stop button shell (glyphs + aura), extracted
// verbatim from the former Dictation.tsx (MicIcon/StopIcon/VoiceAura + the
// fonos-voice-button classNames) so the Test Run bench (Task 11) can drive
// mock microphone input with the same recording affordance. Dictation.tsx
// itself has since been deleted (Task 12) — this component no longer depends
// on it. CSS lives in index.css — this component only supplies the class
// contract (fonos-voice-button/-live/-idle, fonos-voice-glyph, fonos-mic-ambient*).

function MicIcon() {
  return (
    <svg className="fonos-voice-glyph relative z-10" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
      <rect x="9" y="1" width="6" height="12" rx="3" />
      <path d="M5 10a7 7 0 0 0 14 0" />
      <line x1="12" y1="17" x2="12" y2="21" />
    </svg>
  );
}

function StopIcon() {
  return (
    <svg className="fonos-voice-glyph relative z-10" width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
      <rect x="5" y="5" width="14" height="14" rx="2.5" />
    </svg>
  );
}

function VoiceAura({ active }: { active: boolean }) {
  return (
    <div
      // -z-10: a NEGATIVE z-index positioned descendant paints before (i.e.
      // beneath) the button below it in DOM order — see the `isolate` note
      // on the root div for why this stays scoped to this component instead
      // of escaping to compare against page content.
      className={["fonos-mic-ambient absolute -z-10 left-1/2 top-1/2 -translate-x-1/2 -translate-y-[59%] w-[142px] h-[112px] pointer-events-none", active ? "fonos-mic-ambient-live" : ""].join(" ")}
      aria-hidden="true"
    >
      <span className="fonos-mic-ambient-bloom absolute inset-[5px] rounded-full" />
      <span className="fonos-mic-ambient-core absolute inset-[24px] rounded-full" />
      <span className="fonos-mic-ambient-floor absolute left-[28px] right-[28px] bottom-[9px] h-[18px] rounded-full" />
    </div>
  );
}

export default function MicButton({
  recording, onClick, size = 64,
}: { recording: boolean; onClick: () => void; size?: number }) {
  return (
    // `isolate`: forces this div to be a stacking context of its own
    // (regardless of its z-index:auto), so it becomes the NEAREST ancestor
    // stacking context for the VoiceAura's negative z-index below — the
    // aura's -z-10 is resolved against its sibling <button> right here and
    // can never bubble up to be compared against unrelated content elsewhere
    // on the page (the graph, payload cards, run row, etc.). The whole
    // subtree still paints as one atomic layer in the parent's stacking
    // order, so any visual overflow (blur/breathing) can extend past this
    // div's box without ever painting over later siblings — no
    // overflow-hidden needed, so the animation isn't clipped.
    <div className="relative isolate flex flex-col items-center justify-center">
      <VoiceAura active={recording} />
      <button
        onClick={onClick}
        style={{ width: size, height: size }}
        className={[
          "fonos-voice-button rounded-full flex items-center justify-center transition-all duration-300 active:scale-[0.97]",
          recording ? "fonos-voice-button-live" : "fonos-voice-button-idle",
        ].join(" ")}
      >
        {recording ? <StopIcon /> : <MicIcon />}
      </button>
    </div>
  );
}
