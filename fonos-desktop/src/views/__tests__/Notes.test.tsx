import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import Notes from "../Notes";
import { deleteContainer } from "../../lib/storage-api";

vi.mock("../../lib/i18n", () => ({
  t: (k: string) => k,
  useT: () => 0,
}));
vi.mock("../../lib/api", () => ({
  playAudioFile: vi.fn(),
}));

const NOW = new Date("2026-07-13T12:00:00");
const iso = (d: Date) => d.toISOString();
const entry = (id: number, text: string, at: Date) => ({
  id,
  created_at: iso(at),
  source_type: "note",
  role: "user",
  mode: "note",
  raw_text: text,
  processed_text: text,
  container_id: 3,
  audio_ref: null,
  metadata: {},
});

// Stateful across the "delete notebook" test: once a delete happens,
// listContainers stops returning the deleted notebook — mirroring the real
// IPC round-trip that the auto-select effect can otherwise race against.
let deleted = false;
const quickNoteContainer = { id: 3, container_type: "notebook", title: "Quick Note", parent_id: null, created_at: iso(NOW), updated_at: iso(NOW), metadata: {} };
const zhaichaoContainer = { id: 7, container_type: "notebook", title: "摘抄", parent_id: null, created_at: iso(NOW), updated_at: iso(NOW), metadata: {} };

vi.mock("../../lib/storage-api", () => ({
  listContainers: vi.fn(async () => {
    // A real IPC round-trip crosses a task boundary — delay resolution so
    // tests can observe React re-rendering against a stale notebooks list
    // while a reload is in flight (the actual bug this file regresses).
    await new Promise((resolve) => setTimeout(resolve, 10));
    return deleted ? [quickNoteContainer] : [quickNoteContainer, zhaichaoContainer];
  }),
  // Deliberately out of order: newest first, like the backend can return.
  getContainerEntries: vi.fn(async () => [
    entry(2, "second", new Date("2026-07-13T11:00:00")),
    entry(1, "first", new Date("2026-07-12T10:00:00")),
  ]),
  updateEntry: vi.fn(async () => {}),
  deleteEntry: vi.fn(async () => {}),
  deleteContainer: vi.fn(async () => {
    deleted = true;
  }),
  exportNotebookMd: vi.fn(async () => ""),
  exportNotebookJson: vi.fn(async () => ""),
}));

beforeEach(() => {
  deleted = false;
});

describe("Notes document-flow view", () => {
  it("renders entries oldest-first with day group headers", async () => {
    render(<Notes />);
    await waitFor(() => {
      expect(screen.getAllByTestId("entry-text")).toHaveLength(2);
    });
    const texts = screen.getAllByTestId("entry-text").map((el) => el.textContent);
    expect(texts[0]).toContain("first");
    expect(texts[1]).toContain("second");
  });

  it("renders entries as borderless paragraphs (no card border class)", async () => {
    render(<Notes />);
    await waitFor(() => {
      expect(screen.getAllByTestId("entry-card")).toHaveLength(2);
    });
    for (const el of screen.getAllByTestId("entry-card")) {
      expect(el.className).not.toContain("border");
    }
  });
});

describe("notebook header actions", () => {
  it("hides delete for system notebooks, shows it for custom ones", async () => {
    render(<Notes />);
    // Quick Note auto-selected first.
    await waitFor(() => expect(screen.getAllByTestId("entry-text").length).toBeGreaterThan(0));
    expect(screen.queryByTestId("delete-notebook-btn")).toBeNull();

    fireEvent.click(screen.getByText("摘抄"));
    await waitFor(() => expect(screen.getByTestId("delete-notebook-btn")).toBeInTheDocument());
  });

  it("delete requires a second confirming click, then calls deleteContainer", async () => {
    render(<Notes />);
    await waitFor(() => expect(screen.getAllByTestId("entry-text").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByText("摘抄"));
    const btn = await screen.findByTestId("delete-notebook-btn");

    fireEvent.click(btn);
    expect(deleteContainer).not.toHaveBeenCalled();
    fireEvent.click(btn);
    await waitFor(() => expect(deleteContainer).toHaveBeenCalledWith(7));
  });

  it("export menu lives in the notebook header", async () => {
    render(<Notes />);
    await waitFor(() => expect(screen.getByTestId("export-notebook-btn")).toBeInTheDocument());
  });

  it("recovers selection after deleting the deep-linked notebook", async () => {
    render(<Notes initialNotebookId={7} />);
    // Deep link to 摘抄 (id 7) puts it in view on load.
    const btn = await screen.findByTestId("delete-notebook-btn");
    expect(screen.getByText("摘抄")).toBeInTheDocument();

    fireEvent.click(btn); // arms the confirm state
    fireEvent.click(btn); // confirms — deletes notebook 7
    await waitFor(() => expect(deleteContainer).toHaveBeenCalledWith(7));

    // The deleted notebook must not come back via the stale deep-link
    // preference once the fresh notebook list lands, and the view must not
    // get stuck with a dead selection: the header (export menu) must still
    // render — it only renders when a real, existing notebook is selected —
    // while the delete button stays gone (Quick Note is a system notebook).
    await waitFor(() => {
      expect(screen.queryByText("摘抄")).toBeNull();
      expect(screen.queryByTestId("delete-notebook-btn")).toBeNull();
      expect(screen.getByTestId("export-notebook-btn")).toBeInTheDocument();
    });
  });
});
