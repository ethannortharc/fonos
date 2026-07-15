import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import Onboarding from "../Onboarding";
import { saveConfig, requestAccessibility, startRecording, stopRecording, checkAccessibility } from "../../lib/api";

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
  getConfig: vi.fn(async () => ({ model_profiles: [], stt_profile: "", hotkey_dictation: "cmd+shift+space" })),
  saveConfig: vi.fn(async () => {}),
  checkAccessibility: vi.fn(async () => false),
  requestAccessibility: vi.fn(async () => false),
  startRecording: vi.fn(async () => {}),
  stopRecording: vi.fn(async () => ({ text: "" })),
  recordOnboardingEvent: vi.fn(async () => true),
}));

/** Drive the flow from welcome into the guided step. "Not now" no longer routes
 *  here (Fix 1: it finishes the wizard) — a *granted* Accessibility check is the
 *  only path to guided, and it auto-advances accessibility → guided. */
async function intoGuided() {
  fireEvent.click(screen.getByTestId("ob-start"));
  await waitFor(() => expect(listeners["float:stop"]).toBeDefined());
  act(() => listeners["float:stop"]({ payload: "hi" }));
  vi.mocked(checkAccessibility).mockResolvedValue(true);
  fireEvent.click(screen.getByTestId("ob-next")); // → accessibility → (auto) guided
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

  it("hold-to-talk button drives start_recording on press and stop_recording on release", async () => {
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    const ptt = await screen.findByTestId("ob-ptt");
    // Fix 4: the button only arms once the Apple-STT seed save resolves.
    await waitFor(() => expect(ptt).not.toBeDisabled());
    fireEvent.pointerDown(ptt);
    expect(startRecording).toHaveBeenCalledTimes(1);
    expect(stopRecording).not.toHaveBeenCalled();
    fireEvent.pointerUp(ptt);
    expect(stopRecording).toHaveBeenCalledTimes(1);
    // pointerleave after release must not fire a second stop (guarded).
    fireEvent.pointerLeave(ptt);
    expect(stopRecording).toHaveBeenCalledTimes(1);
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
    // Keep AX untrusted so the step doesn't auto-advance past the grant button.
    vi.mocked(checkAccessibility).mockResolvedValue(false);
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    await waitFor(() => expect(listeners["float:stop"]).toBeDefined());
    act(() => listeners["float:stop"]({ payload: "hi" }));
    fireEvent.click(screen.getByTestId("ob-next"));
    fireEvent.click(await screen.findByTestId("ob-ax-grant"));
    expect(requestAccessibility).toHaveBeenCalled();
  });

  it("Fix 1: 'Not now' on the accessibility step finishes the wizard and never reaches guided", async () => {
    // Without AX the guided task is impossible (InsertOutput returns via the
    // popup Panel before dictation:delivered fires; the global hotkey is a
    // CGEventTap that can't exist without AX) — so "Not now" must complete
    // onboarding, not advance to a guided step whose Finish can never enable.
    vi.mocked(checkAccessibility).mockResolvedValue(false);
    const onDone = vi.fn();
    render(<Onboarding onDone={onDone} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    await waitFor(() => expect(listeners["float:stop"]).toBeDefined());
    act(() => listeners["float:stop"]({ payload: "hi" }));
    fireEvent.click(screen.getByTestId("ob-next")); // → accessibility
    fireEvent.click(await screen.findByTestId("ob-ax-later"));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    const persisted = vi
      .mocked(saveConfig)
      .mock.calls.map((c) => JSON.parse(c[0] as string));
    expect(persisted.some((p) => p.has_completed_onboarding === true)).toBe(true);
    // The guided step (and its Finish button) never rendered.
    expect(screen.queryByTestId("ob-finish")).toBeNull();
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

  it("Fix 4: hold-to-talk stays disabled until the Apple-STT seed save resolves (init race)", async () => {
    // Hold the seeding write pending so we can observe the button is not armed.
    let resolveSave!: () => void;
    vi.mocked(saveConfig).mockImplementationOnce(
      () => new Promise<void>((res) => { resolveSave = () => res(); })
    );
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    const ptt = await screen.findByTestId("ob-ptt");
    await waitFor(() => expect(saveConfig).toHaveBeenCalled());
    // Seed save still pending → recording must not be armed yet.
    expect(ptt).toBeDisabled();
    // Resolve the seed → the button arms.
    await act(async () => { resolveSave(); });
    await waitFor(() => expect(ptt).not.toBeDisabled());
  });

  it("Fix 4: a recording failure shows a visible error and clears on the next successful press", async () => {
    vi.mocked(startRecording).mockRejectedValueOnce(new Error("mic blocked"));
    render(<Onboarding onDone={() => {}} />);
    fireEvent.click(screen.getByTestId("ob-start"));
    const ptt = await screen.findByTestId("ob-ptt");
    await waitFor(() => expect(ptt).not.toBeDisabled());
    // First press: startRecording rejects → error line appears, button resets.
    fireEvent.pointerDown(ptt);
    expect(await screen.findByTestId("ob-play-error")).toBeInTheDocument();
    // Second (successful) press clears the error.
    fireEvent.pointerDown(ptt);
    await waitFor(() => expect(screen.queryByTestId("ob-play-error")).toBeNull());
    fireEvent.pointerUp(ptt);
  });
});
