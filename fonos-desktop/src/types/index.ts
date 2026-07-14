// TypeScript interfaces matching Rust structs from fonos-core.
// Field names use camelCase matching serde serialization from Rust.

// ─── Config ───────────────────────────────────────────────────────────────────

/** One text-action row: hotkey → mode → output target (mirrors Rust TextActionBinding). */
export interface TextActionBinding {
  hotkey: string;
  mode_id: string;
  output_target: "floating_popup" | "active_text_field" | "clipboard" | "append_to_container" | "none";
}

/** Application configuration, persisted to disk as JSON. */
export interface AppConfig {
  hotkey_dictation: string;
  hotkey_dictation_toggle?: string;
  hotkey_tts: string;
  hotkey_agent?: string;
  hotkey_agent_panel?: string;
  dictation_mode: string;
  default_voice: string;
  tts_speed: number;
  audio_input_device: string;
  audio_output_device: string;
  show_floating_indicator: boolean;
  warmup_enabled?: boolean;
  ui_language?: "auto" | "en" | "zh";
  hotkey_listen?: string;
  // DEPRECATED (Workbench P2 T10): still a real Rust config field, but no
  // longer read — Listen always resolves the built-in `llm.listen` widget
  // instead (ScenariosTab snapshots still carry it).
  listen_mode?: string;
  listen_voice_profile?: string;
  listen_voice?: string;
  // Legacy STS/call fields (Workbench P2 T9): still real Rust config fields,
  // but DEPRECATED — one-time-migrated into the wf.call recipe /
  // call.default widget props and no longer settings-tab-editable.
  // ScenariosTab's models-section snapshot (ScenarioAssignments) still
  // carries sts_voice_profile/sts_voice; its speech-section snapshot
  // (SpeechSection) dropped them in Task 14 — see SpeechSection's own
  // comment.
  hotkey_sts?: string;
  sts_persona?: string;
  sts_llm_profile?: string;
  sts_voice_profile?: string;
  sts_voice?: string;
  sts_max_turns?: number;
  call_vad_sensitivity?: number;
  call_vad_silence_ms?: number;
  call_barge_in?: boolean;
  stt_language: string;
  model_profiles: ModelProfile[];
  stt_profile: string;
  tts_profile: string;
  llm_profile: string;
  clean_prompt: string;
  translate_source: string;
  translate_target: string;
  // Agent fields (added by wp-09)
  agent_llm_profile?: string;
  agent_system_prompt?: string;
  agent_safety_allowlist?: string[];
  agent_safety_blocklist?: string[];
  agent_timeout_secs?: number;
  agent_max_turns?: number;
  agent_tts_enabled?: boolean;
  agent_stt_profile?: string;
  // Note fields
  hotkey_note?: string;
  hotkey_note_1?: string;
  hotkey_note_2?: string;
  hotkey_note_3?: string;
  note_processor?: string;
  note_stt_profile?: string;
  note_llm_profile?: string;
  note_prompt?: string;
  // Notebook → hotkey bindings (container IDs)
  notebook_hotkey_1?: number;
  notebook_hotkey_2?: number;
  notebook_hotkey_3?: number;
  // Meeting fields. meeting_audio_source was never a real Rust config field
  // (dead — MeetingTab.tsx wrote it but nothing read it back); removed
  // alongside MeetingTab.tsx's deletion (Workbench P2 Task 7). The other
  // three remain real (if no longer settings-tab-editable) config fields —
  // see their doc comments in fonos-core's AppConfig.
  meeting_stt_profile?: string;
  meeting_llm_profile?: string;
  meeting_summary_prompt?: string;
  hotkey_meeting?: string;
  // Quick transform
  hotkey_transform?: string;
  transform_mode?: string;
  // Text actions
  text_actions?: TextActionBinding[];
  // Text injection
  injection_strategy?: string;
  injection_app_overrides?: InjectionAppOverride[];
  // Onboarding — gates the first-run wizard
  has_completed_onboarding?: boolean;
  vocab_books?: VocabBook[];
  global_vocab_books?: string[];
  // Saved scenarios (issue #29)
  saved_scenarios?: SavedScenario[];
  // Workflow engine (Workflow P1) — components + pipelines that supersede modes
  widgets?: WidgetDef[];
  workflows?: WorkflowDef[];
  workflow_migration_done?: boolean;
  triggers_migration_done?: boolean;
  /** Id of the voice workflow the pill hotkey triggers; empty falls back to
   *  the built-in "wf.dictation". Set by the Dictation drum / float pill. */
  active_voice_workflow?: string;
  /** HuggingFace endpoint override for diarization model downloads (empty =
   *  official). E.g. "https://hf-mirror.com". */
  diarization_hf_endpoint?: string;
  /** Global hotkey owned by the floating pill (Workbench P1, spec §3c):
   *  pressing it runs the pill roller's currently selected workflow
   *  (active_voice_workflow, falling back to wf.dictation). Empty = unset. */
  pill_hotkey?: string;
  /** Key behavior for the pill hotkey. */
  pill_hotkey_capture?: "hold" | "toggle";
  pill_hotkey_migration_done?: boolean;
}

/** A per-app override for the text injection strategy. */
export interface InjectionAppOverride {
  app: string;
  strategy: string;
}

/** Mirrors fonos_core::vocab::VocabRule */
export interface VocabRule {
  from: string;
  to: string;
  kind: "literal" | "regex";
  case_insensitive: boolean;
}

/** Mirrors fonos_core::vocab::VocabBook */
export interface VocabBook {
  id: string;
  name: string;
  enabled: boolean;
  terms: string[];
  rules: VocabRule[];
}

/** A named model profile entry within AppConfig.model_profiles. */
export interface ModelProfile {
  id: string;
  name: string;
  provider: string;
  model: string;
  api_key?: string;
  base_url?: string;
  capabilities?: string[];
  /** STT API path: "whisper" (multipart /v1/audio/transcriptions) or "chat" (base64 audio in chat completions). Default: "whisper". */
  stt_api?: "whisper" | "chat";
}

// ─── Scenario setup (issue #29) ─────────────────────────────────────────────

/** Result of scan_models — probing a server's /v1/models endpoint. */
export interface ScanResult {
  reachable: boolean;
  latency_ms: number;
  models: string[];
}

/** STT / LLM / TTS candidate buckets — mirrors fonos_core::scenarios::ClassifiedModels. */
export interface ClassifiedModels {
  stt: string[];
  llm: string[];
  tts: string[];
}

/** Auto-assigned role → model plan — mirrors fonos_core::scenarios::ModelPlan. */
export interface ModelPlan {
  stt: string | null;
  llm: string | null;
  conversation_tts: string | null;
  listen_tts: string | null;
}

/** Full step-2 probe result — mirrors commands::scenarios::ScenarioProbe. */
export interface ScenarioProbe {
  reachable: boolean;
  latency_ms: number;
  models: string[];
  classified: ClassifiedModels;
  tts_rtfs: Record<string, number>;
  plan: ModelPlan;
}

/** Two-layer engine detection — mirrors commands::engine_setup::EngineDetection. */
export interface EngineDetection {
  engine: string;
  running: boolean;
  installed: boolean;
  url: string;
  /** Raw signals behind the verdict — machine tokens the UI maps to labels:
   *  "path" (binary on PATH), "app" (app bundle), "process" (brand process),
   *  "port" (base URL answered). Empty means nothing was detected. */
  evidence: string[];
}

/** Hardware facts + derived tier — mirrors commands::engine_setup::HardwareInfo.
 *  `tier` mirrors fonos_core::engine_setup::HardwareTier, which serializes
 *  lowercase (`#[serde(rename_all = "lowercase")]`). */
export interface HardwareInfo {
  mem_bytes: number;
  chip: string;
  has_nvidia_gpu: boolean;
  tier: "light" | "balanced" | "max";
}

/** Free disk space — mirrors commands::engine_setup::DiskInfo. */
export interface DiskInfo {
  available_kb: number;
}

/** Confirmed setup plan — mirrors commands::engine_setup::SetupPlanDto. */
export interface SetupPlan {
  engine: string;
  install: boolean;
  start: boolean;
  pulls: string[];
  base_url: string;
}

/** One `engine:setup` progress event (JSON-parsed payload — the event itself
 *  carries a JSON string, mirroring the `diarize:download` double-parse
 *  pattern; parse with `JSON.parse(e.payload) as EngineSetupEvent`).
 *
 *  Mirrors every `emit_setup`/`emit_error` call site in
 *  commands::engine_setup: `engine` is present on every variant (not just
 *  errors). `failed_stage` is only set when `stage === "error"` and
 *  includes `"busy"` for the re-entrancy rejection (a second `engine_setup`
 *  invocation while one is already running). */
export interface EngineSetupEvent {
  stage: "install" | "start" | "wait" | "pull" | "manual" | "done" | "error";
  engine: string;
  pct?: number;
  model?: string;
  message?: string;
  failed_stage?: "install" | "start" | "wait" | "pull" | "busy";
}

/** Default-service assignments captured in a SavedScenario's models section. */
export interface ScenarioAssignments {
  stt_profile: string;
  llm_profile: string;
  tts_profile: string;
  sts_voice_profile: string;
  listen_voice_profile: string;
  sts_voice: string;
  listen_voice: string;
}

/** The models section — profiles + role assignments. */
export interface ModelsSection {
  profiles: ModelProfile[];
  assignments: ScenarioAssignments;
}

/** The dictation section — workflow/widget overlays + config fields (Workbench
 *  P2 Task 11: superseded the modes.json-shaped snapshot; the engine world
 *  has been the source of truth since Workflow P1). */
export interface DictationSection {
  /** DEPRECATED: opaque legacy `modes.json`-shaped blob (id → mode), exactly
   *  as persisted before Workbench P2 Task 11. `null` on every snapshot from
   *  that task onward — present only for reading a pre-Task-11 scenario
   *  file, which the backend converts into `llm.*` processor widgets on
   *  apply rather than writing it back. The frontend never reads its
   *  contents (the `Mode` shape it used to carry was deleted along with the
   *  legacy `modes` system in Workbench P2 Task 12), so it's untyped JSON
   *  here rather than a resurrected `Mode` interface. */
  user_modes: Record<string, unknown> | null;
  /** User workflow overlays (config.workflows verbatim). */
  user_workflows: WorkflowDef[];
  /** User widget overlays (config.widgets verbatim). */
  user_widgets: WidgetDef[];
  dictation_mode: string;
  translate_target: string;
}

/** The speech section — Listen + the still-live-read STS/call fields.
 *  Audited for Workbench P2 Task 11: `listen_mode`, `sts_persona`, and
 *  `sts_max_turns` were dropped — no reader anywhere in the app. Two stay:
 *  `listen_voice_profile`/`listen_voice` (Listen synthesis) and
 *  `sts_llm_profile` (still read by `call.default`'s fallback chain).
 *  `sts_voice_profile`/`sts_voice` were also dropped in Workbench P2 Task 14
 *  — they were kept past Task 11 only because the Setup Doctor's
 *  conversation-RTF probe read them directly; that probe now reads
 *  `call.default`'s own `CallProps` instead, so restoring these two via a
 *  scenario no longer does anything (the models section's
 *  `ScenarioAssignments.sts_voice_profile`/`.sts_voice` are unrelated and
 *  unaffected). */
export interface SpeechSection {
  listen_voice_profile: string;
  listen_voice: string;
  sts_llm_profile: string;
}

/** The vocab section — custom vocabulary books + globally-applied book ids. */
export interface VocabSection {
  vocab_books: VocabBook[];
  global_vocab_books: string[];
}

/** The hotkeys section — every global + notebook hotkey binding.
 *
 *  Audited for Workbench P2 Task 11 (the "T6 mandate": old scenarios must not
 *  write back dead hotkey fields): `hotkey_agent`/`hotkey_agent_panel`
 *  (Task 6), `hotkey_meeting` (Task 7), and `hotkey_sts` (Task 9) are each
 *  one-time-folded into a Hotkey trigger chip on the matching recipe and then
 *  cleared, with no reader left anywhere — dropped from the section entirely.
 *  `hotkey_transform` stays: it still drives a real reconciliation with
 *  `text_actions` on every apply. */
export interface HotkeysSection {
  hotkey_dictation: string;
  hotkey_dictation_toggle: string;
  hotkey_tts: string;
  hotkey_note: string;
  hotkey_note_1: string;
  hotkey_note_2: string;
  hotkey_note_3: string;
  notebook_hotkey_1: number;
  notebook_hotkey_2: number;
  notebook_hotkey_3: number;
  hotkey_transform: string;
  hotkey_listen: string;
  /** `undefined`/`null` = scenario predates text actions (apply leaves current
   *  bindings untouched); present (even `[]`) = apply verbatim. */
  text_actions?: TextActionBinding[] | null;
}

/** A saved, switchable configuration bundle — mirrors
 *  fonos_core::scenarios::SavedScenario. Sectioned: each of models / dictation /
 *  speech / vocab / hotkeys is optional and present only when the save
 *  included it. */
export interface SavedScenario {
  id: string;
  name: string;
  created_at: string;
  models?: ModelsSection;
  dictation?: DictationSection;
  speech?: SpeechSection;
  vocab?: VocabSection;
  hotkeys?: HotkeysSection;
}

// ─── Setup Doctor (issue #30) ───────────────────────────────────────────────

/** Severity of a doctor Finding — mirrors fonos_core::doctor::Severity. */
export type DoctorSeverity = "pass" | "warn" | "advise";

/** A typed one-click fix — mirrors fonos_core::doctor::FixAction (tag: "kind").
 *  `reset_listen_mode`/`point_mode_model_to_default` were retired in
 *  Workbench P2 Task 11 along with the mode-system checks that produced them. */
export type DoctorFix =
  | { kind: "attach_book_global"; book_id: string }
  | { kind: "clear_profile_ref"; field: string }
  | { kind: "switch_tts_model"; profile_id: string; model: string }
  | { kind: "open_settings_pane"; pane: string };

/** One doctor result row — mirrors fonos_core::doctor::Finding. */
export interface DoctorFinding {
  id: string;
  severity: DoctorSeverity;
  message_key: string;
  message_params: string[];
  fix: DoctorFix | null;
}

// ─── Workflow (Workflow P1) ─────────────────────────────────────────────────
// Component model that supersedes Modes: a workflow wires one source widget
// through zero or more processor widgets to one or more output widgets.

/** The role a widget plays in a pipeline — mirrors
 *  fonos_core::workflow::model::WidgetRole. */
export type WidgetRole = "source" | "processor" | "output";

/** A configured widget instance — mirrors
 *  fonos_core::workflow::model::WidgetDef. `props` is interpreted per
 *  `type_tag` by the registry (e.g. an `llm` widget's props carry
 *  system/user_template/temperature/max_tokens/output_language/vocab_books;
 *  a `microphone` source's props carry `capture`: "hold" | "toggle"). */
export interface WidgetDef {
  /** Globally unique id; builtins use a role prefix, e.g. "src.selection",
   *  "stt.default", "llm.polish", "out.insert". */
  id: string;
  role: WidgetRole;
  /** Which registered component implementation to instantiate, e.g.
   *  "selection" | "instant" | "microphone" | "stt" | "llm" | "insert" |
   *  "replace" | "clipboard" | "notebook" | "speak" | "panel" | "uppercase". */
  type_tag: string;
  name: string;
  icon?: string;
  props?: Record<string, unknown>;
  /** Whether this widget ships with the app (builtins cannot be deleted). */
  builtin?: boolean;
}

/** Usage-side trigger chip attached to a workflow. */
export type Trigger =
  | {
      kind: "hotkey";
      combo: string;
      /** Only meaningful for microphone-source workflows. Absent = "hold". */
      capture?: "hold" | "toggle";
    }
  | { kind: "pill_slot"; order?: number };

/** A configured workflow: a source, an ordered processor chain, and one or
 *  more outputs, referenced by widget id — mirrors
 *  fonos_core::workflow::model::WorkflowDef. */
export interface WorkflowDef {
  /** Globally unique id; builtins use fixed ids (e.g. "wf.dictation"),
   *  custom workflows use "wf.custom-{uuid}". */
  id: string;
  name: string;
  icon?: string;
  /** DEPRECATED — legacy single hotkey; superseded by triggers. */
  hotkey?: string;
  /** Usage-side triggers. Replaces the legacy `hotkey` field. */
  triggers?: Trigger[];
  /** Id of the WidgetDef used as this workflow's source. */
  source: string;
  /** Ids of the WidgetDefs used as this workflow's processors, in order. */
  processors?: string[];
  /** Ids of the WidgetDefs used as this workflow's outputs, in delivery
   *  order. Must be non-empty; enforced by the engine. */
  outputs: string[];
  /** Whether this workflow ships with the app (builtins cannot be deleted). */
  builtin?: boolean;
}

/** A workflow row as returned by list_workflows: the WorkflowDef flattened,
 *  plus the `type_tag` of its source widget (`""` if the source id no longer
 *  resolves) — mirrors commands::workflow_cfg::WorkflowRow. Lets the
 *  Dictation drum filter microphone workflows without re-resolving widgets. */
export interface WorkflowRow extends WorkflowDef {
  source_type_tag: string;
}

// ─── Model Caps ───────────────────────────────────────────────────────────────

/** Cached capability flags for a specific LLM model. */
export interface ModelCaps {
  model_id: string;
  follows_system_prompt: boolean;
  preserves_language: boolean;
  probed_at: string;
}

// ─── Stats / History ─────────────────────────────────────────────────────────

/** A single recorded event (STT, TTS, or LLM interaction). */
export interface Event {
  id: number;
  type: string;
  created_at: string;
  date: string;
  input_text: string;
  output_text: string;
  words_in: number;
  words_out: number;
  duration_secs: number;
  latency_ms: number;
  mode: string;
  model: string;
  voice: string;
  audio_path: string;
  tokens_in: number;
  tokens_out: number;
  session_id: string;
}

/** Daily aggregated statistics for a single calendar date. */
export interface DailyStat {
  date: string;
  stt_count: number;
  stt_seconds: number;
  stt_words: number;
  tts_count: number;
  tts_words: number;
  llm_count: number;
  llm_latency_total: number;
  tokens_total: number;
  time_saved_secs: number;
}

/** Summary of activity for today. */
export interface TodaySummary {
  time_saved_secs: number;
  total_words: number;
  total_sessions: number;
  stt_count: number;
  stt_words: number;
  stt_seconds: number;
  tts_count: number;
  tts_words: number;
  llm_count: number;
  llm_latency_avg: number;
  tokens_total: number;
}

// ─── STT ─────────────────────────────────────────────────────────────────────

/** Result from stop_recording command. */
export interface SttResult {
  text: string;
  audio_path: string;
  latency_ms: number;
  duration_secs: number;
  /** For Apple Speech: "on-device" or "server". Empty for HTTP providers. */
  stt_engine: string;
  /** Low-frequency noise removed by HPF, as percentage of total energy. */
  noise_removed_pct: number;
  /** Normalization gain applied in dB. */
  gain_db: number;
}

// ─── TTS ─────────────────────────────────────────────────────────────────────

/** Result from generate_and_play command. */
export interface TtsResult {
  duration_secs: number;
  latency_ms: number;
  size_bytes: number;
  audio_path: string;
}

// ─── Voice ───────────────────────────────────────────────────────────────────

/** A locally stored voice entry. */
export interface Voice {
  id: string;
  name: string;
  audio_path: string;
  created_at: string;
}

/** The shape of list_voices response entries (includes default voice). */
export interface VoiceEntry {
  voice_id: string;
  name: string;
  status: string;
  audio_path?: string;
}

/** Response from list_voices command. */
export interface VoiceList {
  voices: VoiceEntry[];
}

// ─── Record Event options ────────────────────────────────────────────────────

/** Options for the record_event command. */
export interface RecordEventOptions {
  event_type: string;
  input_text: string;
  output_text: string;
  duration_secs: number;
  latency_ms: number;
  mode: string;
  model: string;
  voice: string;
  audio_path: string;
  tokens_in?: number | null;
  tokens_out?: number | null;
  session_id?: string | null;
}

// ─── Agent ────────────────────────────────────────────────────────────────────

/** A single skill execution step within an agent response. */
export interface SkillExecution {
  skill_name: string;
  params: Record<string, unknown>;
  output: string;
  latency_ms: number;
  blocked: boolean;
}

/** Result returned by the agent_process command. */
export interface AgentResult {
  response_text: string;
  skill_executions: SkillExecution[];
}

/** Metadata for a skill as returned by list_skills. */
export interface SkillInfo {
  id: string;
  name: string;
  description: string;
  skill_type: string;
  enabled: boolean;
  builtin: boolean;
  parameters: SkillParamInfo[];
}

export interface SkillParamInfo {
  name: string;
  description: string;
  required: boolean;
  default_value: string | null;
}

/** Mirrors fonos_core::stats::LatencyModelStat */
export interface LatencyModelStat {
  model: string;
  count: number;
  p50_ms: number;
  p95_ms: number;
}

/** Mirrors fonos_core::stats::LatencyStats */
export interface LatencyStats {
  count: number;
  p50_ms: number;
  p95_ms: number;
  avg_ms: number;
  min_ms: number;
  max_ms: number;
  by_model: LatencyModelStat[];
}
