// Settings view constants and form types.

export type SettingsTab = "general" | "models" | "dictation" | "agent" | "notes" | "meeting" | "hotkeys";

export const TABS: { key: SettingsTab; label: string }[] = [
  { key: "general", label: "General" },
  { key: "models", label: "Models" },
  { key: "dictation", label: "Dictation" },
  { key: "agent", label: "Agent" },
  { key: "notes", label: "Notes" },
  { key: "meeting", label: "Meeting" },
  { key: "hotkeys", label: "Hotkeys" },
];

export const PROVIDERS = [
  { id: "openai", label: "OpenAI", url: "https://api.openai.com" },
  { id: "openrouter", label: "OpenRouter", url: "https://openrouter.ai/api/v1" },
  { id: "anthropic", label: "Anthropic", url: "https://api.anthropic.com" },
  { id: "google", label: "Google", url: "https://generativelanguage.googleapis.com" },
  { id: "ollama", label: "Ollama", url: "http://localhost:11434" },
  { id: "lmstudio", label: "LM Studio", url: "http://localhost:1234" },
  { id: "omlx", label: "OMLX", url: "http://localhost:8000" },
  { id: "custom", label: "Custom", url: "" },
];

// Supported languages for STT and translation
export const LANGUAGES = [
  { code: "auto", label: "Auto Detect", flag: "\u{1F310}" },
  { code: "Chinese", label: "Chinese \u4E2D\u6587", flag: "\u{1F1E8}\u{1F1F3}" },
  { code: "English", label: "English", flag: "\u{1F1FA}\u{1F1F8}" },
  { code: "Japanese", label: "Japanese \u65E5\u672C\u8A9E", flag: "\u{1F1EF}\u{1F1F5}" },
  { code: "Korean", label: "Korean \uD55C\uAD6D\uC5B4", flag: "\u{1F1F0}\u{1F1F7}" },
  { code: "Cantonese", label: "Cantonese \u7CA4\u8BED", flag: "\u{1F1ED}\u{1F1F0}" },
  { code: "French", label: "French Fran\u00E7ais", flag: "\u{1F1EB}\u{1F1F7}" },
  { code: "German", label: "German Deutsch", flag: "\u{1F1E9}\u{1F1EA}" },
  { code: "Spanish", label: "Spanish Espa\u00F1ol", flag: "\u{1F1EA}\u{1F1F8}" },
  { code: "Portuguese", label: "Portuguese Portugu\u00EAs", flag: "\u{1F1F5}\u{1F1F9}" },
  { code: "Italian", label: "Italian Italiano", flag: "\u{1F1EE}\u{1F1F9}" },
  { code: "Russian", label: "Russian \u0420\u0443\u0441\u0441\u043A\u0438\u0439", flag: "\u{1F1F7}\u{1F1FA}" },
  { code: "Arabic", label: "Arabic \u0627\u0644\u0639\u0631\u0628\u064A\u0629", flag: "\u{1F1F8}\u{1F1E6}" },
  { code: "Hindi", label: "Hindi \u0939\u093F\u0928\u094D\u0926\u0940", flag: "\u{1F1EE}\u{1F1F3}" },
  { code: "Thai", label: "Thai \u0E44\u0E17\u0E22", flag: "\u{1F1F9}\u{1F1ED}" },
  { code: "Vietnamese", label: "Vietnamese Ti\u1EBFng Vi\u1EC7t", flag: "\u{1F1FB}\u{1F1F3}" },
  { code: "Dutch", label: "Dutch Nederlands", flag: "\u{1F1F3}\u{1F1F1}" },
  { code: "Polish", label: "Polish Polski", flag: "\u{1F1F5}\u{1F1F1}" },
  { code: "Turkish", label: "Turkish T\u00FCrk\u00E7e", flag: "\u{1F1F9}\u{1F1F7}" },
  { code: "Indonesian", label: "Indonesian Bahasa", flag: "\u{1F1EE}\u{1F1E9}" },
];

// Languages for translate target (no "auto" option)
export const TARGET_LANGUAGES = LANGUAGES.filter((l) => l.code !== "auto");

export const CAP_BADGE: Record<string, string> = {
  stt: "bg-[rgba(245,158,11,0.1)] text-[rgba(251,191,36,0.7)]",
  tts: "bg-[rgba(196,181,253,0.08)] text-[rgba(196,181,253,0.6)]",
  llm: "bg-[rgba(134,239,172,0.08)] text-[rgba(134,239,172,0.6)]",
};

export interface ModeForm {
  id: string;
  name: string;
  description: string;
  icon: string;
  system: string;
  user_template: string;
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

export const EMPTY_MODE: ModeForm = {
  id: "",
  name: "",
  description: "",
  icon: "\u2728",
  system: "",
  user_template: "{text}",
  temperature: 0.3,
  model: "",
  stt_model: "",
  stt_prompt: "",
  stt_temperature: 0,
  max_tokens: 4096,
  output_language: "auto",
  auto_paste: true,
  auto_press_enter: false,
};

export interface ModelForm {
  id: string;
  name: string;
  provider: string;
  model: string;
  api_key: string;
  base_url: string;
  capabilities: string[];
  stt_api: "whisper" | "chat";
}

export const EMPTY_MODEL: ModelForm = {
  id: "",
  name: "",
  provider: "",
  model: "",
  api_key: "",
  base_url: "",
  capabilities: [],
  stt_api: "whisper",
};
