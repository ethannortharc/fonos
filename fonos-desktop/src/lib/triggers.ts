import type { WorkflowDef, WorkflowRow } from "../types";

/** Pill-roller slot order, or null when the workflow has no pill_slot chip. */
export function pillOrder(wf: WorkflowDef): number | null {
  const t = (wf.triggers ?? []).find((x) => x.kind === "pill_slot");
  return t ? (t.kind === "pill_slot" ? (t.order ?? 0) : 0) : null;
}

/** Workflows that appear in the float pill's roller, in slot order. */
export function pillWorkflows(rows: WorkflowRow[]): WorkflowRow[] {
  return rows
    .filter((w) => pillOrder(w) !== null)
    .sort((a, b) => (pillOrder(a) ?? 0) - (pillOrder(b) ?? 0));
}

/** How many workflows reference this widget (each workflow counted once). */
export function usageCount(widgetId: string, rows: WorkflowRow[]): number {
  return rows.filter(
    (w) =>
      w.source === widgetId ||
      (w.processors ?? []).includes(widgetId) ||
      w.outputs.includes(widgetId),
  ).length;
}
