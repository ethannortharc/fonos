// MicButton — the fonos mic/stop button shell (glyphs + aura), extracted
// verbatim from Dictation.tsx (MicIcon/StopIcon/VoiceAura + the
// fonos-voice-button classNames) so the Test Run bench (Task 11) can drive
// mock microphone input with the same recording affordance, without
// depending on the (soon to be deleted, Task 12) Dictation view. CSS lives
// in index.css — this component only supplies the class contract
// (fonos-voice-button/-live/-idle, fonos-voice-glyph, fonos-mic-ambient*).

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
      className={["fonos-mic-ambient absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-[59%] w-[142px] h-[112px] pointer-events-none", active ? "fonos-mic-ambient-live" : ""].join(" ")}
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
    <div className="relative flex flex-col items-center justify-center">
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
