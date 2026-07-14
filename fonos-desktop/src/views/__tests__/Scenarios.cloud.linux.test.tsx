// CloudStep role-coverage placeholders (R4), Linux. Off-macOS there is no Apple
// STT fallback, so an LLM-only provider's dictation row is an editable input
// carrying the explanatory no-stt hint (not the generic model-id one).

import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import Scenarios from "../Scenarios";
import type { EngineDetection } from "../../types";

vi.mock("../../lib/i18n", () => ({
  t: (k: string) => k,
  td: (k: string, args: string[]) => `${k}:${args.join(",")}`,
  useT: () => 0,
}));

vi.mock("../../lib/platform", () => ({ isMacOS: false }));
vi.mock("../../components/EngineSetupReview", () => ({ default: () => null }));

const DETECTION: EngineDetection[] = [
  { engine: "ollama", running: false, installed: false, url: "http://localhost:11434", evidence: [] },
];

vi.mock("../../lib/api", () => ({
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "" })),
  saveConfig: vi.fn(async () => {}),
  scenarioProbe: vi.fn(async () => ({ reachable: false, latency_ms: 0, models: [], classified: { stt: [], llm: [], tts: [] }, tts_rtfs: {}, plan: { stt: null, llm: null, conversation_tts: null, listen_tts: null } })),
  engineDetect: vi.fn(async () => DETECTION),
  detectHardware: vi.fn(async () => ({ mem_bytes: 32e9, chip: "x86_64", has_nvidia_gpu: true, tier: "max" })),
  checkDiskSpace: vi.fn(async () => ({ available_kb: 900_000_000 })),
}));

const ph = (role: string) => (screen.getByTestId(`cloud-row-${role}`) as HTMLInputElement).placeholder;

describe("Scenarios · CloudStep role-coverage (Linux)", () => {
  it("shows the no-stt hint on the dictation row for an LLM-only provider off-macOS", async () => {
    render(<Scenarios mode="overlay" onDone={() => {}} />);
    fireEvent.click(screen.getByText("scen.cloud.name"));
    await waitFor(() => expect(screen.getByTestId("cloud-row-llm")).toBeInTheDocument());
    fireEvent.click(screen.getByText("Cerebras"));
    // No Apple fallback here → the STT row is an editable input with the hint.
    await waitFor(() => expect(ph("stt")).toBe("scen.cloud.ph.no-stt"));
    expect(ph("conv")).toBe("scen.cloud.ph.no-tts");
  });
});
