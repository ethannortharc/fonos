import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import type { ComponentProps } from "react";
import EngineSetupReview from "../EngineSetupReview";
import type { BuiltPlan } from "../../lib/engineSetup";

vi.mock("../../lib/i18n", () => ({
  t: (k: string) => k,
  td: (k: string, args: string[]) => `${k}:${args.join(",")}`,
  useT: () => 0,
}));

const listeners: Record<string, (e: { payload: string }) => void> = {};
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: string }) => void) => {
    listeners[name] = cb;
    return () => delete listeners[name];
  }),
}));

const engineSetupMock = vi.fn(async (_plan: unknown) => {});
vi.mock("../../lib/api", () => ({
  engineSetup: (plan: unknown) => engineSetupMock(plan),
}));

// Ample disk unless a test overrides it — the built fixture below needs ≈9.3 GB.
const DISK = 500_000_000; // KB → 500 GB

const built: BuiltPlan = {
  plan: { engine: "ollama", install: true, start: true, pulls: ["qwen3:14b"], base_url: "http://localhost:11434" },
  rows: [{ kind: "install" }, { kind: "start" }, { kind: "pull", model: "qwen3:14b", sizeGb: 9.3 }],
  diskOk: true,
  requiredGb: 9.3,
  downgrade: null,
};

function renderReview(overrides: Partial<ComponentProps<typeof EngineSetupReview>> = {}) {
  const props: ComponentProps<typeof EngineSetupReview> = {
    built,
    engineName: "Ollama",
    tier: "balanced",
    diskAvailableKb: DISK,
    onRetier: () => {},
    onCancel: () => {},
    onDone: () => {},
    ...overrides,
  };
  return render(<EngineSetupReview {...props} />);
}

describe("EngineSetupReview", () => {
  it("renders rows and starts setup on confirm, reporting progress → done", async () => {
    const onDone = vi.fn();
    renderReview({ onDone });
    expect(screen.getByTestId("review-row-install")).toBeInTheDocument();
    expect(screen.getByTestId("review-row-pull")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalledWith(built.plan));

    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "pull", model: "qwen3:14b", pct: 42 }) });
    await waitFor(() => expect(screen.getByTestId("review-progress").textContent).toContain("42"));

    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "done" }) });
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it("blocks confirm and offers downgrade when disk is insufficient", () => {
    const onRetier = vi.fn();
    // 5 GB free can't hold the balanced pull (≈9.3 GB) → recomputed diskOk false.
    renderReview({ diskAvailableKb: 5_000_000, onRetier });
    expect(screen.getByTestId("review-confirm")).toBeDisabled();
    fireEvent.click(screen.getByTestId("review-downgrade"));
    expect(onRetier).toHaveBeenCalledWith("light");
  });

  it("shows the error stage with a message", async () => {
    renderReview();
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalled());
    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "error", message: "pull failed" }) });
    await waitFor(() => expect(screen.getByTestId("review-error").textContent).toContain("pull failed"));
  });

  it("pull failure offers a downgrade when a lower tier exists", async () => {
    const onRetier = vi.fn();
    renderReview({ onRetier });
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalled());
    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "error", failed_stage: "pull", message: "pull failed" }) });
    const btn = await screen.findByTestId("review-error-downgrade");
    fireEvent.click(btn);
    expect(onRetier).toHaveBeenCalledWith("light");
  });

  // ── Reconciliations against shipped backend (T6 post-reconciliation) ──
  // EngineSetupEvent carries `failed_stage` on every error, incl. "busy" for
  // the re-entrancy rejection, and a terminal "manual" stage for engines with
  // no automated install. The brief's snapshot predates these; the paths below
  // lock in the reconciled behavior.

  it("busy rejection shows an already-running notice and offers no downgrade", async () => {
    const onRetier = vi.fn();
    renderReview({ onRetier });
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalled());
    listeners["engine:setup"]({
      payload: JSON.stringify({ stage: "error", failed_stage: "busy", message: "engine setup already in progress" }),
    });
    const err = await screen.findByTestId("review-error");
    expect(err.textContent).toContain("engine.review.busy");
    expect(screen.queryByTestId("review-error-downgrade")).toBeNull();
  });

  it("install failure offers no downgrade (a smaller model won't fix an install)", async () => {
    renderReview();
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalled());
    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "error", failed_stage: "install", message: "brew failed" }) });
    const err = await screen.findByTestId("review-error");
    expect(err.textContent).toContain("brew failed");
    expect(screen.queryByTestId("review-error-downgrade")).toBeNull();
  });

  it("renders a terminal manual notice for engines with no automated install", async () => {
    renderReview();
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalled());
    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "manual", message: "install it from ollama.com" }) });
    const notice = await screen.findByTestId("review-manual");
    expect(notice.textContent).toContain("install it from ollama.com");
    expect(screen.queryByTestId("review-progress")).toBeNull();
  });

  it("ignores events from a different engine and only fires onDone for the matching engine", async () => {
    const onDone = vi.fn();
    renderReview({ onDone });
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalled());

    // Send a done event from a different engine — should be ignored
    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "done", engine: "vllm" }) });
    expect(onDone).not.toHaveBeenCalled();

    // Send done from the matching engine — should fire onDone
    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "done", engine: "ollama" }) });
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  // ── Editable pull models (Finding 4) ──────────────────────────────────────

  it("changing the model via the select sends the edited pull list", async () => {
    renderReview();
    const select = screen.getByTestId("review-pull-select") as HTMLSelectElement;
    expect(select.value).toBe("qwen3:14b"); // seeded from the built plan
    fireEvent.change(select, { target: { value: "qwen3:4b" } }); // light tier
    // Size label follows the picked tier's known size.
    await waitFor(() => expect(screen.getByTestId("review-row-pull").textContent).toContain("2.6"));

    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() =>
      expect(engineSetupMock).toHaveBeenCalledWith(expect.objectContaining({ pulls: ["qwen3:4b"] }))
    );
  });

  it("removing the only pull row confirms with an empty pull list", async () => {
    renderReview();
    fireEvent.click(screen.getByTestId("review-pull-remove"));
    expect(screen.queryByTestId("review-row-pull")).toBeNull();

    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() =>
      expect(engineSetupMock).toHaveBeenCalledWith(expect.objectContaining({ pulls: [] }))
    );
  });

  it("adding a custom model reveals a free-text input, shows unknown size, and sends it", async () => {
    renderReview();
    fireEvent.click(screen.getByTestId("review-pull-add"));
    const customInputs = screen.getAllByTestId("review-pull-custom");
    expect(customInputs).toHaveLength(1);
    // The new custom row has no known size.
    expect(screen.getAllByText("engine.review.size.unknown").length).toBeGreaterThan(0);

    fireEvent.change(customInputs[0], { target: { value: "phi4:mini" } });
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() =>
      expect(engineSetupMock).toHaveBeenCalledWith(
        expect.objectContaining({ pulls: ["qwen3:14b", "phi4:mini"] })
      )
    );
  });

  it("recomputes the disk verdict when the model changes (bigger model → blocked)", () => {
    // 15 GB free: fits balanced (≈9.3) but not max (≈18.6).
    renderReview({ diskAvailableKb: 15_000_000, tier: "balanced" });
    expect(screen.getByTestId("review-confirm")).not.toBeDisabled();
    fireEvent.change(screen.getByTestId("review-pull-select"), { target: { value: "qwen3:30b-a3b" } });
    expect(screen.getByTestId("review-confirm")).toBeDisabled();
    expect(screen.getByTestId("review-downgrade")).toBeInTheDocument();
  });
});
