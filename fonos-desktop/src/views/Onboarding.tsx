// First-run onboarding (P1) — magic-first flow: value prop → mic playground →
// accessibility → guided task, with engine setup (the Scenarios cards)
// reachable from every screen via skip. Replaces <Scenarios mode="fullscreen">
// as the first-run gate; Scenarios itself is unchanged and embeds here as the
// "engines" step in overlay mode. Funnel milestones are recorded backend-side
// (mic/transcript/insert/ax) — this component only records nothing itself.
// Spec: docs/superpowers/specs/2026-07-14-onboarding-redesign-design.md §P1.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  getConfig,
  saveConfig,
  checkAccessibility,
  requestAccessibility,
  startRecording,
  stopRecording,
} from "../lib/api";
import { ensureAppleSttDefault } from "../lib/appleSttSeed";
import { isMacOS } from "../lib/platform";
import { t, useT } from "../lib/i18n";
import Scenarios, { isSttConfigured } from "./Scenarios";

/** Ordered steps. Linux front-loads engine setup because it has no built-in
 *  STT (spec §P1 Linux 差异); "engines" renders <Scenarios mode="overlay">. */
export type ObStep = "welcome" | "engines" | "playground" | "accessibility" | "guided";

// ── hotkey combo formatting (I1) ────────────────────────────────────────────
// Render the *actual* configured dictation combo into the guided/playground
// copy instead of the vague "your dictation hotkey". Mirrors ScenariosTab's
// formatCombo, but spaces the key off the modifiers ("⌘⇧ Space") on mac and
// falls back to a text form ("Ctrl+Shift+Space") elsewhere.
const MOD_MAC: Record<string, string> = {
  cmd: "⌘", command: "⌘", meta: "⌘",
  ctrl: "⌃", control: "⌃",
  alt: "⌥", option: "⌥", opt: "⌥",
  shift: "⇧",
};
const MOD_TXT: Record<string, string> = {
  cmd: "Ctrl", command: "Ctrl", meta: "Ctrl",
  ctrl: "Ctrl", control: "Ctrl",
  alt: "Alt", option: "Alt", opt: "Alt",
  shift: "Shift",
};

function keyName(p: string): string {
  if (p === "space") return "Space";
  return p.length === 1 ? p.toUpperCase() : p.charAt(0).toUpperCase() + p.slice(1);
}

/** Format a stored combo ("cmd+shift+space") for display. */
function formatHotkey(combo: string, mac: boolean): string {
  const parts = combo.split("+").map((p) => p.trim().toLowerCase()).filter(Boolean);
  if (parts.length === 0) return "";
  const key = keyName(parts[parts.length - 1]);
  const mods = parts.slice(0, -1).map((p) => (mac ? MOD_MAC[p] : MOD_TXT[p]) ?? keyName(p));
  return mac
    ? mods.join("") + (mods.length ? " " : "") + key
    : [...mods, key].join("+");
}

const FLOW: ObStep[] = isMacOS
  ? ["welcome", "playground", "accessibility", "guided"]
  : ["welcome", "engines", "playground", "accessibility", "guided"];

/** Local mirror of Scenarios' errStr — a backend rejection's human message. */
const errStr = (e: unknown) => (e instanceof Error ? e.message : String(e));

const pill =
  "mt-4 px-8 py-2.5 rounded-full bg-gradient-to-r from-[#f4c063] to-[#e8a72e] text-[#1a1917] text-[13px] font-semibold hover:opacity-90 transition-opacity disabled:opacity-40";
const ghost =
  "mt-4 px-6 py-2.5 rounded-full border border-[rgba(255,255,255,0.1)] text-[rgba(255,255,255,0.45)] text-[12px] hover:border-[rgba(255,255,255,0.2)] transition-colors";

export default function Onboarding({ onDone }: { onDone: () => void }) {
  useT();
  const [step, setStep] = useState<ObStep>("welcome");
  const [playText, setPlayText] = useState("");
  const [axWaiting, setAxWaiting] = useState(false);
  const [guidedDone, setGuidedDone] = useState(false);
  // Whether an STT engine is configured; gates the "no engine" warning in the
  // playground. Re-computed every time the playground step is (re-)entered.
  const [sttReady, setSttReady] = useState(true);
  // The actual configured dictation combo, formatted for display (I1). Loaded
  // once on mount; defaults to the built-in combo until config resolves.
  const [hotkey, setHotkey] = useState(() => formatHotkey("cmd+shift+space", isMacOS));
  // Hold-to-talk button state (C2). `pttActive` guards start/stop so the
  // pointerup + pointerleave pair can't double-fire the recording commands.
  const [ptt, setPtt] = useState(false);
  const pttActive = useRef(false);
  // Playground readiness (Fix 4). 'loading' until the Apple-STT seed is sought
  // and — when a patch is needed — actually persisted; 'ready' arms hold-to-talk;
  // 'error' keeps it disabled after a config failure. Guards the init race where
  // recording could start against a not-yet-seeded config. `playErr` is the
  // visible amber line: the raw backend text for a config failure, or the
  // ob.play.error template (raw message appended) for a recording failure.
  const [playState, setPlayState] = useState<"loading" | "ready" | "error">("loading");
  const [playErr, setPlayErr] = useState("");
  // macOS reaches "engines" only via skip, where it is terminal. On Linux it
  // sits mid-flow and continues to the playground.
  const enginesTerminal = useRef(isMacOS);

  // Load the configured dictation combo once so the copy shows the real keys.
  useEffect(() => {
    getConfig()
      .then((cfg) => {
        const combo = (cfg as { hotkey_dictation?: string }).hotkey_dictation || "cmd+shift+space";
        setHotkey(formatHotkey(combo, isMacOS));
      })
      .catch(() => {});
  }, []);

  // Hold-to-talk: an AX-independent fallback to the global hotkey (which needs
  // Accessibility to fire). Drives the same start_recording/stop_recording path
  // as the pill, which revives the legacy funnel instrumentation and emits
  // float:stop (caught by the playground listener below).
  const startPtt = useCallback(() => {
    if (pttActive.current) return;
    pttActive.current = true;
    setPtt(true);
    setPlayErr(""); // a fresh press clears any prior recording error
    startRecording().catch((e) => {
      // Mic-permission / device / engine failures used to vanish into a
      // no-op catch (Fix 4). Reset the button and surface the raw reason so
      // the user sees why nothing happened.
      pttActive.current = false;
      setPtt(false);
      setPlayErr(t("ob.play.error").replace("{msg}", errStr(e)));
    });
  }, []);
  const stopPtt = useCallback(() => {
    if (!pttActive.current) return;
    pttActive.current = false;
    setPtt(false);
    // Use the returned transcript directly: without Accessibility the legacy
    // path can't inject (no float:stop fires), but the text still comes back
    // here — the whole point of this AX-independent fallback.
    stopRecording()
      .then((r) => {
        if (r?.text) setPlayText(r.text);
      })
      .catch((e) => {
        setPlayErr(t("ob.play.error").replace("{msg}", errStr(e)));
      });
  }, []);

  const finish = useCallback(async () => {
    try {
      await saveConfig(JSON.stringify({ has_completed_onboarding: true }));
    } catch {
      /* non-Tauri/demo env — still leave the wizard */
    }
    onDone();
  }, [onDone]);

  const next = useCallback(() => {
    const i = FLOW.indexOf(step);
    if (i >= 0 && i + 1 < FLOW.length) setStep(FLOW[i + 1]);
    else void finish();
  }, [step, finish]);

  const skip = useCallback(() => {
    const i = FLOW.indexOf(step);
    const enginesAt = FLOW.indexOf("engines"); // -1 on macOS
    if (enginesAt === -1 || i < enginesAt) {
      enginesTerminal.current = enginesAt === -1;
      setStep("engines");
    } else {
      void finish();
    }
  }, [step, finish]);

  // Playground: seed Apple STT (macOS, unconfigured installs) the moment the
  // playground needs it, and listen for the dictation pipeline's transcript.
  // Hold-to-talk stays disabled (playState !== 'ready') until the seed's
  // saveConfig RESOLVES — arming it earlier lets a recording start against a
  // not-yet-persisted config, the init race (Fix 4).
  useEffect(() => {
    if (step !== "playground") return;
    let disposed = false;
    setPlayState("loading");
    setPlayErr("");
    getConfig()
      .then(async (cfg) => {
        const patch = ensureAppleSttDefault(cfg, isMacOS);
        if (patch) {
          // The seed makes STT usable even though `cfg` (read before the
          // patch) still looks unconfigured. Await the write so the button
          // only arms once the config is actually on disk.
          setSttReady(true);
          await saveConfig(JSON.stringify(patch));
        } else {
          setSttReady(isSttConfigured(cfg));
        }
        if (!disposed) setPlayState("ready");
      })
      .catch((e) => {
        // getConfig/saveConfig failures used to be swallowed, leaving the
        // button live against a broken config. Surface the reason and keep
        // recording disabled (Fix 4).
        if (!disposed) {
          setPlayState("error");
          setPlayErr(errStr(e));
        }
      });
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const un = await listen<string>("float:stop", (e) => {
          const text = typeof e.payload === "string" ? e.payload : "";
          if (text.trim()) setPlayText(text);
        });
        if (disposed) { un(); return; }
        unlisten = un;
      } catch {
        /* not in Tauri */
      }
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [step]);

  // Accessibility: pass straight through when already trusted (always true
  // off macOS — Linux never sees this screen).
  useEffect(() => {
    if (step !== "accessibility") return;
    let alive = true;
    checkAccessibility()
      .then((ok) => {
        if (alive && ok) next();
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step]);

  // After the user asked for the OS prompt, poll until granted. The backend's
  // check_accessibility also records the ax_granted milestone when true.
  useEffect(() => {
    if (step !== "accessibility" || !axWaiting) return;
    const timer = setInterval(() => {
      checkAccessibility()
        .then((ok) => {
          if (ok) {
            setAxWaiting(false);
            next();
          }
        })
        .catch(() => {});
    }, 2000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step, axWaiting]);

  // Guided task: a successful insertion into any app that isn't Fonos itself.
  useEffect(() => {
    if (step !== "guided") return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        const un = await listen<{ target_app: string | null }>("dictation:delivered", (e) => {
          const app = e.payload?.target_app ?? "";
          // Unknown target counts as success; only an insertion back into
          // Fonos's own windows doesn't complete the "any other app" task.
          if (!/fonos/i.test(app)) setGuidedDone(true);
        });
        if (disposed) { un(); return; }
        unlisten = un;
      } catch {
        /* not in Tauri */
      }
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [step]);

  if (step === "engines") {
    return (
      <div className="h-screen bg-[var(--bg)]">
        <Scenarios
          mode="overlay"
          onDone={() => {
            if (enginesTerminal.current) void finish();
            else setStep("playground");
          }}
        />
      </div>
    );
  }

  return (
    <div
      data-testid="onboarding"
      className="relative h-screen bg-[var(--bg)] flex items-center justify-center select-none"
    >
      {step === "welcome" && (
        <div className="flex flex-col items-center gap-3 text-center max-w-[440px] px-6">
          <div className="text-[26px] font-semibold text-[#fafaf9] [text-wrap:balance]">
            {t("ob.welcome.title")}
          </div>
          <p className="text-[13px] text-[rgba(255,255,255,0.5)]">{t("ob.welcome.tagline")}</p>
          <button data-testid="ob-start" onClick={next} className={pill}>
            {t("ob.welcome.start")}
          </button>
        </div>
      )}

      {step === "playground" && (
        <div className="flex flex-col items-center gap-3 text-center max-w-[480px] w-full px-6">
          <div className="text-[18px] font-semibold text-[#fafaf9]">{t("ob.playground.title")}</div>
          <p className="text-[12px] text-[rgba(255,255,255,0.5)]">{t("ob.playground.privacy")}</p>
          <div
            data-testid="ob-playground-box"
            className="w-full min-h-[96px] rounded-xl bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.07)] px-4 py-3 text-left"
          >
            {playText ? (
              <p
                data-testid="ob-playground-text"
                className="text-[14px] text-[#fafaf9] leading-relaxed whitespace-pre-wrap"
              >
                {playText}
              </p>
            ) : (
              <p className="text-[12px] text-[rgba(255,255,255,0.35)]">
                {t("ob.playground.hint").replace("{hotkey}", hotkey)}
              </p>
            )}
          </div>
          {playText && <p className="text-[11px] text-[#7ed492]">{t("ob.playground.ready")}</p>}
          {/* AX-independent fallback: the hotkey hint above stays primary. */}
          <button
            data-testid="ob-ptt"
            onPointerDown={startPtt}
            onPointerUp={stopPtt}
            onPointerLeave={stopPtt}
            disabled={playState !== "ready"}
            className={pill}
          >
            {ptt ? t("ob.play.ptt-active") : t("ob.play.ptt")}
          </button>
          <p className="text-[11px] text-[rgba(255,255,255,0.35)]">{t("ob.play.ptt-hint")}</p>
          {playState === "loading" && (
            <p data-testid="ob-play-preparing" className="text-[11px] text-[rgba(255,255,255,0.35)]">
              {t("ob.play.preparing")}
            </p>
          )}
          {playErr && (
            <p data-testid="ob-play-error" className="text-[11px] text-[#e8a72e]">
              {playErr}
            </p>
          )}
          {!sttReady && (
            <>
              <p data-testid="ob-no-stt" className="text-[11px] text-[#e8a72e]">
                {t("ob.play.no-stt")}
              </p>
              <button
                data-testid="ob-to-engines"
                onClick={() => setStep("engines")}
                className={ghost}
              >
                {t("ob.play.setup-engine")}
              </button>
            </>
          )}
          <button data-testid="ob-next" onClick={next} disabled={!playText} className={pill}>
            {t("ob.playground.next")}
          </button>
        </div>
      )}

      {step === "accessibility" && (
        <div className="flex flex-col items-center gap-3 text-center max-w-[480px] w-full px-6">
          <style>{`
            @keyframes ob-demo { 0%,10% {opacity:0} 22%,78% {opacity:1} 92%,100% {opacity:0} }
            .ob-demo span { opacity: 0; animation: ob-demo 5s infinite; }
            .ob-demo span:nth-child(2) { animation-delay: .9s; }
            .ob-demo span:nth-child(3) { animation-delay: 1.8s; }
            @media (prefers-reduced-motion: reduce) {
              .ob-demo span { animation: none; opacity: 1; }
            }
          `}</style>
          <div className="text-[18px] font-semibold text-[#fafaf9]">{t("ob.ax.title")}</div>
          <p className="text-[12px] text-[rgba(255,255,255,0.5)] max-w-[420px]">{t("ob.ax.desc")}</p>
          <div className="ob-demo w-full rounded-xl bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.07)] px-4 py-3 text-left text-[13px] text-[rgba(255,255,255,0.75)]">
            <span>{t("ob.ax.demo1")}</span>
            <span>{t("ob.ax.demo2")}</span>
            <span>{t("ob.ax.demo3")}</span>
          </div>
          <div className="flex gap-2">
            <button
              data-testid="ob-ax-grant"
              onClick={() => {
                setAxWaiting(true);
                requestAccessibility().catch(() => {});
              }}
              className={pill}
            >
              {axWaiting ? t("ob.ax.waiting") : t("ob.ax.grant")}
            </button>
            {/* "Not now" finishes the wizard rather than advancing to the
                guided task. Without Accessibility that task is impossible:
                InsertOutput (workflow_widgets.rs) delivers via the popup Panel
                and returns BEFORE it emits dictation:delivered — which is the
                only thing that enables guided's Finish — and the global hotkey
                itself is a CGEventTap that can't exist without AX. So Finish
                could never enable; complete onboarding here (popups still work). */}
            <button data-testid="ob-ax-later" onClick={() => void finish()} className={ghost}>
              {t("ob.ax.later")}
            </button>
          </div>
        </div>
      )}

      {step === "guided" && (
        <div className="flex flex-col items-center gap-3 text-center max-w-[480px] px-6">
          <div className="text-[18px] font-semibold text-[#fafaf9]">{t("ob.guided.title")}</div>
          <p className="text-[12px] text-[rgba(255,255,255,0.5)]">
            {t("ob.guided.desc").replace("{hotkey}", hotkey)}
          </p>
          {guidedDone ? (
            <p data-testid="ob-guided-done" className="text-[12px] text-[#7ed492]">
              {t("ob.guided.success")}
            </p>
          ) : (
            <p className="text-[11px] text-[rgba(255,255,255,0.3)]">{t("ob.guided.waiting")}</p>
          )}
          <button
            data-testid="ob-finish"
            onClick={() => void finish()}
            disabled={!guidedDone}
            className={pill}
          >
            {t("ob.guided.finish")}
          </button>
        </div>
      )}

      <div className="absolute bottom-6 right-8 flex flex-col items-end gap-1">
        {!isMacOS && (
          <p className="text-[10px] text-[rgba(255,255,255,0.25)] max-w-[360px] text-right">
            {t("ob.linux.hotkey-hint")}
          </p>
        )}
        <button
          data-testid="ob-skip"
          onClick={skip}
          className="text-[11px] text-[rgba(255,255,255,0.35)] hover:text-[var(--accent)] transition-colors"
        >
          {t("ob.skip")}
        </button>
      </div>
    </div>
  );
}
