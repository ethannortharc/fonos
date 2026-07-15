import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import Onboarding from "../Onboarding";

vi.mock("../../lib/i18n", () => ({ t: (k: string) => k, useT: () => 0 }));
vi.mock("../../lib/platform", () => ({ isMacOS: false }));
vi.mock("../Scenarios", () => ({
  default: ({ onDone }: { onDone: () => void }) => (
    <button data-testid="scenarios-stub" onClick={onDone}>
      engines
    </button>
  ),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("../../lib/api", () => ({
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "", hotkey_dictation: "cmd+shift+space" })),
  saveConfig: vi.fn(async () => {}),
  // Linux has no built-in STT and nothing is seeded → the runtime-backed gate
  // stays false until an engine is configured via the overlay.
  sttConfigured: vi.fn(async () => false),
  checkAccessibility: vi.fn(async () => true),
  requestAccessibility: vi.fn(async () => true),
  startRecording: vi.fn(async () => {}),
  stopRecording: vi.fn(async () => ({ text: "" })),
  recordOnboardingEvent: vi.fn(async () => true),
}));

describe("Onboarding (Linux flow)", () => {
  it("shows the hotkey fallback hint on the welcome screen", () => {
    render(<Onboarding onDone={() => {}} />);
    expect(screen.getByText("ob.linux.hotkey-hint")).toBeInTheDocument();
  });

  it("front-loads engine setup, then continues to the playground", async () => {
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    // Linux: engines comes before the playground (no built-in STT).
    fireEvent.click(await screen.findByTestId("scenarios-stub"));
    expect(await screen.findByTestId("ob-playground-box")).toBeInTheDocument();
  });

  it("warns and offers an engines shortcut when the overlay was closed unconfigured", async () => {
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    // Close the engines overlay via its ✕ (mocked as onDone) without
    // configuring anything — lands on the playground with no STT.
    fireEvent.click(await screen.findByTestId("scenarios-stub"));
    expect(await screen.findByTestId("ob-no-stt")).toBeInTheDocument();
    const toEngines = screen.getByTestId("ob-to-engines");
    expect(toEngines).toBeInTheDocument();
    fireEvent.click(toEngines);
    expect(await screen.findByTestId("scenarios-stub")).toBeInTheDocument();
  });

  it("Fix B: with STT unconfigured the warning shows AND hold-to-talk is disabled", async () => {
    // Regression: on Linux with no seeded engine the playground effect still
    // reaches playState "ready", but sttReady stays false — so a live record
    // button must not sit armed beneath the "no engine" warning.
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    fireEvent.click(await screen.findByTestId("scenarios-stub"));
    expect(await screen.findByTestId("ob-no-stt")).toBeInTheDocument();
    // The seed never runs off macOS, so the effect settles synchronously after
    // the config/sttConfigured pair resolves; the button must be disabled.
    await waitFor(() => expect(screen.getByTestId("ob-ptt")).toBeDisabled());
  });
});
