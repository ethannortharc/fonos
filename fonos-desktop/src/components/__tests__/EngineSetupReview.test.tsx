import { render, screen, fireEvent, waitFor } from "@testing-library/react";
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

const engineSetupMock = vi.fn(async () => {});
vi.mock("../../lib/api", () => ({
  engineSetup: (plan: unknown) => engineSetupMock(plan),
}));

const built: BuiltPlan = {
  plan: { engine: "ollama", install: true, start: true, pulls: ["qwen3:14b"], base_url: "http://localhost:11434" },
  rows: [{ kind: "install" }, { kind: "start" }, { kind: "pull", model: "qwen3:14b", sizeGb: 9.3 }],
  diskOk: true,
  requiredGb: 9.3,
  downgrade: null,
};

describe("EngineSetupReview", () => {
  it("renders rows and starts setup on confirm, reporting progress → done", async () => {
    const onDone = vi.fn();
    render(
      <EngineSetupReview built={built} engineName="Ollama" tier="balanced" onRetier={() => {}} onCancel={() => {}} onDone={onDone} />
    );
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
    render(
      <EngineSetupReview
        built={{ ...built, diskOk: false, downgrade: "light" }}
        engineName="Ollama"
        tier="balanced"
        onRetier={onRetier}
        onCancel={() => {}}
        onDone={() => {}}
      />
    );
    expect(screen.getByTestId("review-confirm")).toBeDisabled();
    fireEvent.click(screen.getByTestId("review-downgrade"));
    expect(onRetier).toHaveBeenCalledWith("light");
  });

  it("shows the error stage with a message", async () => {
    render(
      <EngineSetupReview built={built} engineName="Ollama" tier="balanced" onRetier={() => {}} onCancel={() => {}} onDone={() => {}} />
    );
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalled());
    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "error", message: "pull failed" }) });
    await waitFor(() => expect(screen.getByTestId("review-error").textContent).toContain("pull failed"));
  });

  it("pull failure offers a downgrade when a lower tier exists", async () => {
    const onRetier = vi.fn();
    render(
      <EngineSetupReview built={built} engineName="Ollama" tier="balanced" onRetier={onRetier} onCancel={() => {}} onDone={() => {}} />
    );
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
    render(
      <EngineSetupReview built={built} engineName="Ollama" tier="balanced" onRetier={onRetier} onCancel={() => {}} onDone={() => {}} />
    );
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
    render(
      <EngineSetupReview built={built} engineName="Ollama" tier="balanced" onRetier={() => {}} onCancel={() => {}} onDone={() => {}} />
    );
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalled());
    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "error", failed_stage: "install", message: "brew failed" }) });
    const err = await screen.findByTestId("review-error");
    expect(err.textContent).toContain("brew failed");
    expect(screen.queryByTestId("review-error-downgrade")).toBeNull();
  });

  it("renders a terminal manual notice for engines with no automated install", async () => {
    render(
      <EngineSetupReview built={built} engineName="Ollama" tier="balanced" onRetier={() => {}} onCancel={() => {}} onDone={() => {}} />
    );
    fireEvent.click(screen.getByTestId("review-confirm"));
    await waitFor(() => expect(engineSetupMock).toHaveBeenCalled());
    listeners["engine:setup"]({ payload: JSON.stringify({ stage: "manual", message: "install it from ollama.com" }) });
    const notice = await screen.findByTestId("review-manual");
    expect(notice.textContent).toContain("install it from ollama.com");
    expect(screen.queryByTestId("review-progress")).toBeNull();
  });
});
