import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

const now = new Date("2026-06-17T21:24:00-07:00");

const iso = (minutesAgo: number) =>
  new Date(now.getTime() - minutesAgo * 60_000).toISOString();

const modelProfiles = [
  {
    id: "openai-gpt-4o-mini-transcribe",
    name: "OpenAI Whisper",
    provider: "openai",
    model: "gpt-4o-mini-transcribe",
    base_url: "https://api.openai.com",
    capabilities: ["stt"],
  },
  {
    id: "openrouter-gemini",
    name: "OpenRouter Gemini",
    provider: "openrouter",
    model: "google/gemini-2.5-flash",
    base_url: "https://openrouter.ai/api/v1",
    capabilities: ["stt", "llm"],
    stt_api: "chat",
  },
  {
    id: "openai-tts",
    name: "OpenAI Voice",
    provider: "openai",
    model: "tts-1-hd",
    base_url: "https://api.openai.com",
    capabilities: ["tts"],
  },
  {
    id: "ollama-local",
    name: "Local Ollama",
    provider: "ollama",
    model: "llama3.2",
    base_url: "http://localhost:11434",
    capabilities: ["llm"],
  },
];

const config = {
  hotkey_dictation: "cmd+shift+space",
  hotkey_dictation_toggle: "cmd+shift+d",
  hotkey_tts: "cmd+shift+s",
  hotkey_agent: "cmd+shift+a",
  hotkey_agent_panel: "cmd+shift+g",
  hotkey_note: "option+n",
  hotkey_meeting: "cmd+shift+m",
  hotkey_note_1: "option+1",
  hotkey_note_2: "option+2",
  hotkey_note_3: "option+3",
  dictation_mode: "polish",
  default_voice: "alloy",
  tts_speed: 1,
  audio_input_device: "MacBook Pro Microphone",
  audio_output_device: "System Default",
  show_floating_indicator: true,
  warmup_enabled: true,
  ui_language: "auto",
  hotkey_listen: "option+l",
  listen_mode: "listen",
  listen_voice_profile: "",
  listen_voice: "default",
  hotkey_sts: "option+s",
  sts_persona: "You are a friendly voice assistant.",
  sts_llm_profile: "",
  sts_voice_profile: "",
  sts_voice: "default",
  sts_max_turns: 8,
  call_barge_in: true,
  stt_language: "auto",
  model_profiles: modelProfiles,
  stt_profile: "openai-gpt-4o-mini-transcribe",
  tts_profile: "openai-tts",
  llm_profile: "openrouter-gemini",
  agent_llm_profile: "ollama-local",
  agent_stt_profile: "openai-gpt-4o-mini-transcribe",
  agent_tts_enabled: false,
  agent_timeout_secs: 30,
  agent_max_turns: 6,
  agent_safety_allowlist: ["open", "osascript", "pbcopy"],
  agent_safety_blocklist: ["rm -rf", "curl | sh"],
  clean_prompt: "",
  translate_source: "auto",
  translate_target: "English",
  note_processor: "llm",
  note_stt_profile: "openai-gpt-4o-mini-transcribe",
  note_llm_profile: "openrouter-gemini",
  notebook_hotkey_1: 1,
  notebook_hotkey_2: 2,
  notebook_hotkey_3: 3,
  meeting_stt_profile: "openrouter-gemini",
  meeting_llm_profile: "openrouter-gemini",
  meeting_audio_source: "mic+system",
  // Demo mode is already "set up" — never trigger the first-run wizard.
  has_completed_onboarding: true,
  vocab_books: [
    {
      id: "coding",
      name: "Coding",
      enabled: true,
      terms: ["Kubernetes", "OpenRouter", "Qwen3-ASR", "ScreenCaptureKit"],
      rules: [
        { from: "Whisper Flow", to: "Wispr Flow", kind: "literal", case_insensitive: true },
        { from: "\\bOMLX\\b", to: "OMLX", kind: "regex", case_insensitive: false },
      ],
    },
    {
      id: "product-launch",
      name: "Product launch",
      enabled: true,
      terms: ["Product Hunt", "Show HN", "local-first", "voice terminal"],
      rules: [
        { from: "tap lines", to: "Taplines", kind: "literal", case_insensitive: true },
      ],
    },
  ],
  global_vocab_books: ["product-launch"],
  saved_scenarios: [
    // Models-only bundle — just the model profiles + role assignments.
    {
      id: "saved-local-omlx-demo",
      name: "Local · OMLX",
      created_at: String(Math.floor(now.getTime() / 1000) - 3600),
      models: {
        profiles: [
          { id: "scenario-omlx-qwen3-asr-1-7b", name: "Qwen3-ASR-1.7B", provider: "omlx", model: "Qwen3-ASR-1.7B", base_url: "http://localhost:8000", capabilities: ["stt"], stt_api: "whisper" },
          { id: "scenario-omlx-qwen3-8b-instruct", name: "Qwen3-8B-Instruct", provider: "omlx", model: "Qwen3-8B-Instruct", base_url: "http://localhost:8000", capabilities: ["llm"] },
          { id: "scenario-omlx-kokoro-82m", name: "Kokoro-82M", provider: "omlx", model: "Kokoro-82M", base_url: "http://localhost:8000", capabilities: ["tts"] },
        ],
        assignments: {
          stt_profile: "scenario-omlx-qwen3-asr-1-7b",
          llm_profile: "scenario-omlx-qwen3-8b-instruct",
          tts_profile: "scenario-omlx-kokoro-82m",
          sts_voice_profile: "scenario-omlx-kokoro-82m",
          listen_voice_profile: "scenario-omlx-kokoro-82m",
          sts_voice: "default",
          listen_voice: "default",
        },
      },
    },
    // Full bundle — models + dictation (custom modes) + speech.
    {
      id: "saved-cloud-openai-demo",
      name: "Fast cloud · OpenAI",
      created_at: String(Math.floor(now.getTime() / 1000) - 2 * 86400),
      models: {
        // Keys stripped (as an exported/shared bundle would be) → "needs key" chips.
        profiles: [
          { id: "scenario-openai-transcribe", name: "gpt-4o-mini-transcribe", provider: "openai", model: "gpt-4o-mini-transcribe", base_url: "https://api.openai.com", api_key: "", capabilities: ["stt"], stt_api: "whisper" },
          { id: "scenario-openai-gpt-4o-mini", name: "gpt-4o-mini", provider: "openai", model: "gpt-4o-mini", base_url: "https://api.openai.com", api_key: "", capabilities: ["llm"] },
          { id: "scenario-openai-tts", name: "gpt-4o-mini-tts", provider: "openai", model: "gpt-4o-mini-tts", base_url: "https://api.openai.com", api_key: "", capabilities: ["tts"] },
        ],
        assignments: {
          stt_profile: "scenario-openai-transcribe",
          llm_profile: "scenario-openai-gpt-4o-mini",
          tts_profile: "scenario-openai-tts",
          sts_voice_profile: "scenario-openai-tts",
          listen_voice_profile: "scenario-openai-tts",
          sts_voice: "alloy",
          listen_voice: "nova",
        },
      },
      dictation: {
        user_modes: {
          terminal: { name: "Terminal", description: "Convert speech into a shell-friendly command.", icon: "⌨️", temperature: 0.1 },
        },
        dictation_mode: "polish",
        translate_target: "English",
      },
      speech: {
        listen_mode: "listen",
        listen_voice_profile: "scenario-openai-tts",
        listen_voice: "nova",
        sts_persona: "You are a friendly voice assistant. Keep replies short and spoken.",
        sts_llm_profile: "scenario-openai-gpt-4o-mini",
        sts_voice_profile: "scenario-openai-tts",
        sts_voice: "alloy",
        sts_max_turns: 8,
      },
      vocab: {
        vocab_books: [
          { id: "vb-med", name: "Medical", enabled: true, terms: ["myocardium", "tachycardia"], rules: [] },
          { id: "vb-eng", name: "Engineering", enabled: true, terms: ["Kubernetes"], rules: [] },
        ],
        global_vocab_books: ["vb-med"],
      },
      hotkeys: {
        hotkey_dictation: "cmd+shift+space",
        hotkey_dictation_toggle: "cmd+shift+d",
        hotkey_tts: "cmd+shift+s",
        hotkey_agent: "cmd+shift+a",
        hotkey_agent_panel: "cmd+shift+g",
        hotkey_note: "option+n",
        hotkey_note_1: "option+1",
        hotkey_note_2: "",
        hotkey_note_3: "",
        notebook_hotkey_1: 0,
        notebook_hotkey_2: 0,
        notebook_hotkey_3: 0,
        hotkey_meeting: "option+m",
        hotkey_transform: "cmd+shift+t",
        hotkey_listen: "option+l",
        hotkey_sts: "option+s",
      },
    },
  ],
};

const modes = [
  {
    id: "raw",
    name: "Raw",
    description: "Verbatim transcription.",
    icon: "mic",
    system: null,
    user_template: null,
    temperature: 0,
    model: "",
    stt_model: "",
    stt_prompt: "",
    stt_temperature: 0,
    max_tokens: 4096,
    output_language: "auto",
    auto_paste: true,
    auto_press_enter: false,
    builtin: true,
  },
  {
    id: "polish",
    name: "Polish",
    description: "Turn rough speech into natural writing.",
    icon: "sparkles",
    system: "Rewrite the transcript into clear, natural writing.",
    user_template: "{text}",
    temperature: 0.2,
    model: "openrouter-gemini",
    stt_model: "",
    stt_prompt: "",
    stt_temperature: 0,
    max_tokens: 4096,
    output_language: "auto",
    auto_paste: true,
    auto_press_enter: false,
    builtin: true,
  },
  {
    id: "translate",
    name: "Translate",
    description: "Translate speech into English.",
    icon: "languages",
    system: "Translate the transcript while preserving intent.",
    user_template: "{text}",
    temperature: 0.1,
    model: "openrouter-gemini",
    stt_model: "",
    stt_prompt: "",
    stt_temperature: 0,
    max_tokens: 4096,
    output_language: "English",
    auto_paste: true,
    auto_press_enter: false,
    builtin: true,
  },
  {
    id: "note",
    name: "Note",
    description: "Save the result into a notebook.",
    icon: "notebook",
    system: "Convert the transcript into a concise note.",
    user_template: "{text}",
    temperature: 0.2,
    model: "openrouter-gemini",
    stt_model: "",
    stt_prompt: "",
    stt_temperature: 0,
    max_tokens: 4096,
    output_language: "auto",
    auto_paste: false,
    auto_press_enter: false,
    builtin: true,
  },
  {
    id: "terminal",
    name: "Terminal",
    description: "Convert speech into a shell-friendly command.",
    icon: "terminal",
    system: "Return a concise terminal command when possible.",
    user_template: "{text}",
    temperature: 0.1,
    model: "ollama-local",
    stt_model: "",
    stt_prompt: "",
    stt_temperature: 0,
    max_tokens: 1024,
    output_language: "auto",
    auto_paste: false,
    auto_press_enter: false,
    builtin: false,
  },
];

const containers = [
  {
    id: 1,
    container_type: "notebook",
    title: "Quick Note",
    parent_id: null,
    created_at: iso(1200),
    updated_at: iso(12),
    metadata: {},
  },
  {
    id: 2,
    container_type: "notebook",
    title: "Product Ideas",
    parent_id: null,
    created_at: iso(900),
    updated_at: iso(34),
    metadata: {},
  },
  {
    id: 3,
    container_type: "notebook",
    title: "Language Practice",
    parent_id: null,
    created_at: iso(760),
    updated_at: iso(80),
    metadata: {},
  },
  {
    id: 10,
    container_type: "meeting_session",
    title: "Fonos roadmap sync",
    parent_id: null,
    created_at: iso(145),
    updated_at: iso(95),
    metadata: {
      summary_generated: true,
      summary_preview:
        "Ship README demo assets, tighten hotkey onboarding, and validate OpenRouter audio models.",
    },
  },
];

const entries = [
  {
    id: 99,
    created_at: iso(5),
    source_type: "listen",
    role: "user",
    mode: "listen",
    raw_text: "Long article text captured from the browser about open-source voice AI…",
    processed_text:
      "Open-source voice AI is closing the gap with proprietary stacks. Modular pipelines let teams swap recognition, language and speech models independently.",
    container_id: null,
    audio_ref: "/tmp/demo-listen.wav",
    metadata: { title: "Open-source voice AI briefing" },
  },
  {
    id: 101,
    created_at: iso(12),
    source_type: "dictation",
    role: "user",
    mode: "polish",
    raw_text:
      "Make the readme explain that this is like Wispr Flow but for terminal and notes.",
    processed_text:
      "Fonos is a voice terminal inspired by Wispr Flow and Taplines: hold a hotkey, speak, and send polished text to your cursor, notes, meetings, or an AI agent.",
    container_id: null,
    audio_ref: null,
    metadata: {},
  },
  {
    id: 102,
    created_at: iso(34),
    source_type: "note",
    role: "user",
    mode: "note",
    raw_text: "Remember to test OpenRouter audio transcription with Gemini.",
    processed_text:
      "Test OpenRouter audio transcription with Gemini and compare latency against Whisper multipart upload.",
    container_id: 2,
    audio_ref: null,
    metadata: {},
  },
  {
    id: 103,
    created_at: iso(76),
    source_type: "note",
    role: "user",
    mode: "translate",
    raw_text: "Wo xiang yao yi ge geng ziran de yingwen banben.",
    processed_text:
      "I want a more natural English version that keeps the original tone.",
    container_id: 3,
    audio_ref: null,
    metadata: {},
  },
  {
    id: 104,
    created_at: iso(145),
    source_type: "meeting",
    role: "user",
    mode: "meeting",
    raw_text: "Let's focus the first release on dictation, notes, and model setup.",
    processed_text:
      "Let's focus the first release on dictation, notes, and model setup.",
    container_id: 10,
    audio_ref: null,
    metadata: {},
  },
  {
    id: 105,
    created_at: iso(142),
    source_type: "meeting",
    role: "assistant",
    mode: "meeting",
    raw_text: "We should also make the floating panel feel instant.",
    processed_text: "We should also make the floating panel feel instant.",
    container_id: 10,
    audio_ref: null,
    metadata: {},
  },
  {
    id: 106,
    created_at: iso(136),
    source_type: "meeting",
    role: "user",
    mode: "meeting",
    raw_text: "Add screenshots and a GIF so the README is easier to understand.",
    processed_text:
      "Add screenshots and a GIF so the README is easier to understand.",
    container_id: 10,
    audio_ref: null,
    metadata: {},
  },
];

const meetingSummary = {
  id: 110,
  created_at: iso(94),
  source_type: "meeting",
  role: "system",
  mode: "summary",
  raw_text: "",
  processed_text:
    "## Key Points\n- Fonos should feel like a fast voice terminal for writing, translation, and notes.\n- Default provider setup needs to be understandable at a glance.\n- The README should include a short demo GIF and focused screenshots.\n\n## Decisions\n- Prioritize Dictation, Notes, Meetings, and Models in the README.\n- Keep local-first privacy language visible.\n\n## Action Items\n- [x] Generate demo assets\n- [ ] Validate release packaging\n- [ ] Record a real microphone demo before launch",
  container_id: 10,
  audio_ref: null,
  metadata: {},
};

const dailyStats = ["06-11", "06-12", "06-13", "06-14", "06-15", "06-16", "06-17"].map(
  (day, index) => ({
    date: `2026-${day}`,
    stt_count: [5, 8, 4, 10, 12, 9, 14][index],
    stt_seconds: [120, 240, 95, 360, 420, 300, 510][index],
    stt_words: [620, 1040, 480, 1580, 2100, 1700, 2600][index],
    tts_count: [1, 2, 1, 4, 2, 3, 5][index],
    tts_words: [120, 180, 80, 420, 260, 330, 510][index],
    llm_count: [4, 5, 3, 8, 9, 7, 11][index],
    llm_latency_total: [1200, 1550, 900, 2600, 2800, 2100, 3400][index],
    tokens_total: [4200, 6200, 3100, 8600, 10400, 7700, 12200][index],
    time_saved_secs: [180, 260, 140, 420, 540, 460, 720][index],
  })
);

export function installDemoIpc() {
  mockWindows("main");
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;

    switch (cmd) {
      case "plugin:app|version":
      case "plugin:app|get_version":
        return "0.3.0";
      case "play_audio_file":
      case "stop_playback":
        return null;
      case "reset_sts_session":
      case "sts_page_start":
      case "call_start":
      case "call_stop":
        return null;
      case "sts_page_stop":
        return "This is a demo reply.";
      case "get_sts_history":
        return [["帮我总结一下今天的会议重点", "好的,今天会议有三个重点:发布计划确认到月底,预算需要再压缩百分之十,新同事下周入职。"]];
      case "create_listen_from_text":
        return 99;
      case "has_microphone":
        return true;
      case "check_accessibility":
        return true;
      case "open_settings_pane":
        return null;
      case "start_recording":
        return null;
      case "stop_recording":
        return {
          text:
            "Fonos should let me speak once and route the result to dictation, translation, notes, or a meeting transcript.",
          audio_path: "/tmp/fonos-demo.wav",
          latency_ms: 428,
          duration_secs: 3.6,
          stt_engine: "cloud",
          noise_removed_pct: 3.8,
          gain_db: 1.4,
        };
      case "process_with_llm":
        return {
          original: payload.text ?? "",
          processed:
            "Fonos lets you speak once, then route the result to dictation, translation, notes, meeting transcripts, or an AI agent without leaving the keyboard.",
          mode: payload.mode ?? "polish",
          mode_name: "Polish",
          latency_ms: 612,
          auto_paste: true,
          auto_press_enter: false,
        };
      case "get_config":
        return config;
      case "save_config":
      case "save_custom_mode":
      case "delete_custom_mode":
      case "update_entry":
      case "update_entry_text":
      case "delete_entry":
      case "delete_container":
      case "set_note_notebook":
        return null;
      case "list_modes":
        return modes;
      case "list_containers":
        return containers;
      case "list_entries": {
        const sourceType = payload.source_type;
        return entries
          .filter((entry) => !sourceType || entry.source_type === sourceType)
          .slice(0, (payload.limit as number | undefined) ?? 20);
      }
      case "search_entries": {
        const q = String(payload.query ?? "").toLowerCase();
        if (!q) return [];
        return entries
          .filter((entry) =>
            (entry.raw_text ?? "").toLowerCase().includes(q) ||
            (entry.processed_text ?? "").toLowerCase().includes(q))
          .slice(0, (payload.limit as number | undefined) ?? 50);
      }
      case "get_container_entries":
        return entries.filter((entry) => entry.container_id === payload.container_id);
      case "get_meetings":
        return containers.filter((container) => container.container_type === "meeting_session");
      case "get_meeting_detail":
        return {
          container: containers.find((container) => container.id === payload.container_id) ?? containers[3],
          entries: entries.filter((entry) => entry.container_id === payload.container_id),
          summary: meetingSummary,
        };
      case "export_notebook_md":
      case "export_notebook_json":
      case "export_meeting_md":
      case "export_meeting_json":
        return "/tmp/fonos-demo-export";
      case "get_dictation_latency":
        return {
          count: 42,
          p50_ms: 640,
          p95_ms: 1980,
          avg_ms: 780,
          min_ms: 310,
          max_ms: 2400,
          by_model: [
            { model: "Qwen3-ASR-1.7B-bf16", count: 31, p50_ms: 580, p95_ms: 1400 },
            { model: "gpt-4o-mini-transcribe", count: 11, p50_ms: 910, p95_ms: 2400 },
          ],
        };
      case "get_stats":
        return dailyStats;
      case "get_today":
        return {
          time_saved_secs: 720,
          total_words: 3110,
          total_sessions: 30,
          stt_count: 14,
          stt_words: 2600,
          stt_seconds: 510,
          tts_count: 5,
          tts_words: 510,
          llm_count: 11,
          llm_latency_avg: 309,
          tokens_total: 12200,
        };
      case "list_model_voices":
        return ["zf_xiaoxiao", "zm_yunjian", "af_heart"];
      case "list_voices":
        return {
          voices: [
            { voice_id: "alloy", name: "Alloy", status: "default" },
            { voice_id: "demo-voice", name: "Demo Voice", status: "cloned" },
          ],
        };
      case "list_audio_inputs":
        return ["MacBook Pro Microphone", "Studio Display Microphone"];
      case "list_provider_models":
        return [
          { id: "gpt-4o-mini-transcribe", owned_by: "openai" },
          { id: "gpt-4o-mini-tts", owned_by: "openai" },
          { id: "google/gemini-2.5-flash", owned_by: "google" },
        ];
      case "test_stt":
        return "OK — demo endpoint responded";
      case "run_doctor":
        return [
          { id: "endpoint_ok:localhost:8000", severity: "pass", message_key: "doctor.endpoint_ok", message_params: ["STT · LLM · TTS", "localhost:8000 · 47ms"], fix: null },
          { id: "permissions_ok", severity: "pass", message_key: "doctor.permissions_ok", message_params: [], fix: null },
          { id: "hotkeys_ok", severity: "pass", message_key: "doctor.hotkeys_ok", message_params: [], fix: null },
          { id: "vocab_unattached:coding", severity: "warn", message_key: "doctor.vocab_unattached", message_params: ["Coding"], fix: { kind: "attach_book_global", book_id: "coding" } },
          { id: "rtf_slow", severity: "advise", message_key: "doctor.rtf_slow", message_params: ["2.3"], fix: { kind: "switch_tts_model", profile_id: "openai-tts", model: "kokoro-82m" } },
          { id: "refs_ok", severity: "pass", message_key: "doctor.refs_ok", message_params: [], fix: null },
        ];
      case "apply_doctor_fix":
        return null;
      case "scan_models": {
        // Only the OMLX/vLLM default port (8000) "answers" in the demo.
        const url = String(payload.baseUrl ?? "");
        const reachable = url.includes(":8000");
        return {
          reachable,
          latency_ms: reachable ? 47 : 0,
          models: reachable
            ? ["Qwen3-ASR-1.7B", "Qwen3-4B-Instruct-2507", "Qwen3-8B-Instruct", "Kokoro-82M", "Qwen3-TTS-1.7B", "bge-m3-embed", "DeepFilterNet3-SE"]
            : [],
        };
      }
      case "scenario_probe": {
        const models = ["Qwen3-ASR-1.7B", "Qwen3-4B-Instruct-2507", "Qwen3-8B-Instruct", "Kokoro-82M", "Qwen3-TTS-1.7B", "bge-m3-embed", "DeepFilterNet3-SE"];
        return {
          reachable: true,
          latency_ms: 47,
          models,
          classified: {
            stt: ["Qwen3-ASR-1.7B"],
            llm: ["Qwen3-4B-Instruct-2507", "Qwen3-8B-Instruct"],
            tts: ["Kokoro-82M", "Qwen3-TTS-1.7B"],
          },
          tts_rtfs: { "Kokoro-82M": 0.5, "Qwen3-TTS-1.7B": 1.8 },
          plan: {
            stt: "Qwen3-ASR-1.7B",
            llm: "Qwen3-8B-Instruct",
            conversation_tts: "Kokoro-82M",
            listen_tts: "Qwen3-TTS-1.7B",
          },
        };
      }
      case "save_scenario":
        return {
          id: "saved-new-" + Date.now(),
          name: String(payload.name ?? "Setup"),
          created_at: String(Math.floor(Date.now() / 1000)),
          ...(payload.includeModels !== false
            ? {
                models: {
                  profiles: [],
                  assignments: {
                    stt_profile: "openai-gpt-4o-mini-transcribe",
                    llm_profile: "openrouter-gemini",
                    tts_profile: "openai-tts",
                    sts_voice_profile: "openai-tts",
                    listen_voice_profile: "openai-tts",
                    sts_voice: "default",
                    listen_voice: "default",
                  },
                },
              }
            : {}),
          ...(payload.includeDictation
            ? { dictation: { user_modes: {}, dictation_mode: "polish", translate_target: "English" } }
            : {}),
          ...(payload.includeSpeech
            ? {
                speech: {
                  listen_mode: "listen", listen_voice_profile: "", listen_voice: "default",
                  sts_persona: "You are a friendly voice assistant.", sts_llm_profile: "",
                  sts_voice_profile: "", sts_voice: "default", sts_max_turns: 8,
                },
              }
            : {}),
          ...(payload.includeVocab
            ? { vocab: { vocab_books: [], global_vocab_books: [] } }
            : {}),
          ...(payload.includeHotkeys
            ? {
                hotkeys: {
                  hotkey_dictation: "cmd+shift+space", hotkey_dictation_toggle: "", hotkey_tts: "cmd+shift+s",
                  hotkey_agent: "cmd+shift+a", hotkey_agent_panel: "cmd+shift+g", hotkey_note: "option+n",
                  hotkey_note_1: "", hotkey_note_2: "", hotkey_note_3: "",
                  notebook_hotkey_1: 0, notebook_hotkey_2: 0, notebook_hotkey_3: 0,
                  hotkey_meeting: "option+m", hotkey_transform: "cmd+shift+t",
                  hotkey_listen: "option+l", hotkey_sts: "option+s",
                },
              }
            : {}),
        };
      case "apply_saved_scenario":
      case "delete_saved_scenario":
        return null;
      case "export_scenario":
        return "/Users/demo/Downloads/fonos-scenario-demo.json";
      case "import_scenario":
      case "import_scenario_json":
        return {
          id: "saved-imported-" + Date.now(),
          name: "Imported setup",
          created_at: String(Math.floor(Date.now() / 1000)),
          models: {
            profiles: [],
            assignments: {
              stt_profile: "", llm_profile: "", tts_profile: "",
              sts_voice_profile: "", listen_voice_profile: "", sts_voice: "default", listen_voice: "default",
            },
          },
        };
      default:
        return null;
    }
  }, { shouldMockEvents: true });
}
