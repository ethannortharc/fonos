import { describe, expect, it } from "vitest";
import { pillOrder, pillWorkflows, usageCount } from "../triggers";
import type { WorkflowRow } from "../../types";

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
});
