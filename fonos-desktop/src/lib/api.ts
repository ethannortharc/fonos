// Typed Tauri IPC wrappers for all 26 Fonos commands.
// Uses @tauri-apps/api v2 — no raw __TAURI_INTERNALS__ calls.

import { invoke } from "@tauri-apps/api/core";
import type {
  LatencyStats,
  AgentResult,
  AppConfig,
  DailyStat,
  DoctorFinding,
  DoctorFix,
  LlmResult,
  ModeEntry,
  ModelCaps,
  SaveModeOptions,
  SavedScenario,
  ScanResult,
  ScenarioProbe,
  SkillInfo,
  SttResult,
  TodaySummary,
  TtsResult,
  VoiceEntry,
  VoiceList,
} from "../types";

// ─── Dictation ────────────────────────────────────────────────────────────────

/** Check if a microphone is available and accessible. */
export async function hasMicrophone(): Promise<boolean> {
  return invoke<boolean>("has_microphone");
}

// ─── Permissions ──────────────────────────────────────────────────────────────

/** Whether the app is trusted for Accessibility (hotkeys + text injection). */
export async function checkAccessibility(): Promise<boolean> {
  return invoke<boolean>("check_accessibility");
}

/** Open a System Settings privacy pane. Pane is one of:
 *  "microphone" | "accessibility" | "speech_recognition" | "screen_recording". */
export async function openSettingsPane(pane: string): Promise<void> {
  return invoke<void>("open_settings_pane", { pane });
}

/** Start capturing audio from the microphone. */
export async function startRecording(): Promise<void> {
  return invoke<void>("start_recording");
}

/** Stop recording, transcribe audio, and return the result.
 *  Pass modeOverride to use a specific mode (e.g. from Dictation view testing)
 *  instead of config.dictation_mode (used by float pill / hotkey). */
export async function stopRecording(modeOverride?: string): Promise<SttResult> {
  return invoke<SttResult>("stop_recording", modeOverride ? { modeOverride } : {});
}

/** Transcribe an audio file at the given path. */
export async function transcribeFile(path: string): Promise<string> {
  return invoke<string>("transcribe_file", { path });
}

// ─── TTS ──────────────────────────────────────────────────────────────────────

/** Synthesize speech and return raw WAV bytes as a number array. */
export async function synthesizeSpeech(
  text: string,
  voice: string,
  speed: number
): Promise<number[]> {
  return invoke<number[]>("synthesize_speech", { text, voice, speed });
}

/** Generate speech AND play it in one call. */
export async function generateAndPlay(
  text: string,
  voice: string,
  speed: number
): Promise<TtsResult> {
  return invoke<TtsResult>("generate_and_play", { text, voice, speed });
}

/** Play a WAV file from disk by path. */
export async function playAudioFile(path: string): Promise<void> {
  return invoke<void>("play_audio_file", { path });
}

/** Decode WAV bytes and play through the default output device. */
export async function playSpeech(audioData: number[]): Promise<void> {
  return invoke<void>("play_speech", { audioData });
}

/** Stop playback immediately. */
export async function stopPlayback(): Promise<void> {
  return invoke<void>("stop_playback");
}

/** Pause playback at the current position. */
export async function pausePlayback(): Promise<void> {
  return invoke<void>("pause_playback");
}

/** Resume a paused playback. */
export async function resumePlayback(): Promise<void> {
  return invoke<void>("resume_playback");
}

// ─── Voices ───────────────────────────────────────────────────────────────────

/** List all saved voices (local storage). */
export async function listVoices(): Promise<VoiceList> {
  return invoke<VoiceList>("list_voices");
}

/** Clone a voice by saving audio locally. */
export async function cloneVoice(
  name: string,
  audioPath: string
): Promise<VoiceEntry> {
  return invoke<VoiceEntry>("clone_voice", { name, audioPath });
}

/** Delete a saved voice by ID. */
export async function deleteVoice(voiceId: string): Promise<void> {
  return invoke<void>("delete_voice", { voiceId });
}

/** Preview a voice by playing back its saved recording. */
export async function previewVoice(
  voiceId: string,
  text: string
): Promise<void> {
  return invoke<void>("preview_voice", { voiceId, text });
}

/** Open native file picker for audio files. Returns path or null if cancelled. */
export async function pickAudioFile(): Promise<string | null> {
  return invoke<string | null>("pick_audio_file");
}

/** Record audio from mic for voice cloning. Returns path to WAV file. */
export async function recordVoiceSample(
  durationSecs: number
): Promise<string> {
  return invoke<string>("record_voice_sample", { durationSecs });
}

// ─── LLM & Modes ─────────────────────────────────────────────────────────────

/** Process text through the configured LLM using the specified mode. */
export async function processWithLlm(
  text: string,
  mode: string
): Promise<LlmResult> {
  return invoke<LlmResult>("process_with_llm", { text, mode });
}

/** List all available audio input devices. */
export async function listAudioInputs(): Promise<string[]> {
  return invoke<string[]>("list_audio_inputs");
}

/** Probe the configured model's capabilities and cache results. */
export async function probeModel(): Promise<ModelCaps> {
  return invoke<ModelCaps>("probe_model");
}

/** Query a provider's /v1/models endpoint to list available models. */
export async function listProviderModels(baseUrl: string, apiKey: string): Promise<{ id: string; owned_by: string }[]> {
  return invoke<{ id: string; owned_by: string }[]>("list_provider_models", { baseUrl, apiKey });
}

/** Verify a model's STT endpoint with a silent probe clip. Resolves with an OK
 *  message, or rejects with the endpoint error (404, auth, network, …). */
export async function testStt(profileId: string): Promise<string> {
  return invoke<string>("test_stt", { profileId });
}

/** List all modes (built-in + custom). */
export async function listModes(): Promise<ModeEntry[]> {
  return invoke<ModeEntry[]>("list_modes");
}

/** Save a custom mode. */
export async function saveCustomMode(opts: SaveModeOptions): Promise<void> {
  return invoke<void>("save_custom_mode", {
    id: opts.id,
    name: opts.name,
    description: opts.description ?? "",
    icon: opts.icon ?? "",
    system: opts.system ?? "",
    userTemplate: opts.user_template ?? "",
    temperature: opts.temperature ?? 0.1,
    model: opts.model ?? "",
    sttModel: opts.stt_model ?? "",
    sttPrompt: opts.stt_prompt ?? "",
    sttTemperature: opts.stt_temperature ?? 0,
    maxTokens: opts.max_tokens ?? 4096,
    outputLanguage: opts.output_language ?? "auto",
    autoPaste: opts.auto_paste !== false,
    autoPressEnter: opts.auto_press_enter === true,
    vocabBooks: opts.vocab_books ?? [],
  });
}

/** Delete a custom mode by ID. */
export async function deleteCustomMode(id: string): Promise<void> {
  return invoke<void>("delete_custom_mode", { id });
}

// ─── Setup Doctor (issue #30) ───────────────────────────────────────────────

/** Run all config-health checks (config lint + endpoint/permission/RTF probes). */
export async function runDoctor(): Promise<DoctorFinding[]> {
  return invoke<DoctorFinding[]>("run_doctor");
}

/** Apply one doctor fix, then re-run the doctor to refresh the card. */
export async function applyDoctorFix(fix: DoctorFix): Promise<void> {
  return invoke<void>("apply_doctor_fix", { fix });
}

// ─── Scenario setup (issue #29) ─────────────────────────────────────────────

/** Probe a server's /v1/models endpoint (used for card detection). */
export async function scanModels(baseUrl: string, apiKey: string): Promise<ScanResult> {
  return invoke<ScanResult>("scan_models", { baseUrl, apiKey });
}

/** Scan + classify + measure TTS speeds + build a default plan for a server. */
export async function scenarioProbe(
  baseUrl: string,
  apiKey: string,
  voice?: string
): Promise<ScenarioProbe> {
  return invoke<ScenarioProbe>("scenario_probe", { baseUrl, apiKey, voice: voice ?? null });
}

/** Snapshot the live config as a new saved scenario, capturing the chosen
 *  sections (models / dictation / speech). */
export async function saveScenario(
  name: string,
  includeModels: boolean,
  includeDictation: boolean,
  includeSpeech: boolean,
  includeVocab: boolean,
  includeHotkeys: boolean
): Promise<SavedScenario> {
  return invoke<SavedScenario>("save_scenario", {
    name,
    includeModels,
    includeDictation,
    includeSpeech,
    includeVocab,
    includeHotkeys,
  });
}

/** Apply a saved scenario by id (upsert profiles + restore assignments). */
export async function applySavedScenario(id: string): Promise<void> {
  return invoke<void>("apply_saved_scenario", { id });
}

/** Delete a saved scenario by id. */
export async function deleteSavedScenario(id: string): Promise<void> {
  return invoke<void>("delete_saved_scenario", { id });
}

/** Write a scenario JSON blob to ~/Downloads and return the full path. */
export async function exportScenario(scenarioJson: string, name: string): Promise<string> {
  return invoke<string>("export_scenario", { scenarioJson, name });
}

/** Validate + import a scenario from raw JSON text (drag-drop path). */
export async function importScenarioJson(json: string): Promise<SavedScenario> {
  return invoke<SavedScenario>("import_scenario_json", { json });
}

/** Validate + import a scenario from a file path. */
export async function importScenario(path: string): Promise<SavedScenario> {
  return invoke<SavedScenario>("import_scenario", { path });
}

// ─── Stats & History ──────────────────────────────────────────────────────────

/** Record a new event and return the row ID. */
export async function recordEvent(
  eventType: string,
  inputText: string,
  outputText: string,
  durationSecs: number,
  latencyMs: number,
  mode: string,
  model: string,
  voice: string,
  audioPath: string,
  tokensIn: number | null,
  tokensOut: number | null,
  sessionId: string | null
): Promise<number> {
  return invoke<number>("record_event", {
    eventType,
    inputText,
    outputText,
    durationSecs,
    latencyMs,
    mode,
    model,
    voice,
    audioPath,
    tokensIn,
    tokensOut,
    sessionId,
  });
}

/** Get daily statistics for a date range (inclusive). */
export async function getStats(
  dateFrom: string,
  dateTo: string
): Promise<DailyStat[]> {
  return invoke<DailyStat[]>("get_stats", { dateFrom, dateTo });
}

/** Get today's aggregated summary. */
/** End-to-end dictation latency percentiles over [from, to] (YYYY-MM-DD). */
export async function getDictationLatency(from: string, to: string): Promise<LatencyStats> {
  return invoke<LatencyStats>("get_dictation_latency", { date_from: from, date_to: to });
}

export async function getToday(): Promise<TodaySummary> {
  return invoke<TodaySummary>("get_today");
}

// ─── Config ───────────────────────────────────────────────────────────────────

/** Return the current application configuration. */
export async function getConfig(): Promise<AppConfig> {
  return invoke<AppConfig>("get_config");
}

/** Merge the provided JSON fields into the config and persist to disk. */
export async function saveConfig(configJson: string): Promise<void> {
  return invoke<void>("save_config", { configJson });
}

// ─── Window ───────────────────────────────────────────────────────────────────

/** Resize the float window. */
export async function resizeFloat(
  width: number,
  height: number
): Promise<void> {
  return invoke<void>("resize_float", { width, height });
}

// ─── Selection ───────────────────────────────────────────────────────────────

export interface SelectionContext {
  text: string;
  app_name: string;
  editable: boolean;
}

/** Grab the currently selected text from the frontmost app (Cmd+C under the hood). */
export async function grabSelection(): Promise<SelectionContext> {
  return invoke<SelectionContext>("grab_selection");
}

/** Replace the current selection in the target app with new text (Cmd+V). */
export async function replaceSelection(text: string, targetApp?: string): Promise<void> {
  return invoke<void>("replace_selection", { text, targetApp });
}

// ─── Agent ────────────────────────────────────────────────────────────────────

/** Send text to the agent processor and get a response with skill executions. */
export async function agentProcess(text: string): Promise<AgentResult> {
  return invoke<AgentResult>("agent_process", { text });
}

/** Reset the agent's conversation context. */
export async function agentReset(): Promise<void> {
  return invoke<void>("agent_reset");
}

/** List all available skills (built-in + custom) with their enabled status. */
export async function listSkills(): Promise<SkillInfo[]> {
  return invoke<SkillInfo[]>("list_skills");
}

/** Enable or disable a skill by ID. */
export async function toggleSkill(id: string, enabled: boolean): Promise<void> {
  return invoke<void>("toggle_skill", { id, enabled });
}

/** Save a custom skill definition (JSON string). */
export async function saveCustomSkill(jsonStr: string): Promise<void> {
  return invoke<void>("save_custom_skill", { jsonStr });
}

/** Full stored definition of a custom skill, as persisted in its JSON file. */
export interface CustomSkillConfig {
  name: string;
  description: string;
  icon?: string | null;
  skill_type: string;
  command?: string | null;
  url?: string | null;
  script?: string | null;
  parameters: Record<string, { description: string; default?: string | null }>;
  response_template?: string | null;
}

/** Fetch a custom skill's full definition so an edit form can be pre-filled. */
export async function getCustomSkill(id: string): Promise<CustomSkillConfig> {
  return invoke<CustomSkillConfig>("get_custom_skill", { id });
}

/** Delete a custom skill by ID. */
export async function deleteCustomSkill(id: string): Promise<void> {
  return invoke<void>("delete_custom_skill", { id });
}

/** Test a skill with sample input and return the output string. */
export async function testSkill(id: string, input: string): Promise<string> {
  return invoke<string>("test_skill", { id, input });
}

// ── STS conversation (issue #24) ──────────────────────────────────────────

export async function stsPageStart(): Promise<void> {
  return invoke<void>("sts_page_start");
}

export async function stsPageStop(persona?: string): Promise<string> {
  return invoke<string>("sts_page_stop", { persona: persona ?? null });
}

export async function getStsHistory(): Promise<[string, string][]> {
  return invoke<[string, string][]>("get_sts_history");
}

export async function resetStsSession(): Promise<void> {
  return invoke<void>("reset_sts_session");
}

/** Start a hands-free call (listen → reply loop) until hung up. */
export async function callStart(): Promise<void> {
  return invoke<void>("call_start");
}

/** Hang up the hands-free call. Safe to call in any phase. */
export async function callStop(): Promise<void> {
  return invoke<void>("call_stop");
}

export async function listModelVoices(profileId: string): Promise<string[]> {
  return invoke<string[]>("list_model_voices", { profileId });
}
