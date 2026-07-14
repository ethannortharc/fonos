import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import Onboarding from "../Onboarding";
import { saveConfig, requestAccessibility } from "../../lib/api";

const listeners: Record<string, (e: unknown) => void> = {};

vi.mock("../../lib/i18n", () => ({ t: (k: string) => k, useT: () => 0 }));
vi.mock("../../lib/platform", () => ({ isMacOS: true }));
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
  listen: vi.fn(async (name: string, cb: (e: unknown) => void) => {
    listeners[name] = cb;
    return () => {
      delete listeners[name];
    };
  }),
}));
vi.mock("../../lib/api", () => ({
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "" })),
  saveConfig: vi.fn(async () => {}),
  checkAccessibility: vi.fn(async () => false),
  requestAccessibility: vi.fn(async () => false),
  recordOnboardingEvent: vi.fn(async () => true),
}));

/** Drive the flow from welcome into the guided step. */
async function intoGuided() {
  fireEvent.click(screen.getByTestId("ob-start"));
  await waitFor(() => expect(listeners["float:stop"]).toBeDefined());
  act(() => listeners["float:stop"]({ payload: "hi" }));
  fireEvent.click(screen.getByTestId("ob-next")); // → accessibility
  fireEvent.click(await screen.findByTestId("ob-ax-later")); // → guided
  await waitFor(() => expect(listeners["dictation:delivered"]).toBeDefined());
}

describe("Onboarding (macOS flow)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    for (const k of Object.keys(listeners)) delete listeners[k];
  });

  it("welcome → start advances to playground and seeds Apple STT", async () => {
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    expect(await screen.findByTestId("ob-playground-box")).toBeInTheDocument();
    await waitFor(() => expect(saveConfig).toHaveBeenCalled());
    const patch = JSON.parse(vi.mocked(saveConfig).mock.calls[0][0] as string);
    expect(patch.stt_profile).toBe("scenario-apple-stt");
  });

  it("skip goes straight to engine setup; engines onDone finishes the wizard", async () => {
    const onDone = vi.fn();
    render(<Onboarding onDone={onDone} />);
    fireEvent.click(screen.getByTestId("ob-skip"));
    fireEvent.click(await screen.findByTestId("scenarios-stub"));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    const persisted = vi
      .mocked(saveConfig)
      .mock.calls.map((c) => JSON.parse(c[0] as string));
    expect(persisted.some((p) => p.has_completed_onboarding === true)).toBe(true);
  });

  it("float:stop fills the playground and enables Continue", async () => {
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    await waitFor(() => expect(listeners["float:stop"]).toBeDefined());
    expect(screen.getByTestId("ob-next")).toBeDisabled();
    act(() => listeners["float:stop"]({ payload: "hello world" }));
    expect(screen.getByTestId("ob-playground-text")).toHaveTextContent("hello world");
    expect(screen.getByTestId("ob-next")).not.toBeDisabled();
  });

  it("grant button asks the OS for the accessibility prompt", async () => {
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    await waitFor(() => expect(listeners["float:stop"]).toBeDefined());
    act(() => listeners["float:stop"]({ payload: "hi" }));
    fireEvent.click(screen.getByTestId("ob-next"));
    fireEvent.click(await screen.findByTestId("ob-ax-grant"));
    expect(requestAccessibility).toHaveBeenCalled();
  });

  it("guided task completes on dictation:delivered from another app", async () => {
    render(<Onboarding onDone={() => {}} />);
    await intoGuided();
    act(() => listeners["dictation:delivered"]({ payload: { target_app: "Notes" } }));
    expect(await screen.findByTestId("ob-guided-done")).toBeInTheDocument();
    expect(screen.getByTestId("ob-finish")).not.toBeDisabled();
  });

  it("an insertion into Fonos itself does not complete the guided task", async () => {
    render(<Onboarding onDone={() => {}} />);
    await intoGuided();
    act(() => listeners["dictation:delivered"]({ payload: { target_app: "Fonos" } }));
    expect(screen.queryByTestId("ob-guided-done")).toBeNull();
  });
});
