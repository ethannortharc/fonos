import { render, screen, waitFor } from "@testing-library/react";
import Notes from "../Notes";

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

vi.mock("../../lib/storage-api", () => ({
  listContainers: vi.fn(async () => [
    {
      id: 3,
      container_type: "notebook",
      title: "Quick Note",
      parent_id: null,
      created_at: iso(NOW),
      updated_at: iso(NOW),
      metadata: {},
    },
  ]),
  // Deliberately out of order: newest first, like the backend can return.
  getContainerEntries: vi.fn(async () => [
    entry(2, "second", new Date("2026-07-13T11:00:00")),
    entry(1, "first", new Date("2026-07-12T10:00:00")),
  ]),
  updateEntry: vi.fn(async () => {}),
  deleteEntry: vi.fn(async () => {}),
  deleteContainer: vi.fn(async () => {}),
  exportNotebookMd: vi.fn(async () => ""),
  exportNotebookJson: vi.fn(async () => ""),
}));

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
