// CloudStep role-coverage placeholders (R4) + Cerebras provider tile (R1),
// macOS. Every cloud plan row is an editable input; when a role is genuinely
// absent the *placeholder* turns into an explanatory hint instead of the
// generic model-id one. The i18n mock echoes keys, so we assert on the raw
// placeholder key each row renders. Off-macOS no-stt is covered in
// Scenarios.cloud.linux.test.tsx.

import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import Scenarios from "../Scenarios";
import type { EngineDetection } from "../../types";

vi.mock("../../lib/i18n", () => ({
  t: (k: string) => k,
  td: (k: string, args: string[]) => `${k}:${args.join(",")}`,
  useT: () => 0,
}));

vi.mock("../../lib/platform", () => ({ isMacOS: true }));
vi.mock("../../components/EngineSetupReview", () => ({ default: () => null }));

const DETECTION: EngineDetection[] = [
  { engine: "omlx", running: false, installed: false, url: "http://localhost:8000", evidence: [] },
];

vi.mock("../../lib/api", () => ({
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "" })),
  saveConfig: vi.fn(async () => {}),
  scenarioProbe: vi.fn(async () => ({ reachable: false, latency_ms: 0, models: [], classified: { stt: [], llm: [], tts: [] }, tts_rtfs: {}, plan: { stt: null, llm: null, conversation_tts: null, listen_tts: null } })),
  engineDetect: vi.fn(async () => DETECTION),
  detectHardware: vi.fn(async () => ({ mem_bytes: 16e9, chip: "Apple M3", has_nvidia_gpu: false, tier: "balanced" })),
  checkDiskSpace: vi.fn(async () => ({ available_kb: 500_000_000 })),
}));

async function openCloud() {
  render(<Scenarios mode="overlay" onDone={() => {}} />);
  fireEvent.click(screen.getByText("scen.cloud.name"));
  await waitFor(() => expect(screen.getByTestId("cloud-row-llm")).toBeInTheDocument());
}

const ph = (role: string) => (screen.getByTestId(`cloud-row-${role}`) as HTMLInputElement).placeholder;

describe("Scenarios · CloudStep role-coverage (macOS)", () => {
  it("offers Cerebras as a provider tile (R1)", async () => {
    await openCloud();
    expect(screen.getByText("Cerebras")).toBeInTheDocument();
  });

  it("shows the no-tts hint on the voice rows for an LLM-only provider", async () => {
    await openCloud();
    fireEvent.click(screen.getByText("OpenRouter"));
    await waitFor(() => expect(ph("conv")).toBe("scen.cloud.ph.no-tts"));
    expect(ph("listen")).toBe("scen.cloud.ph.no-tts");
    // LLM row is prefilled → the generic model-id placeholder, not a hint.
    expect(ph("llm")).toBe("scen.cloud.row.ph");
    // macOS keeps the Apple STT fallback as a static row (no stt input).
    expect(screen.queryByTestId("cloud-row-stt")).toBeNull();
    expect(screen.getByText("scen.apple")).toBeInTheDocument();
  });

  it("shows the no-tts hint for Cerebras voice rows too", async () => {
    await openCloud();
    fireEvent.click(screen.getByText("Cerebras"));
    await waitFor(() => expect(ph("conv")).toBe("scen.cloud.ph.no-tts"));
    expect(ph("listen")).toBe("scen.cloud.ph.no-tts");
  });

  it("keeps generic model-id placeholders when the role is prefilled (OpenAI 3/3)", async () => {
    await openCloud();
    // OpenAI is the default provider — every role has a real model default.
    expect(ph("conv")).toBe("scen.cloud.row.ph");
    expect(ph("listen")).toBe("scen.cloud.row.ph");
    expect(ph("llm")).toBe("scen.cloud.row.ph");
  });
});
