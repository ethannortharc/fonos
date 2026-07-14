import { render, screen, waitFor } from "@testing-library/react";
import FirstRunTimeline, {
  buildFirstRunTimeline,
  formatElapsed,
} from "../FirstRunTimeline";
import { getOnboardingEvents } from "../../../lib/api";

vi.mock("../../../lib/i18n", () => ({
  t: (k: string) => k,
  useT: () => 0,
}));

vi.mock("../../../lib/api", () => ({
  getOnboardingEvents: vi.fn(async () => []),
}));

const ev = (step: string, iso: string) => ({ step, created_at: iso });
const FULL = [
  ev("launch", "2026-07-14T10:00:00Z"),
  ev("mic_granted", "2026-07-14T10:00:12Z"),
  ev("first_transcript", "2026-07-14T10:00:47Z"),
  ev("ax_granted", "2026-07-14T10:01:38Z"),
  ev("first_insert", "2026-07-14T10:01:55Z"),
  ev("first_command", "2026-07-14T10:09:03Z"),
];

describe("buildFirstRunTimeline", () => {
  it("orders six fixed rows and computes elapsed seconds from launch", () => {
    const rows = buildFirstRunTimeline(FULL)!;
    expect(rows.map((r) => r.step)).toEqual([
      "launch",
      "mic_granted",
      "first_transcript",
      "ax_granted",
      "first_insert",
      "first_command",
    ]);
    expect(rows.map((r) => r.elapsedSecs)).toEqual([0, 12, 47, 98, 115, 543]);
  });

  it("marks targets: first_transcript ≤60s, first_insert ≤120s", () => {
    const rows = buildFirstRunTimeline(FULL)!;
    expect(rows.find((r) => r.step === "first_transcript")!.targetMet).toBe(true);
    expect(rows.find((r) => r.step === "first_insert")!.targetMet).toBe(true);
    expect(rows.find((r) => r.step === "launch")!.targetMet).toBeNull();

    const slow = buildFirstRunTimeline([
      ev("launch", "2026-07-14T10:00:00Z"),
      ev("first_transcript", "2026-07-14T10:01:15Z"),
    ])!;
    expect(slow.find((r) => r.step === "first_transcript")!.targetMet).toBe(false);
  });

  it("unreached steps are null/null; missing launch collapses to null", () => {
    const partial = buildFirstRunTimeline([ev("launch", "2026-07-14T10:00:00Z")])!;
    expect(partial.find((r) => r.step === "first_command")).toEqual({
      step: "first_command",
      elapsedSecs: null,
      targetMet: null,
    });
    expect(buildFirstRunTimeline([ev("mic_granted", "2026-07-14T10:00:12Z")])).toBeNull();
    expect(buildFirstRunTimeline([])).toBeNull();
  });
});

describe("formatElapsed", () => {
  it("renders m:ss", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(47)).toBe("0:47");
    expect(formatElapsed(115)).toBe("1:55");
    expect(formatElapsed(543)).toBe("9:03");
  });
});

describe("FirstRunTimeline card", () => {
  it("renders done and pending rows from fetched events", async () => {
    vi.mocked(getOnboardingEvents).mockResolvedValueOnce(FULL.slice(0, 5));
    render(<FirstRunTimeline />);
    await waitFor(() => expect(screen.getByTestId("firstrun-card")).toBeInTheDocument());
    expect(screen.getByTestId("firstrun-row-first_insert")).toHaveAttribute("data-state", "done");
    expect(screen.getByTestId("firstrun-row-first_command")).toHaveAttribute("data-state", "pending");
    expect(screen.getByTestId("firstrun-row-first_insert").textContent).toContain("1:55");
    expect(screen.getByTestId("firstrun-row-first_command").textContent).toContain("—");
  });

  it("shows the empty state when there is no launch record or fetch fails", async () => {
    vi.mocked(getOnboardingEvents).mockRejectedValueOnce(new Error("no table"));
    render(<FirstRunTimeline />);
    await waitFor(() => expect(screen.getByTestId("firstrun-empty")).toBeInTheDocument());
  });
});
