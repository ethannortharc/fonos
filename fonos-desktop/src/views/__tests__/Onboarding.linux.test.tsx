import { render, screen, fireEvent } from "@testing-library/react";
import Onboarding from "../Onboarding";

vi.mock("../../lib/i18n", () => ({ t: (k: string) => k, useT: () => 0 }));
vi.mock("../../lib/platform", () => ({ isMacOS: false }));
vi.mock("../Scenarios", () => ({
  default: ({ onDone }: { onDone: () => void }) => (
    <button data-testid="scenarios-stub" onClick={onDone}>
      engines
    </button>
  ),
  // appleSttSeed imports this from the same module — the mock must keep it.
  isSttConfigured: () => false,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("../../lib/api", () => ({
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "" })),
  saveConfig: vi.fn(async () => {}),
  checkAccessibility: vi.fn(async () => true),
  requestAccessibility: vi.fn(async () => true),
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
});
