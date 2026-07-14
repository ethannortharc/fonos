// Platform-optimal default (onboarding P3 Task 9): on Linux the local channel
// defaults to Ollama (no OMLX/Apple pipeline), so the setup CTA and review card
// target Ollama out of the box. macOS defaulting is covered in
// Scenarios.local.test.tsx.

import { render, screen, fireEvent } from "@testing-library/react";
import Scenarios from "../Scenarios";
import type { EngineDetection } from "../../types";

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
  { engine: "omlx", running: false, installed: false, url: "http://localhost:8000" },
  { engine: "lmstudio", running: false, installed: false, url: "http://localhost:1234" },
  { engine: "ollama", running: false, installed: false, url: "http://localhost:11434" },
  { engine: "vllm", running: false, installed: false, url: "http://localhost:8000" },
];

vi.mock("../../lib/api", () => ({
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "" })),
  saveConfig: vi.fn(async () => {}),
  scenarioProbe: vi.fn(async () => ({
    reachable: false,
    latency_ms: 0,
    models: [],
    classified: { stt: [], llm: [], tts: [] },
    tts_rtfs: {},
    plan: { stt: null, llm: null, conversation_tts: null, listen_tts: null },
  })),
  engineDetect: vi.fn(async () => DETECTION),
  detectHardware: vi.fn(async () => ({ mem_bytes: 32e9, chip: "x86_64", has_nvidia_gpu: true, tier: "max" })),
  checkDiskSpace: vi.fn(async () => ({ available_kb: 900_000_000 })),
}));

describe("Scenarios · LocalStep wiring (Linux)", () => {
  it("defaults the local channel to Ollama and targets it in the setup review", async () => {
    render(<Scenarios mode="overlay" onDone={() => {}} />);
    fireEvent.click(screen.getByText("scen.local.name"));

    const cta = await screen.findByTestId("engine-setup-cta");
    // Ollama is auto-installable and absent → Install & start.
    expect(cta.textContent).toBe("scen.setup.install");
    fireEvent.click(cta);
    expect((await screen.findByTestId("review-engine")).textContent).toBe("Ollama");
  });
});
