// LocalStep wiring (onboarding P3 Task 9): two-layer detection → three-state
// engine display → platform-optimal default → one-click setup that opens the
// EngineSetupReview card → review-done re-detects and auto-probes into the
// existing plan-row machinery. EngineSetupReview and the api wrappers are
// mocked; this exercises Scenarios' own wiring, not the review card internals
// (covered by EngineSetupReview.test.tsx).

import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import Scenarios from "../Scenarios";
import { engineDetect, scenarioProbe } from "../../lib/api";
import type { EngineDetection } from "../../types";

vi.mock("../../lib/i18n", () => ({
  t: (k: string) => k,
  td: (k: string, args: string[]) => `${k}:${args.join(",")}`,
  useT: () => 0,
}));

vi.mock("../../lib/platform", () => ({ isMacOS: true }));

// EngineSetupReview stub — surfaces its props so the test can assert the
// engine/tier handed in and drive the done/cancel callbacks.
vi.mock("../../components/EngineSetupReview", () => ({
  default: ({
    engineName,
    tier,
    onDone,
    onCancel,
  }: {
    engineName: string;
    tier: string;
    onDone: () => void;
    onCancel: () => void;
  }) => (
    <div data-testid="review-stub">
      <span data-testid="review-engine">{engineName}</span>
      <span data-testid="review-tier">{tier}</span>
      <button data-testid="review-done" onClick={onDone}>
        done
      </button>
      <button data-testid="review-cancel" onClick={onCancel}>
        cancel
      </button>
    </div>
  ),
}));

const DETECTION: EngineDetection[] = [
  { engine: "omlx", running: false, installed: true, url: "http://localhost:8000" }, // mac default → installed, stopped
  { engine: "lmstudio", running: false, installed: false, url: "http://localhost:1234" }, // absent
  { engine: "ollama", running: true, installed: true, url: "http://localhost:11434" }, // running
  { engine: "vllm", running: false, installed: false, url: "http://localhost:8000" },
];

const engineDetectMock = vi.fn(async () => DETECTION);
const scenarioProbeMock = vi.fn(async () => ({
  reachable: true,
  latency_ms: 12,
  models: ["m1"],
  classified: { stt: [], llm: ["m1"], tts: [] },
  tts_rtfs: {},
  plan: { stt: null, llm: "m1", conversation_tts: null, listen_tts: null },
}));

vi.mock("../../lib/api", () => ({
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "" })),
  saveConfig: vi.fn(async () => {}),
  scenarioProbe: (...a: unknown[]) => scenarioProbeMock(...(a as [])),
  engineDetect: (...a: unknown[]) => engineDetectMock(...(a as [])),
  detectHardware: vi.fn(async () => ({ mem_bytes: 16e9, chip: "Apple M3", has_nvidia_gpu: false, tier: "balanced" })),
  checkDiskSpace: vi.fn(async () => ({ available_kb: 500_000_000 })),
}));

async function openLocal() {
  render(<Scenarios mode="overlay" onDone={() => {}} />);
  fireEvent.click(screen.getByText("scen.local.name"));
}

describe("Scenarios · LocalStep wiring (macOS)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    engineDetectMock.mockResolvedValue(DETECTION);
    scenarioProbeMock.mockResolvedValue({
      reachable: true,
      latency_ms: 12,
      models: ["m1"],
      classified: { stt: [], llm: ["m1"], tts: [] },
      tts_rtfs: {},
      plan: { stt: null, llm: "m1", conversation_tts: null, listen_tts: null },
    });
  });

  it("runs two-layer detection on mount and renders the three engine states", async () => {
    await openLocal();
    await waitFor(() => expect(engineDetectMock).toHaveBeenCalled());
    // running → detected; installed-not-running → installed.stopped; absent → notdetected
    await screen.findByText("scen.detected");
    expect(screen.getByText("scen.installed.stopped")).toBeInTheDocument();
    expect(screen.getAllByText("scen.notdetected").length).toBeGreaterThan(0);
  });

  it("offers a Start CTA for the installed-but-stopped platform default and opens the review card", async () => {
    await openLocal();
    const cta = await screen.findByTestId("engine-setup-cta");
    // mac default = omlx, which is installed but not running → Start, not Install.
    expect(cta.textContent).toBe("scen.setup.start");

    fireEvent.click(cta);
    expect(await screen.findByTestId("review-stub")).toBeInTheDocument();
    expect(screen.getByTestId("review-engine").textContent).toBe("OMLX");
    // hardware tier flows into the review card.
    expect(screen.getByTestId("review-tier").textContent).toBe("balanced");
  });

  it("offers an Install CTA for an engine that is neither installed nor running", async () => {
    await openLocal();
    await screen.findByTestId("engine-setup-cta");
    fireEvent.click(screen.getByText("LM Studio")); // absent engine
    await waitFor(() =>
      expect(screen.getByTestId("engine-setup-cta").textContent).toBe("scen.setup.install")
    );
  });

  it("hides the CTA for a running engine (the normal probe path takes over)", async () => {
    await openLocal();
    await screen.findByTestId("engine-setup-cta");
    fireEvent.click(screen.getByText("Ollama")); // running engine
    await waitFor(() => expect(screen.queryByTestId("engine-setup-cta")).toBeNull());
  });

  it("review done re-detects and auto-probes into the plan rows", async () => {
    await openLocal();
    const cta = await screen.findByTestId("engine-setup-cta");
    expect(engineDetectMock).toHaveBeenCalledTimes(1);
    fireEvent.click(cta);
    fireEvent.click(await screen.findByTestId("review-done"));

    // The review card closes, the engine is re-detected, and the normal probe
    // flow takes over automatically.
    await waitFor(() => expect(screen.queryByTestId("review-stub")).toBeNull());
    await waitFor(() => expect(engineDetectMock).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(scenarioProbeMock).toHaveBeenCalled());
    // plan rows land (LLM role row from the probe result).
    expect(await screen.findByText("scen.role.llm")).toBeInTheDocument();
  });

  it("degrades to the manual probe path when detection fails (never a dead screen)", async () => {
    engineDetectMock.mockRejectedValueOnce(new Error("no ipc"));
    await openLocal();
    await waitFor(() => expect(engineDetectMock).toHaveBeenCalled());
    // No CTA (nothing detected) but the manual Probe escape hatch still works.
    expect(screen.queryByTestId("engine-setup-cta")).toBeNull();
    fireEvent.click(screen.getByText("scen.probe"));
    await waitFor(() => expect(scenarioProbeMock).toHaveBeenCalled());
  });
});
