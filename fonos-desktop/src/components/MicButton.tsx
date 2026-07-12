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
      // beneath) the button below it — see the `isolate` note on the root
      // div: `isolate` makes that div the aura's nearest ancestor stacking
      // context, so this -z-10 is resolved only against its sibling <button>
      // right here and can never be compared against content outside
      // MicButton's own subtree. That's the full extent of what `isolate`
      // guarantees — it does NOT stop the MicButton root div itself (and
      // this aura along with it) from painting over later, unrelated
      // siblings elsewhere on the page; see the root div's comment.
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
    // stacking context for VoiceAura's negative z-index above — the aura's
    // -z-10 is resolved against its sibling <button> right here and can
    // never bubble up to be compared against unrelated content elsewhere on
    // the page. That is ALL this fixes.
    //
    // It does NOT contain the whole subtree. This div is `position: relative`
    // (required so it's the aura's containing block) with z-index: auto —
    // and per CSS painting order, a positioned element (or a stacking context
    // with no explicit z-index, which is what `isolate` makes this) always
    // paints in the "z-index: 0" bucket, which paints AFTER in-flow
    // non-positioned content within whatever its nearest ANCESTOR stacking
    // context is — regardless of DOM order. Neither this div's usual parent
    // (the caller's mic-wrap) nor typical grandparents form a stacking
    // context of their own, so this whole div (aura + button, as one atomic
    // layer) bubbles past them and WOULD paint over later normal-flow
    // siblings anywhere up to that ancestor (e.g. a node graph rendered
    // below it) — empirically confirmed: adding `isolate` to a container row
    // alone does NOT stop this, since an isolated-but-unpositioned-z-index
    // row is itself just another auto-z-index escapee one level up. The only
    // fix that has been verified to work is giving the row an explicit
    // (lower) z-index AND giving its later siblings an explicit (higher) one
    // — see the row-level comment in TestRunSection.tsx, which is the
    // load-bearing containment. Do not assume `isolate` here is sufficient on
    // its own for any caller.
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
