// Custom provider entries (R3), macOS. Local: a fifth manual tile with no
// detection three-state and no install/start CTA — just the URL + Probe path.
// Cloud: a Custom provider whose apply gate is relaxed (base URL + ≥1 row, no
// API key) because LAN servers are often keyless.

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

// OMLX installed-but-stopped (mac default) so the CTA renders for it — proving
// the custom tile's *absence* of a CTA is a real difference, not a global off.
const DETECTION: EngineDetection[] = [
  { engine: "omlx", running: false, installed: true, url: "http://localhost:8000", evidence: ["path"] },
  { engine: "lmstudio", running: false, installed: false, url: "http://localhost:1234", evidence: [] },
  { engine: "ollama", running: false, installed: false, url: "http://localhost:11434", evidence: [] },
  { engine: "vllm", running: false, installed: false, url: "http://localhost:8000", evidence: [] },
];

vi.mock("../../lib/api", () => ({
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "" })),
  saveConfig: vi.fn(async () => {}),
  scenarioProbe: vi.fn(async () => ({ reachable: false, latency_ms: 0, models: [], classified: { stt: [], llm: [], tts: [] }, tts_rtfs: {}, plan: { stt: null, llm: null, conversation_tts: null, listen_tts: null } })),
  engineDetect: vi.fn(async () => DETECTION),
  detectHardware: vi.fn(async () => ({ mem_bytes: 16e9, chip: "Apple M3", has_nvidia_gpu: false, tier: "balanced" })),
  checkDiskSpace: vi.fn(async () => ({ available_kb: 500_000_000 })),
}));

describe("Scenarios · Custom local tile (macOS)", () => {
  it("renders a manual state with no detection badge and no install/start CTA", async () => {
    render(<Scenarios mode="overlay" onDone={() => {}} />);
    fireEvent.click(screen.getByText("scen.local.name"));

    // The mac default (OMLX, installed-stopped) has a CTA…
    await screen.findByTestId("engine-setup-cta");
    // …but the Custom tile shows a muted "manual" line, no three-state badge.
    const manual = screen.getByTestId("engine-manual-custom");
    expect(manual.textContent).toBe("scen.manual");
    expect(screen.getByText("Custom")).toBeInTheDocument();

    // Selecting Custom drops the CTA entirely (pure URL + Probe path).
    fireEvent.click(screen.getByText("Custom"));
    await waitFor(() => expect(screen.queryByTestId("engine-setup-cta")).toBeNull());
    // No evidence line rendered for the manual tile.
    expect(screen.queryByTestId("engine-evidence-custom")).toBeNull();
    // The manual probe escape hatch is still available.
    expect(screen.getByText("scen.probe")).toBeInTheDocument();
  });
});

describe("Scenarios · Custom cloud provider gate (macOS)", () => {
  const applyBtn = () => screen.getByText("scen.apply").closest("button") as HTMLButtonElement;

  async function openCustomCloud() {
    render(<Scenarios mode="overlay" onDone={() => {}} />);
    fireEvent.click(screen.getByText("scen.cloud.name"));
    await waitFor(() => expect(screen.getByTestId("cloud-row-llm")).toBeInTheDocument());
    fireEvent.click(screen.getByText("Custom"));
  }

  it("requires a base URL + one row, but no API key", async () => {
    await openCustomCloud();
    // Empty → cannot apply, and the hint points at the URL, not the key.
    expect(applyBtn()).toBeDisabled();
    expect(screen.getByText("scen.custom.needurl")).toBeInTheDocument();

    // Base URL alone is not enough (needs a model row).
    fireEvent.change(screen.getByTestId("cloud-base-url"), {
      target: { value: "http://192.168.1.9:1234/v1" },
    });
    expect(applyBtn()).toBeDisabled();

    // Base URL + one model row → applicable with no key entered.
    fireEvent.change(screen.getByTestId("cloud-row-llm"), { target: { value: "my-local-model" } });
    expect(applyBtn()).not.toBeDisabled();
  });

  it("does not accept an API key as a substitute for the base URL", async () => {
    await openCustomCloud();
    // A key + a row but no base URL stays blocked (unlike preset providers).
    fireEvent.change(screen.getByTestId("cloud-row-llm"), { target: { value: "my-local-model" } });
    fireEvent.change(screen.getByPlaceholderText("scen.apikey.ph"), { target: { value: "sk-x" } });
    expect(applyBtn()).toBeDisabled();
  });
});
