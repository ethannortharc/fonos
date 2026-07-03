// TypeScript interfaces matching Rust structs from fonos-core.
// Field names use camelCase matching serde serialization from Rust.

// ─── Config ───────────────────────────────────────────────────────────────────

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
  // Meeting fields
  meeting_stt_profile?: string;
  meeting_llm_profile?: string;
  meeting_summary_prompt?: string;
  meeting_audio_source?: string;
  hotkey_meeting?: string;
  // Quick transform
  hotkey_transform?: string;
  transform_mode?: string;
  // Text injection
  injection_strategy?: string;
  injection_app_overrides?: InjectionAppOverride[];
  // Onboarding — gates the first-run wizard
  has_completed_onboarding?: boolean;
}

/** A per-app override for the text injection strategy. */
export interface InjectionAppOverride {
  app: string;
  strategy: string;
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

// ─── Modes ────────────────────────────────────────────────────────────────────

/** A processing mode that defines how spoken text is transformed by an LLM. */
export interface Mode {
  name: string;
  description: string;
  icon: string;
  system: string | null;
  user_template: string | null;
  temperature: number;
  model: string;
  stt_model: string;
  stt_prompt: string;
  stt_temperature: number;
  max_tokens: number;
  output_language: string;
  auto_paste: boolean;
  auto_press_enter: boolean;
}

/** A mode entry as returned by list_modes — includes id and builtin flag. */
export interface ModeEntry extends Mode {
  id: string;
  builtin: boolean;
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

// ─── LLM ─────────────────────────────────────────────────────────────────────

/** Result from process_with_llm command. */
export interface LlmResult {
  original: string;
  processed: string;
  mode: string;
  mode_name: string;
  latency_ms: number;
  auto_paste: boolean;
  auto_press_enter: boolean;
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

/** Options for the save_custom_mode command. */
export interface SaveModeOptions {
  id: string;
  name: string;
  description?: string;
  icon?: string;
  system?: string;
  user_template?: string;
  temperature?: number;
  model?: string;
  stt_model?: string;
  stt_prompt?: string;
  stt_temperature?: number;
  max_tokens?: number;
  output_language?: string;
  auto_paste?: boolean;
  auto_press_enter?: boolean;
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
