import { resolveNotebookInput } from "../notebookSelection";
import type { Container } from "../storage-api";

const nb = (id: number, title: string): Container => ({
  id,
  container_type: "notebook",
  title,
  parent_id: null,
  created_at: "2026-07-13T00:00:00Z",
  updated_at: "2026-07-13T00:00:00Z",
  metadata: {},
});

describe("resolveNotebookInput", () => {
  const books = [nb(3, "Quick Note"), nb(7, "摘抄")];

  it("empty input means the Quick Note sentinel", () => {
    expect(resolveNotebookInput("", books)).toEqual({ kind: "existing", container_id: 0 });
    expect(resolveNotebookInput("   ", books)).toEqual({ kind: "existing", container_id: 0 });
  });

  it("trimmed exact title match binds the existing notebook", () => {
    expect(resolveNotebookInput(" 摘抄 ", books)).toEqual({ kind: "existing", container_id: 7 });
  });

  it("does not case-fold titles", () => {
    expect(resolveNotebookInput("quick note", books)).toEqual({ kind: "create", title: "quick note" });
  });

  it("unknown text is a pending create", () => {
    expect(resolveNotebookInput("灵感", books)).toEqual({ kind: "create", title: "灵感" });
  });

  it("ignores non-notebook containers", () => {
    const mixed = [...books, { ...nb(9, "会议"), container_type: "meeting_session" as const }];
    expect(resolveNotebookInput("会议", mixed)).toEqual({ kind: "create", title: "会议" });
  });
});
