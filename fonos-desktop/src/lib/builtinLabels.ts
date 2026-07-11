import { t, type TKey } from "./i18n";

// Built-in widget/workflow id → i18n key. Falls back to the backend name when
// missing. This map is the authority on which ids get bilingual labels: it
// holds only stable built-in / migration-namespaced ids (wf.*/llm.*/src.*/
// out.*/stt.*). User-created ids always carry a -custom-{timestamp} suffix
// and never collide with these, so widgetLabel/workflowLabel key off map
// membership rather than the `builtin` flag — the flag is false for
// migration-generated ids like wf.dictation-toggle (created by
// fonos-core/src/workflow/migrate.rs, not builtin.rs), so gating on it would
// miss them.
export const BUILTIN_LABELS: Record<string, TKey> = {
  // workflows
  "wf.dictation": "builtin.wf.dictation",
  "wf.dictation-toggle": "builtin.wf.dictation-toggle",
  "wf.translate-pop": "builtin.wf.translate-pop",
  "wf.summarize-pop": "builtin.wf.summarize-pop",
  "wf.explain": "builtin.wf.explain",
  "wf.listen": "builtin.wf.listen",
  "wf.note": "builtin.wf.note",
  "wf.agent": "builtin.wf.agent",
  "wf.agent-voice": "builtin.wf.agent-voice",
  "wf.meeting": "builtin.wf.meeting",
  "wf.call": "builtin.wf.call",
  // source widgets
  "src.selection": "builtin.src.selection",
  "src.mic-hold": "builtin.src.mic-hold",
  "src.mic-toggle": "builtin.src.mic-toggle",
  "src.instant": "builtin.src.instant",
  // processor widgets
  "stt.default": "builtin.stt.default",
  "llm.polish": "builtin.llm.polish",
  "llm.formal": "builtin.llm.formal",
  "llm.translate": "builtin.llm.translate",
  "llm.summarize": "builtin.llm.summarize",
  "llm.listen": "builtin.llm.listen",
  "llm.explain": "builtin.llm.explain",
  // output widgets
  "out.insert": "builtin.out.insert",
  "out.replace": "builtin.out.replace",
  "out.clipboard": "builtin.out.clipboard",
  "out.panel": "builtin.out.panel",
  "out.dialog": "builtin.out.dialog",
  "out.speak": "builtin.out.speak",
  "out.quicknote": "builtin.out.quicknote",
  "agent.default": "builtin.agent.default",
  "meeting.default": "builtin.meeting.default",
  "call.default": "builtin.call.default",
};

export function widgetLabel(w: { id: string; name: string; builtin?: boolean }): string {
  const k = BUILTIN_LABELS[w.id];
  return k ? t(k) : w.name;
}
export function workflowLabel(w: { id: string; name: string; builtin?: boolean }): string {
  const k = BUILTIN_LABELS[w.id];
  return k ? t(k) : w.name;
}
