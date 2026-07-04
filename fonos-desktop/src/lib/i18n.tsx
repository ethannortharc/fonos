// Lightweight i18n (issue #32): typed dictionaries, a module-level locale
// store, and a `useT()` hook. No dependencies. English is the fallback for
// any key missing from another locale.

import { useSyncExternalStore } from "react";

export type Locale = "en" | "zh";
export type UiLanguage = "auto" | Locale;

/** Resolve the configured language ("auto" follows the system). */
export function resolveLocale(setting: UiLanguage | undefined): Locale {
  if (setting === "en" || setting === "zh") return setting;
  return navigator.language?.toLowerCase().startsWith("zh") ? "zh" : "en";
}

// ── locale store ──────────────────────────────────────────────────────────
let current: Locale = "en";
const listeners = new Set<() => void>();

export function setLocale(l: Locale) {
  if (l === current) return;
  current = l;
  listeners.forEach((fn) => fn());
}
export function getLocale(): Locale {
  return current;
}
function subscribe(fn: () => void) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

// ── dictionaries ──────────────────────────────────────────────────────────
// Keys are dot-scoped by surface. `en` is the source of truth; `zh` overlays.
const en = {
  // navigation
  "nav.dictation": "Dictation",
  "nav.history": "History",
  "nav.notes": "Notes",
  "nav.meetings": "Meetings",
  "nav.talk": "Talk",
  "nav.stats": "Stats",
  "nav.settings": "Settings",

  // settings tabs
  "tab.general": "General",
  "tab.models": "Models",
  "tab.dictation": "Dictation",
  "tab.speech": "Speech",
  "tab.vocab": "Vocabulary",
  "tab.agent": "Agent",
  "tab.notes": "Notes",
  "tab.meeting": "Meeting",
  "tab.hotkeys": "Hotkeys",

  // common
  "common.save": "Save",
  "common.cancel": "Cancel",
  "common.delete": "Delete",
  "common.copy": "Copy",
  "common.play": "▶ Play",
  "common.stop": "■ Stop",
  "common.preview": "▶ Preview",
  "common.playing": "Playing…",
  "common.enabled": "Enabled",
  "common.disabled": "Disabled",
  "common.global": "Global",
  "common.default": "default",
  "common.custom": "Custom…",
  "common.list": "List",
  "common.recheck": "Re-check",
  "common.change": "Change ▾",

  // general tab
  "general.language": "Interface language",
  "general.language.auto": "Auto (system)",
  "general.language.en": "English",
  "general.language.zh": "中文",

  // speech tab
  "speech.listen.title": "Listen queue",
  "speech.listen.desc":
    "Select text anywhere, press the hotkey — summarized, synthesized, playable from History › Listen.",
  "speech.listen.hotkey": "Capture hotkey",
  "speech.processing": "Processing",
  "speech.processing.hint": "how text is rewritten",
  "speech.voicemodel": "Voice model",
  "speech.voicemodel.hint": "empty = default TTS",
  "speech.voice": "Voice",
  "speech.conv.title": "Conversation",
  "speech.conv.desc":
    "Hold to talk (in the Talk page or via the global hotkey) — recognized, answered by the persona, spoken back.",
  "speech.conv.hotkey": "Hold-to-talk",
  "speech.persona": "Persona",
  "speech.persona.hint": "replies are spoken — keep them short",
  "speech.llm": "LLM",
  "speech.llm.hint": "empty = default",
  "speech.memory": "Memory",
  "speech.memory.hint": "turn pairs kept",
  "speech.voices.cloned": "Your cloned voices",
  "speech.voices.model": "Model speakers",
  "speech.voices.placeholder": "speaker name…",

  // conversation page
  "conv.title": "Conversation",
  "conv.state.idle": "Ready",
  "conv.state.listening": "Listening…",
  "conv.state.thinking": "Thinking…",
  "conv.state.speaking": "Speaking…",
  "conv.persona": "Persona",
  "conv.newchat": "New chat",
  "conv.persona.desc": "System prompt for this conversation — replies are spoken aloud, keep them short.",
  "conv.persona.applies": "Applies from the next turn",
  "conv.persona.unsaved": " · unsaved",
  "conv.persona.savedefault": "Save as default",
  "conv.empty.title": "Hold the button and talk",
  "conv.empty.hint": "works from anywhere · memory lasts",
  "conv.empty.turns": "turns",
  "conv.hold": "Hold to talk",
  "conv.release": "Release to send",
  "conv.speaking": "speaking",

  // history
  "history.search": "Search everything…",
  "history.filter.all": "All",
  "history.filter.dictation": "Dictation",
  "history.filter.notes": "Notes",
  "history.filter.meetings": "Meetings",
  "history.filter.listen": "Listen",
  "history.filter.agent": "Agent",

  // vocabulary tab
  "vocab.title": "Vocabulary books",
  "vocab.desc":
    "Terms bias speech recognition and guide LLM output; rules are deterministic find → replace corrections applied to every transcript. Mark a book Global to apply it everywhere — or mount it on specific modes from each mode's card in the Dictation tab.",
  "vocab.terms": "Terms",
  "vocab.terms.hint": "— correct spellings the recognizer should prefer",
  "vocab.terms.placeholder": "Type a term, press Enter — e.g. Kubernetes, OMLX…",
  "vocab.rules": "Correction rules",
  "vocab.rules.hint": "— deterministic fixes, e.g. 衣袖 → issue",
  "vocab.rules.empty": "When the recognizer keeps mishearing a word, pin the fix here.",
  "vocab.rule.from": "heard as…",
  "vocab.rule.pattern": "pattern",
  "vocab.rule.to": "should be…",
  "vocab.addrule": "+ Add rule",
  "vocab.addbook": "+ Add vocabulary book",
  "vocab.bookname": "Book name",
  "vocab.termcount": "terms",
  "vocab.rulecount": "rules",
  "vocab.empty.title": "No vocabulary books yet",
  "vocab.empty.desc":
    "Add one for your domain terms — names, jargon, product words — and dictation will start getting them right.",
};

export type TKey = keyof typeof en;
type Key = TKey;

const zh: Partial<Record<Key, string>> = {
  "nav.dictation": "听写",
  "nav.history": "历史",
  "nav.notes": "笔记",
  "nav.meetings": "会议",
  "nav.talk": "对话",
  "nav.stats": "统计",
  "nav.settings": "设置",

  "tab.general": "通用",
  "tab.models": "模型",
  "tab.dictation": "听写",
  "tab.speech": "语音",
  "tab.vocab": "词汇",
  "tab.agent": "助手",
  "tab.notes": "笔记",
  "tab.meeting": "会议",
  "tab.hotkeys": "快捷键",

  "common.save": "保存",
  "common.cancel": "取消",
  "common.delete": "删除",
  "common.copy": "复制",
  "common.play": "▶ 播放",
  "common.stop": "■ 停止",
  "common.preview": "▶ 试听",
  "common.playing": "播放中…",
  "common.enabled": "已启用",
  "common.disabled": "已停用",
  "common.global": "全局",
  "common.default": "默认",
  "common.custom": "自定义…",
  "common.list": "列表",
  "common.recheck": "重新检查",
  "common.change": "更换 ▾",

  "general.language": "界面语言",
  "general.language.auto": "自动(跟随系统)",
  "general.language.en": "English",
  "general.language.zh": "中文",

  "speech.listen.title": "Listen 队列",
  "speech.listen.desc": "在任意应用选中文字、按下快捷键 — 自动摘要并合成语音,在 历史 › Listen 中播放。",
  "speech.listen.hotkey": "捕获快捷键",
  "speech.processing": "处理方式",
  "speech.processing.hint": "文字如何改写",
  "speech.voicemodel": "语音模型",
  "speech.voicemodel.hint": "留空 = 默认 TTS",
  "speech.voice": "音色",
  "speech.conv.title": "实时对话",
  "speech.conv.desc": "按住说话(对话页或全局快捷键)— 识别、由人设回答、语音播报。",
  "speech.conv.hotkey": "按住说话",
  "speech.persona": "人设",
  "speech.persona.hint": "回复会被朗读 — 保持简短",
  "speech.llm": "语言模型",
  "speech.llm.hint": "留空 = 默认",
  "speech.memory": "记忆",
  "speech.memory.hint": "保留的对话轮数",
  "speech.voices.cloned": "你的克隆音色",
  "speech.voices.model": "模型内置音色",
  "speech.voices.placeholder": "说话人名称…",

  "conv.title": "实时对话",
  "conv.state.idle": "就绪",
  "conv.state.listening": "聆听中…",
  "conv.state.thinking": "思考中…",
  "conv.state.speaking": "播报中…",
  "conv.persona": "人设",
  "conv.newchat": "新对话",
  "conv.persona.desc": "本次对话的系统提示词 — 回复会被朗读,保持简短。",
  "conv.persona.applies": "下一轮生效",
  "conv.persona.unsaved": " · 未保存",
  "conv.persona.savedefault": "存为默认",
  "conv.empty.title": "按住按钮开始说话",
  "conv.empty.hint": "全局可用 · 记忆保留",
  "conv.empty.turns": "轮",
  "conv.hold": "按住说话",
  "conv.release": "松开发送",
  "conv.speaking": "播报中",

  "history.search": "搜索全部内容…",
  "history.filter.all": "全部",
  "history.filter.dictation": "听写",
  "history.filter.notes": "笔记",
  "history.filter.meetings": "会议",
  "history.filter.listen": "Listen",
  "history.filter.agent": "助手",

  "vocab.title": "词汇本",
  "vocab.desc":
    "词条用于语音识别偏置和 LLM 术语对齐;规则是应用于每次转写的确定性替换。标记为「全局」处处生效,或在听写页的模式卡片上挂载到特定模式。",
  "vocab.terms": "词条",
  "vocab.terms.hint": "— 识别时应优先选用的正确写法",
  "vocab.terms.placeholder": "输入词条,回车添加 — 如 Kubernetes、OMLX…",
  "vocab.rules": "纠正规则",
  "vocab.rules.hint": "— 确定性替换,如 衣袖 → issue",
  "vocab.rules.empty": "识别总是听错某个词时,在这里固定纠正。",
  "vocab.rule.from": "被听成…",
  "vocab.rule.pattern": "正则模式",
  "vocab.rule.to": "应该是…",
  "vocab.addrule": "+ 添加规则",
  "vocab.addbook": "+ 新建词汇本",
  "vocab.bookname": "词汇本名称",
  "vocab.termcount": "词条",
  "vocab.rulecount": "规则",
  "vocab.empty.title": "还没有词汇本",
  "vocab.empty.desc": "为你的领域词汇建一本 — 人名、行话、产品名 — 听写从此不再写错。",
};

const dicts: Record<Locale, Partial<Record<Key, string>>> = { en, zh };

/** Translate a key in the active locale (falls back to English). */
export function t(key: Key): string {
  return dicts[current][key] ?? en[key];
}

/** Reactive translator: re-renders the component when the locale changes. */
export function useT() {
  useSyncExternalStore(subscribe, getLocale);
  return t;
}
