import { t, type TKey } from "./i18n";

// Built-in widget/workflow id → i18n key. Falls back to the backend name when
// missing. Ids verified against fonos-core/src/workflow/builtin.rs
// (built_in_widgets/built_in_workflows); wf.dictation-toggle is not defined
// there but is a real id created by fonos-core/src/workflow/migrate.rs during
// legacy config migration (see task-1-report.md for detail).
export const BUILTIN_LABELS: Record<string, TKey> = {
  // workflows
  "wf.dictation": "builtin.wf.dictation",
  "wf.dictation-toggle": "builtin.wf.dictation-toggle",
  "wf.translate-pop": "builtin.wf.translate-pop",
  "wf.summarize-pop": "builtin.wf.summarize-pop",
  "wf.listen": "builtin.wf.listen",
  "wf.note": "builtin.wf.note",
  // source widgets
  "src.selection": "builtin.src.selection",
  "src.mic-hold": "builtin.src.mic-hold",
  "src.mic-toggle": "builtin.src.mic-toggle",
  // processor widgets
  "stt.default": "builtin.stt.default",
  "llm.polish": "builtin.llm.polish",
  "llm.formal": "builtin.llm.formal",
  "llm.translate": "builtin.llm.translate",
  "llm.summarize": "builtin.llm.summarize",
  "llm.listen": "builtin.llm.listen",
  // output widgets
  "out.insert": "builtin.out.insert",
  "out.replace": "builtin.out.replace",
  "out.clipboard": "builtin.out.clipboard",
  "out.panel": "builtin.out.panel",
  "out.speak": "builtin.out.speak",
  "out.quicknote": "builtin.out.quicknote",
};

export function widgetLabel(w: { id: string; name: string; builtin?: boolean }): string {
  const k = w.builtin ? BUILTIN_LABELS[w.id] : undefined;
  return k ? t(k) : w.name;
}
export function workflowLabel(w: { id: string; name: string; builtin?: boolean }): string {
  const k = w.builtin ? BUILTIN_LABELS[w.id] : undefined;
  return k ? t(k) : w.name;
}
