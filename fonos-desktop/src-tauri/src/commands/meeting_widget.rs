//! Session-type Meeting output: continuous meeting capture + AI summary,
//! folding the legacy `hotkey_meeting` toggle arm's start/stop dance into one
//! composite output — reusing the exact `recv*` JS contract the legacy arm
//! drove (see `main.rs`'s former `"meeting"` dispatch arm, retired by
//! Workbench P2 Task 7), plus a real `recvMeetingError` handler where the
//! legacy arm's eval was dead (see the module docs on that below).
//!
//! [`MeetingOutput`] is the terminal [`Output`] a `meeting` widget resolves
//! to. Unlike `agent`/`dialog`, it has no empty-vs-non-empty input branch —
//! `wf.meeting` is always `src.instant`-sourced (always empty text) and the
//! composite reads the live [`crate::commands::meeting::MeetingState::recording`]
//! flag itself to decide which half of the toggle to run, exactly as the
//! legacy arm's key-down handler did.
//!
//! Ref resolution (Task 4's template, copied twice — once per capability):
//! [`MeetingProps::stt_widget`] wins when non-empty (its `stt` widget's
//! `model_profile` resolved via `fonos_core::services::resolve_profile`);
//! empty falls back to the legacy `meeting_stt_profile` config field, then the
//! global `"stt"` profile. **This is BUG 1's fix**: `commands::meeting`'s old
//! `start_meeting` used to read the global `"stt"` profile unconditionally,
//! silently ignoring `meeting_stt_profile` entirely — see
//! [`resolve_meeting_stt_widget_ref`] and [`MeetingOutput::start`].
//!
//! **Fix Round 1 (review)**: the ref's own `model_profile` can itself be
//! EMPTY (`stt.default` ships that way — "use global" convention, mirroring
//! [`super::workflow_widgets::SttProps::model_profile`]'s doc) or the
//! on-device `"apple-speech"` sentinel, which the generic `SttProcessor` can
//! drive but the meeting capture loop cannot (it always transcribes via
//! `commands::dictation::transcribe_http`, HTTP-only, no on-device path).
//! The same is true of a REAL profile whose `provider == "apple"` (the shape
//! first-run seeding writes: `scenario-apple-stt`) — Apple Speech has no HTTP
//! endpoint on ANY platform, so it is rejected unconditionally, unlike
//! dictation's macOS-gated check. [`MeetingOutput::start`] treats all of these
//! the same as an absent ref — falling through to `meeting_stt_profile` →
//! global `"stt"` (each tier apple-filtered too, with a final hard error when
//! nothing HTTP-capable remains) — instead of resolving them blindly into a
//! credential-less/broken `ServiceConfig` that silently failed every chunk.
//! See [`meeting_stt_profile_usable`] and [`resolve_meeting_stt_tier`].
//!
//! [`MeetingProps::llm_widget`] resolves the same way, falling back to the
//! pre-existing `meeting_llm_profile`→`llm_profile` chain (unchanged from
//! `commands::meeting::call_llm_raw`'s old inline behavior) — see
//! [`resolve_meeting_llm_widget_ref`] and [`MeetingOutput::stop`]. **Fix Round
//! 1 (review)**: `stop()` no longer lets a dangling/mismatched `llm_widget`
//! ref's `Err` short-circuit the whole stop via `?` — that used to leave
//! `MeetingState::recording` stuck `true` (nothing ever called
//! `stop_meeting_with`, the only thing that flips it) with no panel-visible
//! error. `stop()` now degrades: it surfaces the error via `recvMeetingError`
//! (same escape as `start()`) and still calls `stop_meeting_with` with the
//! legacy fallback chain, so recording always flips and a summary is still
//! attempted. **Final review wave (Critical)**: the ref's own `model_profile`
//! can itself be EMPTY (`llm.default`'s "use global" convention, same as
//! `stt.default`'s above) — [`resolve_meeting_stop_llm_profile`] used to let
//! that literal `""` win outright over the fallback chain, so a fresh
//! `llm.default`-referencing meeting composite always resolved to an empty
//! profile id and `resolve_llm_profile_service` hard-errored, breaking every
//! summary. It now runs the same emptiness check [`meeting_stt_profile_usable`]
//! applies to STT (see [`meeting_llm_profile_usable`]) before accepting the
//! ref's profile id, falling through to `meeting_llm_profile`→`llm_profile`
//! exactly like a wholly-absent ref. See [`resolve_meeting_stop_llm_profile`].
//!
//! Both legacy profile fields stay live-read (not one-time-migrated) because
//! they name model profiles, not widgets — there is no ref they can become.
//! `MeetingProps::summary_prompt`, by contrast, is free text, so it — and
//! only it — was one-time-migrated from `config.meeting_summary_prompt` by
//! `migrate_legacy_meeting_triggers`; that field is truly DEPRECATED (see its
//! doc comment) and is never read at resolve time here. Its resolution is
//! simply: prop non-empty wins, else `commands::meeting::build_summary_prompt`'s
//! built-in literal.
//!
//! Dead-eval fix: the legacy arm's start branch called
//! `panel.eval("recvMeetingShow()")` right after showing the panel —
//! `meeting-panel.html` never defined `recvMeetingShow`, so that call was a
//! silent no-op (same shape of bug as the agent panel's pre-Task-6
//! `recvSkillExec`/etc. calls). [`MeetingOutput::start`] replaces it with a
//! real `recvStart(title, "")` eval (immediately resetting the panel's
//! transcript/title, before the spawned capture loop's own later, more
//! accurate `recvStart(title, channel_mode)` call lands) — seeing why this
//! needs the pre-generated `title` from `commands::meeting::default_meeting_title`
//! rather than one invented on the spot, see that function's doc comment.
//! The legacy arm's capture-init-failure path already called
//! `recvMeetingError(...)` (also previously dead — the panel had no such
//! function either); rather than reroute those calls through an existing
//! handler, `meeting-panel.html` gained a real minimal `recvMeetingError`
//! (display the message, stop the timer) — chosen over rerouting because an
//! init failure is a genuinely different state from "stopped, summarizing"
//! (`recvStop`) or "done" (`recvSummary`), and conflating them would either
//! misrepresent the error as a normal stop or require the panel to guess
//! which case an empty/error string means.
//!
//! Lock discipline: every `state.config.lock()` use here is a tight scope
//! (widget-ref resolution is synchronous) dropped before the next `.await` —
//! same discipline as `agent_widget.rs`/`dialog.rs`/`workflow_widgets.rs`.

use serde::{Deserialize, Serialize};
use tauri::Manager;

use fonos_core::config::AppConfig;
use fonos_core::workflow::model::{Data, DataKind, WidgetDef};
use fonos_core::workflow::registry::{Output, RunCtx};

use super::meeting::{self, default_meeting_title, js_escape, meeting_js};
use super::AppState;

// ─── MeetingProps ───────────────────────────────────────────────────────────────

/// Configuration for a `meeting`-type composite widget's output — the
/// per-recipe knobs layered on top of `commands::meeting::MeetingState`'s
/// process-global runtime (recording flag / container id / chunk counter stay
/// global state, shared by every meeting entry point — this composite AND
/// the panel's own `stop_meeting` invocation alike).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeetingProps {
    /// Id of a tuned `stt` widget this meeting should resolve its
    /// transcription model profile from instead of the legacy
    /// `meeting_stt_profile`→global-`"stt"` fallback chain (Workbench P2
    /// Task 7, additive — mirrors `DialogProps`/`AgentProps::llm_widget`; see
    /// [`resolve_meeting_stt_widget_ref`]). **BUG 1 fix**: this is the ref
    /// half of it — the fallback chain itself now also honors
    /// `meeting_stt_profile`, which the pre-Task-7 code silently ignored.
    #[serde(default)]
    pub stt_widget: String,
    /// Id of a tuned `llm` widget this meeting should resolve its summary
    /// model profile from instead of the `meeting_llm_profile`→`llm_profile`
    /// config fallback chain (same template, applied to the summary LLM
    /// call — see [`resolve_meeting_llm_widget_ref`]).
    #[serde(default)]
    pub llm_widget: String,
    /// Inline summary-generation instructions, replacing
    /// `commands::meeting::BUILTIN_SUMMARY_INSTRUCTIONS` when non-empty.
    /// Empty means the built-in literal (unchanged pre-Task-7 behavior).
    /// Seeded from the now-deprecated `config.meeting_summary_prompt` by
    /// `migrate_legacy_meeting_triggers` for upgrading users; a brand-new
    /// meeting widget starts with this blank.
    #[serde(default)]
    pub summary_prompt: String,
    /// Opt-in speaker diarization (Workbench meeting-diarization epic, Task
    /// 3): when true AND the capture negotiates dual-channel audio, the
    /// system-audio channel is additionally fed to a `DiarizeSession`
    /// (`crate::audio::diarize`) so remote-participant entries get labeled by
    /// dominant speaker (`"s1"`, `"s2"`, …) instead of the flat `"Audio"`
    /// hint. Defaults false (`#[serde(default)]`) so every persisted meeting
    /// widget predating this field keeps its old mono-speaker behavior.
    /// Every failure mode downstream (mono capture, helper/model missing,
    /// spawn failure, mid-session death) degrades to the pre-diarization
    /// behavior rather than interrupting the meeting — see
    /// `meeting::start_meeting_with`.
    #[serde(default)]
    pub diarize: bool,
}

/// Resolve [`MeetingProps::stt_widget`] — Task 4's ref-resolution template
/// applied to the meeting composite (mirrors
/// `agent_widget::resolve_agent_llm_widget_ref`): a non-empty ref is looked
/// up in `widgets`, erroring on a dangling ref or a `type_tag` other than
/// `"stt"`; on success its `model_profile` is returned verbatim (**note**:
/// this can itself be empty, or the `"apple-speech"` sentinel — see
/// [`meeting_stt_profile_usable`]/[`resolve_meeting_stt_tier`], which the
/// caller applies before deciding whether to fall back; `stt_prompt`/
/// `vocab_books`/`temperature`/`language` are irrelevant here — the meeting
/// capture loop always transcribes via its own fixed 10-second-chunk
/// pipeline in `commands::meeting`, not the generic `SttProcessor`). An empty
/// ref (i.e. `MeetingProps::stt_widget` itself unset) returns `Ok(None)` —
/// see [`MeetingOutput::start`].
pub(crate) fn resolve_meeting_stt_widget_ref(
    props: &MeetingProps,
    widgets: &[WidgetDef],
) -> Result<Option<String>, String> {
    if props.stt_widget.is_empty() {
        return Ok(None);
    }
    let widget = widgets
        .iter()
        .find(|w| w.id == props.stt_widget)
        .ok_or_else(|| format!("meeting: stt_widget '{}' not found", props.stt_widget))?;
    if widget.type_tag != "stt" {
        return Err(format!(
            "meeting stt_widget '{}' is a '{}' widget, expected 'stt'",
            props.stt_widget, widget.type_tag
        ));
    }
    let stt_props: super::workflow_widgets::SttProps = serde_json::from_value(widget.props.clone())
        .map_err(|e| format!("meeting: stt_widget '{}' props: {e}", props.stt_widget))?;
    Ok(Some(stt_props.model_profile))
}

/// The on-device Apple Speech sentinel (see
/// [`super::workflow_widgets::SttProps::model_profile`]'s doc): resolvable by
/// the generic `SttProcessor` (which builds a special local `ServiceConfig`),
/// but NOT by the meeting capture loop, which always transcribes over HTTP
/// (`commands::dictation::transcribe_http`) and has no on-device path.
const MEETING_APPLE_SPEECH_SENTINEL: &str = "apple-speech";

/// True when `profile` names a model profile the meeting capture loop can
/// actually use for its STT tier — mirrors `SttProcessor`'s three-way
/// semantics (`workflow_widgets.rs`'s `SttProcessor::process`): not empty
/// (the `"use global"` convention a `stt_widget`'s own `model_profile` may
/// carry, e.g. `stt.default`), not [`MEETING_APPLE_SPEECH_SENTINEL`]
/// (on-device-only), and not a real profile whose `provider == "apple"`
/// (the shape first-run seeding writes: `scenario-apple-stt`) — meeting's
/// HTTP-only `transcribe_http` cannot drive Apple Speech in either spelling,
/// on ANY platform, so the provider rejection is unconditional (unlike
/// dictation's `cfg!(macos)`-gated gate in `fonos-core::services`). Every
/// unusable case means "this tier has nothing usable" — the caller must fall
/// through exactly as it would for an absent ref, rather than resolving
/// blindly into a credential-less/broken `ServiceConfig` (review Fix Round
/// 1's Critical item; the provider half is review Fix Round 3).
///
/// A DANGLING id (a since-deleted profile) is unusable too: it resolves to an
/// empty `ServiceConfig`, and letting it win the tier used to start a
/// recording whose every chunk died silently in `transcribe_http` — no
/// transcript, no summary (Codex review P1; mirrors `stt_ref_usable`'s
/// dangling rule in `fonos-core::services`). Hence the
/// `profile_exists` check first — `resolve_profile`'s empty fallback can't
/// distinguish dangling from a real profile.
///
/// Finally the RESOLVED shape must actually be drivable: provider not
/// `"apple"`, and a non-blank `base_url` after `resolve_profile` applies the
/// per-provider defaults (a real profile on a provider with no default
/// endpoint and no explicit URL has nowhere to POST). Every unusable shape
/// falls through the cascade identically — otherwise a tier this predicate
/// accepts gets hard-rejected by `validate_meeting_stt_svc` instead of
/// falling to a perfectly good later tier.
pub(crate) fn meeting_stt_profile_usable(config: &AppConfig, profile: &str) -> bool {
    if profile.is_empty() || profile == MEETING_APPLE_SPEECH_SENTINEL {
        return false;
    }
    if !fonos_core::services::profile_exists(config, profile) {
        return false;
    }
    let svc = fonos_core::services::resolve_profile(config, profile);
    svc.provider != "apple" && !svc.base_url.trim().is_empty()
}

/// Decide the effective STT profile-id tier for [`MeetingOutput::start`]'s
/// config lookup — the pure fallback-cascade decision (Task 4's testability
/// template) extracted so it's unit-testable without an `AppConfig`/
/// `resolve_profile`. `ref_profile` is [`resolve_meeting_stt_widget_ref`]'s
/// `Ok` payload (already past the dangling-ref/type-mismatch `Err` case);
/// `legacy_profile` is `config.meeting_stt_profile`.
///
/// `ref_profile` wins when [`meeting_stt_profile_usable`]; otherwise
/// `legacy_profile` wins under the same usability check (it too can name an
/// apple-provider profile — or even the literal sentinel — and meeting's
/// HTTP-only loop can't drive either, so it falls through like an absent
/// ref); otherwise `None` — meaning the caller should resolve the global
/// `"stt"` service instead of a specific profile id (and apply its own final
/// apple backstop, since the global default can be apple-shaped as well —
/// see [`MeetingOutput::start`]).
///
/// Takes `config` since usability now depends on the resolved provider, not
/// just the id's spelling — the price of closing the `scenario-apple-stt`
/// gap. Still synchronous and side-effect-free, so the unit tests below just
/// build an `AppConfig` with the profiles they need.
pub(crate) fn resolve_meeting_stt_tier(
    config: &AppConfig,
    ref_profile: Option<&str>,
    legacy_profile: &str,
) -> Option<String> {
    if let Some(p) = ref_profile {
        if meeting_stt_profile_usable(config, p) {
            return Some(p.to_string());
        }
    }
    if meeting_stt_profile_usable(config, legacy_profile) {
        return Some(legacy_profile.to_string());
    }
    None
}

/// Resolve [`MeetingProps::llm_widget`] — same template as
/// [`resolve_meeting_stt_widget_ref`] applied to the `llm` capability: a
/// non-empty ref is looked up in `widgets`, erroring on a dangling ref or a
/// `type_tag` other than `"llm"`; on success its `model_profile` is returned
/// (`system`/`user_template` are irrelevant — the summary call is a single
/// raw-prompt completion, not a system+template exchange). An empty ref
/// returns `Ok(None)`, telling the caller to fall back to the pre-existing
/// `meeting_llm_profile`→`llm_profile` config chain — see
/// [`MeetingOutput::stop`]. **Note**: unlike [`resolve_meeting_stt_widget_ref`],
/// `stop()` does not let this function's `Err` block stopping — see
/// [`resolve_meeting_stop_llm_profile`].
pub(crate) fn resolve_meeting_llm_widget_ref(
    props: &MeetingProps,
    widgets: &[WidgetDef],
) -> Result<Option<String>, String> {
    if props.llm_widget.is_empty() {
        return Ok(None);
    }
    let widget = widgets
        .iter()
        .find(|w| w.id == props.llm_widget)
        .ok_or_else(|| format!("meeting: llm_widget '{}' not found", props.llm_widget))?;
    if widget.type_tag != "llm" {
        return Err(format!(
            "meeting llm_widget '{}' is a '{}' widget, expected 'llm'",
            props.llm_widget, widget.type_tag
        ));
    }
    let llm_props: fonos_core::workflow::llm_step::LlmProps = serde_json::from_value(widget.props.clone())
        .map_err(|e| format!("meeting: llm_widget '{}' props: {e}", props.llm_widget))?;
    Ok(Some(llm_props.model_profile))
}

/// The legacy `meeting_llm_profile`→`llm_profile` fallback chain, factored out
/// so both arms of [`resolve_meeting_stop_llm_profile`] (ref absent, ref
/// resolution failed) share it verbatim.
fn meeting_llm_fallback_profile(meeting_llm_profile: &str, llm_profile: &str) -> String {
    if !meeting_llm_profile.is_empty() {
        meeting_llm_profile.to_string()
    } else {
        llm_profile.to_string()
    }
}

/// True when `profile_id` is a usable LLM profile tier for the meeting
/// summary call — mirrors [`meeting_stt_profile_usable`]'s shape (there is no
/// on-device sentinel to also reject here, unlike STT: any non-empty ref
/// `model_profile` is a real `model_profiles` id the summary's raw-prompt
/// completion can use). An EMPTY `model_profile` is `llm.default`'s own "use
/// global" convention (Task 4's template) — final review wave Critical fix:
/// a ref resolving to `""` must fall through to the legacy chain exactly like
/// an absent ref, not win outright as an unusable empty profile id.
pub(crate) fn meeting_llm_profile_usable(profile_id: &str) -> bool {
    !profile_id.is_empty()
}

/// Decide the LLM profile id [`MeetingOutput::stop`] should pass to
/// `stop_meeting_with`, from [`resolve_meeting_llm_widget_ref`]'s raw result —
/// the pure decision behind `stop`'s "never block stopping" fix (review Fix
/// Round 1, Important item 2), extracted so it's unit-testable without a
/// `tauri::State`/panel (Task 4's template).
///
/// Returns `(profile_id, error_to_surface)`. `error_to_surface` is `Some` only
/// when ref-resolution itself failed (dangling ref / type mismatch) — the
/// caller must still flip `recording` and attempt a summary using the
/// returned (legacy-fallback) `profile_id` regardless, just also show the
/// panel this error via `recvMeetingError` (same escape `start()` uses on its
/// own failure path).
pub(crate) fn resolve_meeting_stop_llm_profile(
    ref_result: Result<Option<String>, String>,
    meeting_llm_profile: &str,
    llm_profile: &str,
) -> (String, Option<String>) {
    match ref_result {
        Ok(Some(profile_id)) if meeting_llm_profile_usable(&profile_id) => (profile_id, None),
        Ok(Some(_)) | Ok(None) => (
            meeting_llm_fallback_profile(meeting_llm_profile, llm_profile),
            None,
        ),
        Err(e) => (
            meeting_llm_fallback_profile(meeting_llm_profile, llm_profile),
            Some(e),
        ),
    }
}

// ─── MeetingOutput ──────────────────────────────────────────────────────────────

/// `meeting`: continuous meeting capture + AI summary, toggled by the live
/// `MeetingState::recording` flag (start when not recording, stop when
/// recording) — the composite reading of the legacy hotkey arm's key-down
/// toggle dance.
pub struct MeetingOutput {
    /// Handle used to reach `AppState` and the panel window.
    pub app: tauri::AppHandle,
    /// Deserialized widget configuration.
    pub props: MeetingProps,
}

#[async_trait::async_trait]
impl Output for MeetingOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, _ctx: &RunCtx) -> Result<(), String> {
        match result {
            Data::Text(_) => {}
            Data::Audio(_) => return Err("meeting output expected text, got audio".to_string()),
        }

        let state: tauri::State<'_, AppState> = self.app.state();
        let is_recording = state.meeting.lock().await.recording;

        if !is_recording {
            self.start(&state).await
        } else {
            self.stop(&state).await
        }
    }
}

impl MeetingOutput {
    /// Not-recording half of the toggle: resolve the STT service, position +
    /// reveal the panel, start capture, and reset the panel's transcript via
    /// an immediate `recvStart` (replacing the legacy arm's dead
    /// `recvMeetingShow` eval — see the module docs).
    async fn start(&self, state: &tauri::State<'_, AppState>) -> Result<(), String> {
        // STT resolution: stt_widget ref → its model_profile, but only when
        // usable (BUG 1 fix's ref half; review Fix Round 1 hardens this: an
        // EMPTY model_profile — stt.default's "use global" convention — or
        // the on-device "apple-speech" sentinel — which meeting's HTTP-only
        // transcribe_http can't drive — both fall through here exactly like
        // an absent ref, instead of resolving blindly into a
        // credential-less/broken ServiceConfig that silently fails every
        // chunk) → the legacy meeting_stt_profile field (BUG 1 fix's fallback
        // half — the old code never read this at all) → the global "stt"
        // profile. One scoped lock, resolved synchronously, dropped before
        // any await below.
        let stt_svc = {
            let config = state.config.lock().map_err(|e| e.to_string())?;
            let widgets = fonos_core::workflow::engine::effective_widgets(&config);
            let ref_profile = resolve_meeting_stt_widget_ref(&self.props, &widgets)?;

            if ref_profile.as_deref() == Some(MEETING_APPLE_SPEECH_SENTINEL) {
                eprintln!(
                    "fonos: meeting stt_widget '{}' resolves to on-device apple-speech; \
                     meeting capture transcribes via HTTP only and cannot drive on-device \
                     recognition — falling back to meeting_stt_profile / global \"stt\"",
                    self.props.stt_widget
                );
            }

            let svc = match resolve_meeting_stt_tier(
                &config,
                ref_profile.as_deref(),
                &config.meeting_stt_profile,
            ) {
                Some(profile_id) => fonos_core::services::resolve_profile(&config, &profile_id),
                None => fonos_core::services::resolve_service(&config, "stt"),
            };
            // Final backstop: the tier cascade filters apple-provider and
            // dangling profiles, but the global "stt" fallback above can still
            // resolve to an apple one (the first-run scenario-apple-stt
            // default) or to nothing at all (global default unset or itself
            // dangling → empty ServiceConfig). Fail with a clear message here,
            // BEFORE the panel shows, rather than letting every captured chunk
            // die in transcribe_http with a raw error. `start_meeting_with`
            // re-runs the same check as its own invariant (it also guards the
            // legacy `start_meeting` IPC shell, which has no cascade).
            meeting::validate_meeting_stt_svc(&svc)?;
            svc
        };

        #[cfg(target_os = "macos")]
        super::move_panel_to_cursor(&self.app, "meeting-panel", 520, 0, super::PanelAnchor::BottomRight { top_margin: 80.0 });
        if let Some(panel) = self.app.get_webview_window("meeting-panel") {
            let _ = panel.show();
            let _ = panel.set_focus();
        }
        // Let the webview settle before eval() — same trick as the agent/
        // dialog panels.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        // Workbench P2 Task 13: the title's "Meeting"/"会议" label follows
        // `config.ui_language` — a separate short-lived lock (dropped
        // immediately) rather than threading it through the `stt_svc` block
        // above, which returns a `ServiceConfig`, not a `Lang`. A poisoned
        // lock degrades to resolve_lang("auto") (Task 14 — same convention
        // as dialog.rs/workflow_widgets.rs) rather than failing the whole
        // deliver.
        let lang = match state.config.lock() {
            Ok(config) => fonos_core::workflow::builtin::resolve_lang(&config.ui_language),
            Err(_) => fonos_core::workflow::builtin::resolve_lang("auto"),
        };
        let title = default_meeting_title(lang);
        match meeting::start_meeting_with(&self.app, state, stt_svc, title.clone(), self.props.diarize).await {
            Ok(_container_id) => {
                let title_j = serde_json::to_string(&title).unwrap_or_default();
                meeting_js(&self.app, &format!("recvStart({title_j}, '')"));
                Ok(())
            }
            Err(e) => {
                meeting_js(&self.app, &format!("recvMeetingError('{}')", js_escape(&e)));
                Err(e)
            }
        }
    }

    /// Recording half of the toggle: resolve the summary LLM profile and
    /// stop capture (delegates entirely to
    /// `commands::meeting::stop_meeting_with`, which also generates the
    /// summary, records it, notifies the panel, and schedules the delayed
    /// hide).
    ///
    /// **Review Fix Round 1 (Important item 2)**: `llm_widget` ref-resolution
    /// failure (dangling ref / type mismatch — e.g. a hand-edited config)
    /// must NOT block stopping. Previously this used `?`, which returned
    /// before `stop_meeting_with` ever ran — the only thing that flips
    /// `MeetingState::recording` back to `false` — leaving the panel stuck
    /// "recording" forever with no visible error. Now the error is surfaced
    /// via `recvMeetingError` (same escape `start()` uses) and the stop
    /// continues with the legacy `meeting_llm_profile`→`llm_profile`
    /// fallback, so recording always flips and a summary is still attempted.
    /// See [`resolve_meeting_stop_llm_profile`] for the extracted pure
    /// decision.
    async fn stop(&self, state: &tauri::State<'_, AppState>) -> Result<(), String> {
        // LLM resolution: llm_widget ref → its model_profile → the legacy
        // meeting_llm_profile→llm_profile chain (unchanged from the pre-Task-7
        // inline behavior). One scoped lock, dropped before any await below.
        let (llm_profile_id, ref_error) = {
            let config = state.config.lock().map_err(|e| e.to_string())?;
            let widgets = fonos_core::workflow::engine::effective_widgets(&config);
            let ref_result = resolve_meeting_llm_widget_ref(&self.props, &widgets);
            resolve_meeting_stop_llm_profile(ref_result, &config.meeting_llm_profile, &config.llm_profile)
        };

        if let Some(e) = ref_error {
            meeting_js(&self.app, &format!("recvMeetingError('{}')", js_escape(&e)));
        }

        meeting::stop_meeting_with(&self.app, state, llm_profile_id, self.props.summary_prompt.clone())
            .await
            .map(|_summary| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fonos_core::workflow::model::WidgetRole;

    fn base_props() -> MeetingProps {
        serde_json::from_value(serde_json::json!({})).unwrap()
    }

    fn stt_widget_def(id: &str, model_profile: &str) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role: WidgetRole::Processor,
            type_tag: "stt".to_string(),
            name: id.to_string(),
            icon: String::new(),
            props: serde_json::json!({
                "model_profile": model_profile,
                "stt_prompt": "",
                "vocab_books": [],
                "temperature": 0.0,
                "language": "auto",
            }),
            builtin: false,
        }
    }

    fn llm_widget_def(id: &str, model_profile: &str) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role: WidgetRole::Processor,
            type_tag: "llm".to_string(),
            name: id.to_string(),
            icon: String::new(),
            props: serde_json::json!({ "model_profile": model_profile, "system": null }),
            builtin: false,
        }
    }

    // ── MeetingProps serde defaults ──────────────────────────────────────────

    #[test]
    fn meeting_props_defaults_from_empty_json() {
        let props: MeetingProps = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(props.stt_widget, "");
        assert_eq!(props.llm_widget, "");
        assert_eq!(props.summary_prompt, "");
    }

    #[test]
    fn meeting_props_old_json_without_new_fields_still_parses() {
        // Back-compat: a persisted meeting widget missing any of these keys
        // still deserializes.
        let json = r#"{"stt_widget": "stt.tuned"}"#;
        let props: MeetingProps = serde_json::from_str(json).unwrap();
        assert_eq!(props.stt_widget, "stt.tuned");
        assert_eq!(props.llm_widget, "");
        assert_eq!(props.summary_prompt, "");
    }

    #[test]
    fn meeting_props_roundtrip_preserves_custom_values() {
        let props = MeetingProps {
            stt_widget: "stt.tuned".into(),
            llm_widget: "llm.tuned".into(),
            summary_prompt: "Focus on decisions.".into(),
            diarize: false,
        };
        let json = serde_json::to_value(&props).unwrap();
        let back: MeetingProps = serde_json::from_value(json).unwrap();
        assert_eq!(back, props);
    }

    // ── resolve_meeting_stt_widget_ref ────────────────────────────────────────

    #[test]
    fn resolve_meeting_stt_widget_ref_empty_returns_none() {
        let props = base_props();
        assert_eq!(resolve_meeting_stt_widget_ref(&props, &[]).unwrap(), None);
    }

    #[test]
    fn resolve_meeting_stt_widget_ref_resolves_model_profile() {
        let mut props = base_props();
        props.stt_widget = "stt.tuned".into();
        let widgets = vec![stt_widget_def("stt.tuned", "tuned-stt-profile")];
        assert_eq!(
            resolve_meeting_stt_widget_ref(&props, &widgets).unwrap(),
            Some("tuned-stt-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_stt_widget_ref_dangling_ref_errors() {
        let mut props = base_props();
        props.stt_widget = "stt.missing".into();
        let err = resolve_meeting_stt_widget_ref(&props, &[]).unwrap_err();
        assert!(err.contains("stt.missing"), "error should name the missing id, got: {err}");
    }

    #[test]
    fn resolve_meeting_stt_widget_ref_type_mismatch_errors() {
        let mut props = base_props();
        props.stt_widget = "llm.x".into();
        let wrong_type_widget = llm_widget_def("llm.x", "p");
        let err = resolve_meeting_stt_widget_ref(&props, &[wrong_type_widget]).unwrap_err();
        assert!(err.contains("expected 'stt'"), "error should mention type mismatch, got: {err}");
    }

    // ── meeting_stt_profile_usable / resolve_meeting_stt_tier ────────────────

    /// A config whose `model_profiles` holds one entry per (id, provider)
    /// pair — the minimum `meeting_stt_profile_usable`'s provider lookup
    /// needs (mirrors fonos-core services.rs's own test fixture style).
    fn cfg_with_profiles(entries: &[(&str, &str)]) -> AppConfig {
        AppConfig {
            model_profiles: entries
                .iter()
                .map(|(id, provider)| {
                    serde_json::json!({ "id": id, "name": id, "provider": provider, "model": "m" })
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn meeting_stt_profile_usable_rejects_empty_and_apple_speech() {
        let cfg = cfg_with_profiles(&[("some-profile", "openai")]);
        assert!(!meeting_stt_profile_usable(&cfg, ""));
        assert!(!meeting_stt_profile_usable(&cfg, "apple-speech"));
        assert!(meeting_stt_profile_usable(&cfg, "some-profile"));
    }

    #[test]
    fn meeting_stt_profile_usable_rejects_apple_provider_profile() {
        // The first-run shape: a REAL profile (scenario-apple-stt) whose
        // provider is "apple". Meeting transcribes over HTTP only, so this is
        // unusable on EVERY platform — no cfg! gate, unlike dictation's.
        let cfg = cfg_with_profiles(&[("scenario-apple-stt", "apple"), ("p-openai", "openai")]);
        assert!(!meeting_stt_profile_usable(&cfg, "scenario-apple-stt"));
        assert!(meeting_stt_profile_usable(&cfg, "p-openai"));
    }

    #[test]
    fn meeting_stt_profile_usable_rejects_dangling_id() {
        // A since-deleted profile id must be UNUSABLE so the tier cascade falls
        // through. Letting it win the tier resolved an empty ServiceConfig and
        // recording started with every chunk dying silently in transcribe_http
        // — no transcript, no summary (Codex review P1).
        let cfg = cfg_with_profiles(&[("p-openai", "openai")]);
        assert!(!meeting_stt_profile_usable(&cfg, "no-such-profile"));
    }

    #[test]
    fn meeting_stt_profile_usable_rejects_existing_profile_with_no_endpoint() {
        // Profile exists and isn't apple, but resolves to an empty/whitespace
        // base_url (provider with no default endpoint) — the capture loop has
        // nowhere to POST, so the tier must fall through exactly like a
        // dangling or apple tier instead of winning the cascade and then
        // hard-failing validate_meeting_stt_svc (adversarial-review finding).
        let cfg = AppConfig {
            model_profiles: vec![
                serde_json::json!({ "id": "endpointless", "name": "x", "provider": "custom", "model": "m" }),
                serde_json::json!({ "id": "ws-url", "name": "x", "provider": "custom", "model": "m", "base_url": "   " }),
            ],
            ..Default::default()
        };
        assert!(!meeting_stt_profile_usable(&cfg, "endpointless"));
        assert!(!meeting_stt_profile_usable(&cfg, "ws-url"));
    }

    #[test]
    fn resolve_meeting_stt_tier_endpointless_ref_falls_to_legacy_field() {
        let cfg = AppConfig {
            model_profiles: vec![
                serde_json::json!({ "id": "endpointless", "name": "x", "provider": "custom", "model": "m" }),
                serde_json::json!({ "id": "legacy-profile", "name": "x", "provider": "openai", "model": "m" }),
            ],
            ..Default::default()
        };
        assert_eq!(
            resolve_meeting_stt_tier(&cfg, Some("endpointless"), "legacy-profile"),
            Some("legacy-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_stt_tier_dangling_ref_falls_to_legacy_field() {
        let cfg = cfg_with_profiles(&[("legacy-profile", "openai")]);
        assert_eq!(
            resolve_meeting_stt_tier(&cfg, Some("ghost"), "legacy-profile"),
            Some("legacy-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_stt_tier_dangling_ref_and_dangling_legacy_falls_to_global() {
        assert_eq!(
            resolve_meeting_stt_tier(&cfg_with_profiles(&[]), Some("ghost"), "ghost2"),
            None
        );
    }

    #[test]
    fn resolve_meeting_stt_tier_ref_empty_profile_falls_to_legacy_field() {
        // Ref resolved (widget exists) but its own model_profile is empty —
        // stt.default's "use global" convention — so the legacy
        // meeting_stt_profile field wins instead of resolving "" blindly.
        let cfg = cfg_with_profiles(&[("legacy-profile", "openai")]);
        assert_eq!(
            resolve_meeting_stt_tier(&cfg, Some(""), "legacy-profile"),
            Some("legacy-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_stt_tier_ref_empty_profile_and_empty_legacy_falls_to_global() {
        assert_eq!(resolve_meeting_stt_tier(&cfg_with_profiles(&[]), Some(""), ""), None);
    }

    #[test]
    fn resolve_meeting_stt_tier_ref_apple_speech_falls_to_legacy_field() {
        // The on-device sentinel is treated exactly like an empty profile —
        // meeting's HTTP-only transcribe_http can't drive it.
        let cfg = cfg_with_profiles(&[("legacy-profile", "openai")]);
        assert_eq!(
            resolve_meeting_stt_tier(&cfg, Some("apple-speech"), "legacy-profile"),
            Some("legacy-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_stt_tier_ref_apple_speech_and_empty_legacy_falls_to_global() {
        assert_eq!(
            resolve_meeting_stt_tier(&cfg_with_profiles(&[]), Some("apple-speech"), ""),
            None
        );
    }

    #[test]
    fn resolve_meeting_stt_tier_apple_provider_ref_falls_to_legacy_field() {
        // A real apple-provider profile in the ref tier falls through exactly
        // like the sentinel does.
        let cfg =
            cfg_with_profiles(&[("scenario-apple-stt", "apple"), ("legacy-profile", "openai")]);
        assert_eq!(
            resolve_meeting_stt_tier(&cfg, Some("scenario-apple-stt"), "legacy-profile"),
            Some("legacy-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_stt_tier_apple_provider_legacy_falls_to_global() {
        // The legacy field can name an apple-provider profile too — it must
        // fall through to the global tier (None), where start()'s backstop
        // takes over, instead of winning the tier and dying in transcribe_http.
        let cfg = cfg_with_profiles(&[("scenario-apple-stt", "apple")]);
        assert_eq!(resolve_meeting_stt_tier(&cfg, None, "scenario-apple-stt"), None);
    }

    #[test]
    fn resolve_meeting_stt_tier_no_ref_falls_to_legacy_field() {
        let cfg = cfg_with_profiles(&[("legacy-profile", "openai")]);
        assert_eq!(
            resolve_meeting_stt_tier(&cfg, None, "legacy-profile"),
            Some("legacy-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_stt_tier_no_ref_and_empty_legacy_falls_to_global() {
        assert_eq!(resolve_meeting_stt_tier(&cfg_with_profiles(&[]), None, ""), None);
    }

    #[test]
    fn resolve_meeting_stt_tier_usable_ref_wins_over_legacy_field() {
        let cfg =
            cfg_with_profiles(&[("tuned-profile", "openai"), ("legacy-profile", "openai")]);
        assert_eq!(
            resolve_meeting_stt_tier(&cfg, Some("tuned-profile"), "legacy-profile"),
            Some("tuned-profile".to_string())
        );
    }

    // ── resolve_meeting_llm_widget_ref ────────────────────────────────────────

    #[test]
    fn resolve_meeting_llm_widget_ref_empty_returns_none() {
        let props = base_props();
        assert_eq!(resolve_meeting_llm_widget_ref(&props, &[]).unwrap(), None);
    }

    #[test]
    fn resolve_meeting_llm_widget_ref_resolves_model_profile() {
        let mut props = base_props();
        props.llm_widget = "llm.tuned".into();
        let widgets = vec![llm_widget_def("llm.tuned", "tuned-llm-profile")];
        assert_eq!(
            resolve_meeting_llm_widget_ref(&props, &widgets).unwrap(),
            Some("tuned-llm-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_llm_widget_ref_dangling_ref_errors() {
        let mut props = base_props();
        props.llm_widget = "llm.missing".into();
        let err = resolve_meeting_llm_widget_ref(&props, &[]).unwrap_err();
        assert!(err.contains("llm.missing"), "error should name the missing id, got: {err}");
    }

    #[test]
    fn resolve_meeting_llm_widget_ref_type_mismatch_errors() {
        let mut props = base_props();
        props.llm_widget = "stt.x".into();
        let wrong_type_widget = stt_widget_def("stt.x", "p");
        let err = resolve_meeting_llm_widget_ref(&props, &[wrong_type_widget]).unwrap_err();
        assert!(err.contains("expected 'llm'"), "error should mention type mismatch, got: {err}");
    }

    // ── resolve_meeting_stop_llm_profile ──────────────────────────────────────
    //
    // Review Fix Round 1 (Important item 2): a dangling/mismatched llm_widget
    // ref must not block `stop()` — it must still yield a usable profile id
    // (the legacy fallback chain) alongside the error to surface, rather than
    // propagating the Err and skipping stop_meeting_with entirely.

    #[test]
    fn resolve_meeting_stop_llm_profile_ok_some_wins_no_error() {
        let (profile, err) = resolve_meeting_stop_llm_profile(
            Ok(Some("tuned-llm-profile".to_string())),
            "legacy-meeting-profile",
            "legacy-llm-profile",
        );
        assert_eq!(profile, "tuned-llm-profile");
        assert_eq!(err, None);
    }

    // ── meeting_llm_profile_usable / empty-ref-model fall-through (final ────
    // review wave Critical fix — mirrors meeting_stt_profile_usable's shape)

    #[test]
    fn meeting_llm_profile_usable_rejects_empty() {
        assert!(!meeting_llm_profile_usable(""));
        assert!(meeting_llm_profile_usable("tuned-llm-profile"));
    }

    #[test]
    fn resolve_meeting_stop_llm_profile_ok_some_empty_falls_to_meeting_llm_profile() {
        // The ref resolved (no dangling/type-mismatch error) but its own
        // model_profile is empty — llm.default's "use global" convention,
        // which every builtin llm widget ships with. Must fall through to the
        // legacy chain exactly like an absent ref, not win outright as an
        // unusable "" profile id (that used to hard-error every summary).
        let (profile, err) = resolve_meeting_stop_llm_profile(
            Ok(Some(String::new())),
            "legacy-meeting-profile",
            "legacy-llm-profile",
        );
        assert_eq!(profile, "legacy-meeting-profile");
        assert_eq!(err, None);
    }

    #[test]
    fn resolve_meeting_stop_llm_profile_ok_some_empty_and_empty_legacy_falls_to_llm_profile() {
        let (profile, err) =
            resolve_meeting_stop_llm_profile(Ok(Some(String::new())), "", "legacy-llm-profile");
        assert_eq!(profile, "legacy-llm-profile");
        assert_eq!(err, None);
    }

    #[test]
    fn resolve_meeting_stop_llm_profile_ok_none_falls_to_meeting_llm_profile() {
        let (profile, err) =
            resolve_meeting_stop_llm_profile(Ok(None), "legacy-meeting-profile", "legacy-llm-profile");
        assert_eq!(profile, "legacy-meeting-profile");
        assert_eq!(err, None);
    }

    #[test]
    fn resolve_meeting_stop_llm_profile_ok_none_empty_meeting_profile_falls_to_llm_profile() {
        let (profile, err) = resolve_meeting_stop_llm_profile(Ok(None), "", "legacy-llm-profile");
        assert_eq!(profile, "legacy-llm-profile");
        assert_eq!(err, None);
    }

    #[test]
    fn resolve_meeting_stop_llm_profile_dangling_ref_degrades_to_fallback_and_surfaces_error() {
        // The case this fix targets: ref-resolution Err must still produce a
        // usable profile id (so stop() always calls stop_meeting_with and
        // flips `recording`), plus the error to show via recvMeetingError.
        let (profile, err) = resolve_meeting_stop_llm_profile(
            Err("meeting: llm_widget 'llm.missing' not found".to_string()),
            "legacy-meeting-profile",
            "legacy-llm-profile",
        );
        assert_eq!(profile, "legacy-meeting-profile");
        assert_eq!(err, Some("meeting: llm_widget 'llm.missing' not found".to_string()));
    }

    #[test]
    fn resolve_meeting_stop_llm_profile_dangling_ref_empty_meeting_profile_falls_to_llm_profile() {
        let (profile, err) = resolve_meeting_stop_llm_profile(
            Err("meeting: llm_widget 'llm.missing' not found".to_string()),
            "",
            "legacy-llm-profile",
        );
        assert_eq!(profile, "legacy-llm-profile");
        assert!(err.is_some());
    }
}
