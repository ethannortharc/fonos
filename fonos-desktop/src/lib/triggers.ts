import type { WidgetDef, WorkflowDef, WorkflowRow } from "../types";
import { WIDGET_REF_PROPS } from "../views/workbench/typeMeta";

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

/** How many workflows or widgets reference this widget (each referrer
 *  counted once — a composite widget with two ref props pointing at the
 *  same target, e.g. a "call" widget's stt_widget AND llm_widget both set to
 *  the same id, still counts as a single referrer). `widgets` defaults to
 *  `[]` for callers that only care about workflow references (and for
 *  existing tests) — pass the loaded widget list to also pierce into
 *  composite ref props (WIDGET_REF_PROPS), so a capability widget embedded
 *  only inside a composite isn't reported as unused. */
export function usageCount(widgetId: string, rows: WorkflowRow[], widgets: WidgetDef[] = []): number {
  const workflowCount = rows.filter(
    (w) =>
      w.source === widgetId ||
      (w.processors ?? []).includes(widgetId) ||
      w.outputs.includes(widgetId),
  ).length;

  const widgetCount = widgets.filter((w) => {
    if (w.id === widgetId) return false;
    const refProps = WIDGET_REF_PROPS[w.type_tag] ?? [];
    const props = (w.props ?? {}) as Record<string, unknown>;
    return refProps.some((prop) => props[prop] === widgetId);
  }).length;

  return workflowCount + widgetCount;
}
