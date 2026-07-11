import { describe, expect, it } from "vitest";
import { pillOrder, pillWorkflows, usageCount } from "../triggers";
import { GROUPS, TYPE_TAGS } from "../../views/workbench/typeMeta";
import type { WidgetDef, WorkflowRow } from "../../types";

const row = (id: string, over: Partial<WorkflowRow> = {}): WorkflowRow => ({
  id, name: id, source: "src.mic-hold", outputs: ["out.insert"],
  source_type_tag: "microphone", ...over,
});

describe("triggers helpers", () => {
  it("pillWorkflows filters to pill_slot chips and sorts by order", () => {
    const rows = [
      row("b", { triggers: [{ kind: "pill_slot", order: 20 }] }),
      row("none", { triggers: [{ kind: "hotkey", combo: "cmd+shift+e" }] }),
      row("a", { triggers: [{ kind: "pill_slot", order: 0 }] }),
    ];
    expect(pillWorkflows(rows).map((w) => w.id)).toEqual(["a", "b"]);
  });
  it("pillOrder defaults missing order to 0", () => {
    expect(pillOrder(row("x", { triggers: [{ kind: "pill_slot" }] }))).toBe(0);
    expect(pillOrder(row("y"))).toBeNull();
  });
  it("usageCount counts source/processor/output references", () => {
    const rows = [
      row("r1", { source: "w.a", processors: ["w.b"], outputs: ["w.c"] }),
      row("r2", { source: "w.x", processors: ["w.b", "w.b2"], outputs: ["w.b"] }),
    ];
    expect(usageCount("w.b", rows)).toBe(2); // 每配方计一次
    expect(usageCount("w.a", rows)).toBe(1);
    expect(usageCount("w.zzz", rows)).toBe(0);
  });

  const widget = (id: string, type_tag: string, props: Record<string, unknown> = {}): WidgetDef => ({
    id, role: "processor", type_tag, name: id, icon: "", props, builtin: false,
  });

  it("usageCount pierces composite ref props (widget referencing widget)", () => {
    const rows: WorkflowRow[] = [];
    const widgets: WidgetDef[] = [
      widget("call.custom", "call", { stt_widget: "stt.target", llm_widget: "" }),
      widget("stt.target", "stt"),
      widget("stt.other", "stt", { some_field: "stt.target" }), // "stt" has no ref props — not a match
    ];
    expect(usageCount("stt.target", rows, widgets)).toBe(1);
    expect(usageCount("stt.other", rows, widgets)).toBe(0);
  });

  it("usageCount counts a composite with two ref props at the same target once", () => {
    const widgets: WidgetDef[] = [
      widget("call.custom", "call", { stt_widget: "shared.widget", llm_widget: "shared.widget" }),
    ];
    expect(usageCount("shared.widget", [], widgets)).toBe(1);
  });

  it("usageCount adds workflow references and widget references together", () => {
    const rows: WorkflowRow[] = [row("r1", { source: "w.a", outputs: ["out.insert"] })];
    const widgets: WidgetDef[] = [widget("call.custom", "call", { llm_widget: "w.a" })];
    expect(usageCount("w.a", rows, widgets)).toBe(2);
  });
});

// ── typeMeta two-sources-of-truth guard (Workbench P2 Task 6 rider) ─────────
// GROUPS (the four-shelf presentation table) and TYPE_TAGS (the role-keyed
// semantic table pickers/validation actually consume) are hand-maintained
// separately — see typeMeta.ts's own comments on why. This guards that the
// two never silently drift apart: every tag GROUPS renders must be one
// TYPE_TAGS says some role can instantiate, and vice versa.
describe("typeMeta two-sources-of-truth guard", () => {
  it("GROUPS' tag union exactly matches TYPE_TAGS' tag union", () => {
    const groupsTags = [...new Set(GROUPS.flatMap((g) => g.tags))].sort();
    const typeTagsTags = [...new Set(Object.values(TYPE_TAGS).flat())].sort();
    expect(groupsTags).toEqual(typeTagsTags);
  });
});
