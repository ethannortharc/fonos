import { render, screen, fireEvent } from "@testing-library/react";
import NotebookCombobox from "../NotebookCombobox";
import type { Container } from "../../../lib/storage-api";

vi.mock("../../../lib/i18n", () => ({
  t: (k: string) => k,
  useT: () => 0,
}));

const nb = (id: number, title: string): Container => ({
  id,
  container_type: "notebook",
  title,
  parent_id: null,
  created_at: "2026-07-13T00:00:00Z",
  updated_at: "2026-07-13T00:00:00Z",
  metadata: {},
});

const books = [nb(3, "Quick Note"), nb(7, "摘抄")];

describe("NotebookCombobox", () => {
  it("shows the bound notebook's title for a known container id", () => {
    render(
      <NotebookCombobox containerId={7} pendingTitle="" notebooks={books} onChange={() => {}} />
    );
    expect(screen.getByTestId("notebook-combobox-input")).toHaveValue("摘抄");
  });

  it("typing an unknown name fires a create selection and shows the hint", () => {
    const onChange = vi.fn();
    render(
      <NotebookCombobox containerId={0} pendingTitle="" notebooks={books} onChange={onChange} />
    );
    fireEvent.change(screen.getByTestId("notebook-combobox-input"), {
      target: { value: "灵感" },
    });
    expect(onChange).toHaveBeenLastCalledWith({ kind: "create", title: "灵感" });
    expect(screen.getByTestId("notebook-combobox-hint")).toBeInTheDocument();
  });

  it("picking a suggestion fires an existing selection", () => {
    const onChange = vi.fn();
    render(
      <NotebookCombobox containerId={0} pendingTitle="" notebooks={books} onChange={onChange} />
    );
    fireEvent.focus(screen.getByTestId("notebook-combobox-input"));
    fireEvent.mouseDown(screen.getByTestId("notebook-option-7"));
    expect(onChange).toHaveBeenLastCalledWith({ kind: "existing", container_id: 7 });
    expect(screen.getByTestId("notebook-combobox-input")).toHaveValue("摘抄");
  });

  it("typing an exact existing title binds instead of creating", () => {
    const onChange = vi.fn();
    render(
      <NotebookCombobox containerId={0} pendingTitle="" notebooks={books} onChange={onChange} />
    );
    fireEvent.change(screen.getByTestId("notebook-combobox-input"), {
      target: { value: "摘抄" },
    });
    expect(onChange).toHaveBeenLastCalledWith({ kind: "existing", container_id: 7 });
    expect(screen.queryByTestId("notebook-combobox-hint")).toBeNull();
  });
});
