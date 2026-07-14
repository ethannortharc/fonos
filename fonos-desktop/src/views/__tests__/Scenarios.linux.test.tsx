// Platform-optimal default (onboarding P3 Task 9): on Linux the local channel
// defaults to Ollama (no OMLX/Apple pipeline), so the setup CTA and review card
// target Ollama out of the box. macOS defaulting is covered in
// Scenarios.local.test.tsx.
//
// Also covers Finding 1 (P3 Task 9 fix round 1): Apple on-device Speech is
// macOS-only, so the local flow must never sentinel-fall to it off-macOS —
// neither in the auto-assigned plan row nor as a selectable option nor in the
// specs an Apply actually writes.

import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import Scenarios from "../Scenarios";
import type { EngineDetection, ScenarioProbe } from "../../types";

vi.mock("../../lib/i18n", () => ({
  t: (k: string) => k,
  td: (k: string, args: string[]) => `${k}:${args.join(",")}`,
  useT: () => 0,
}));

vi.mock("../../lib/platform", () => ({ isMacOS: false }));

vi.mock("../../components/EngineSetupReview", () => ({
  default: ({ engineName }: { engineName: string }) => (
    <div data-testid="review-stub">
      <span data-testid="review-engine">{engineName}</span>
    </div>
  ),
}));

const DETECTION: EngineDetection[] = [
  { engine: "omlx", running: false, installed: false, url: "http://localhost:8000", evidence: [] },
  { engine: "lmstudio", running: false, installed: false, url: "http://localhost:1234", evidence: [] },
  { engine: "ollama", running: false, installed: false, url: "http://localhost:11434", evidence: [] },
  { engine: "vllm", running: false, installed: false, url: "http://localhost:8000", evidence: [] },
];

const saveConfigMock = vi.fn(async (..._args: unknown[]) => {});
const scenarioProbeMock = vi.fn(async (): Promise<ScenarioProbe> => ({
  reachable: false,
  latency_ms: 0,
  models: [],
  classified: { stt: [], llm: [], tts: [] },
  tts_rtfs: {},
  plan: { stt: null, llm: null, conversation_tts: null, listen_tts: null },
}));

vi.mock("../../lib/api", () => ({
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "" })),
  saveConfig: (...a: unknown[]) => saveConfigMock(...(a as [])),
  scenarioProbe: (...a: unknown[]) => scenarioProbeMock(...(a as [])),
  engineDetect: vi.fn(async () => DETECTION),
  detectHardware: vi.fn(async () => ({ mem_bytes: 32e9, chip: "x86_64", has_nvidia_gpu: true, tier: "max" })),
  checkDiskSpace: vi.fn(async () => ({ available_kb: 900_000_000 })),
}));

describe("Scenarios · LocalStep wiring (Linux)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    scenarioProbeMock.mockResolvedValue({
      reachable: false,
      latency_ms: 0,
      models: [],
      classified: { stt: [], llm: [], tts: [] },
      tts_rtfs: {},
      plan: { stt: null, llm: null, conversation_tts: null, listen_tts: null },
    } satisfies ScenarioProbe);
  });

  it("defaults the local channel to Ollama and targets it in the setup review", async () => {
    render(<Scenarios mode="overlay" onDone={() => {}} />);
    fireEvent.click(screen.getByText("scen.local.name"));

    const cta = await screen.findByTestId("engine-setup-cta");
    // Ollama is auto-installable and absent → Install & start.
    expect(cta.textContent).toBe("scen.setup.install");
    fireEvent.click(cta);
    expect((await screen.findByTestId("review-engine")).textContent).toBe("Ollama");
  });

  it("never assigns Apple STT in the local flow — not as a plan-row option, not in the applied specs", async () => {
    // A reachable probe against a non-full engine (Ollama) with no explicit
    // STT pick — pre-fix, runProbe's sentinel fallback forced "apple" here
    // regardless of platform.
    scenarioProbeMock.mockResolvedValue({
      reachable: true,
      latency_ms: 9,
      models: ["some-stt-model", "m1"],
      classified: { stt: ["some-stt-model"], llm: ["m1"], tts: [] },
      tts_rtfs: {},
      plan: { stt: null, llm: "m1", conversation_tts: null, listen_tts: null },
    } satisfies ScenarioProbe);

    render(<Scenarios mode="overlay" onDone={() => {}} />);
    fireEvent.click(screen.getByText("scen.local.name"));
    fireEvent.click(screen.getByText("scen.probe"));
    await waitFor(() => expect(scenarioProbeMock).toHaveBeenCalled());

    // The STT plan row never offers — and never auto-selects — "apple".
    await screen.findByText("scen.role.stt");
    expect(screen.queryByText("scen.apple")).toBeNull();

    // Apply and inspect the actual specs written — no apple provider profile,
    // no stt_profile pointed at one.
    fireEvent.click(screen.getByText("scen.apply"));
    await waitFor(() => expect(saveConfigMock).toHaveBeenCalled());
    const payload = JSON.parse((saveConfigMock.mock.calls[0]![0] as unknown) as string);
    expect(
      (payload.model_profiles ?? []).some((p: { provider: string }) => p.provider === "apple")
    ).toBe(false);
  });
});
