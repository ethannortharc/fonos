import {
  buildSetupPlan,
  suggestDowngrade,
  TIER_PULLS,
  type EngineDetection,
} from "../engineSetup";

const det = (running: boolean, installed: boolean): EngineDetection => ({
  engine: "ollama",
  running,
  installed,
  url: "http://localhost:11434",
});

describe("suggestDowngrade", () => {
  it("steps max→balanced→light→null", () => {
    expect(suggestDowngrade("max")).toBe("balanced");
    expect(suggestDowngrade("balanced")).toBe("light");
    expect(suggestDowngrade("light")).toBeNull();
  });
});

describe("buildSetupPlan (ollama)", () => {
  it("not installed → install+start+pull, disk-checked", () => {
    const b = buildSetupPlan(det(false, false), "balanced", 500_000_000, "ollama");
    expect(b.plan).toEqual({
      engine: "ollama",
      install: true,
      start: true,
      pulls: [TIER_PULLS.balanced.model],
      base_url: "http://localhost:11434",
    });
    expect(b.diskOk).toBe(true);
    expect(b.rows.some((r) => r.kind === "install")).toBe(true);
    expect(b.rows.some((r) => r.kind === "pull")).toBe(true);
  });

  it("installed but not running → start only, still pulls", () => {
    const b = buildSetupPlan(det(false, true), "light", 500_000_000, "ollama");
    expect(b.plan.install).toBe(false);
    expect(b.plan.start).toBe(true);
    expect(b.plan.pulls).toEqual([TIER_PULLS.light.model]);
  });

  it("running → neither install nor start, pull is idempotent", () => {
    const b = buildSetupPlan(det(true, true), "max", 500_000_000, "ollama");
    expect(b.plan.install).toBe(false);
    expect(b.plan.start).toBe(false);
    expect(b.plan.pulls).toEqual([TIER_PULLS.max.model]);
  });

  it("insufficient disk → diskOk false with a downgrade suggestion", () => {
    // max needs ~18.6 GB; give it 5 GB.
    const b = buildSetupPlan(det(true, true), "max", 5_000_000, "ollama");
    expect(b.diskOk).toBe(false);
    expect(b.downgrade).toBe("balanced");
  });
});

describe("buildSetupPlan (omlx)", () => {
  it("no pulls — models load on demand; brew install when missing", () => {
    const d: EngineDetection = { engine: "omlx", running: false, installed: false, url: "http://localhost:8000" };
    const b = buildSetupPlan(d, "max", 500_000_000, "omlx");
    expect(b.plan.pulls).toEqual([]);
    expect(b.plan.install).toBe(true);
    expect(b.rows.some((r) => r.kind === "note")).toBe(true);
    expect(b.diskOk).toBe(true);
  });
});

describe("buildSetupPlan (manual engines)", () => {
  it("lmstudio never auto-installs; uninstalled yields a manual row", () => {
    const d: EngineDetection = { engine: "lmstudio", running: false, installed: false, url: "http://localhost:1234" };
    const b = buildSetupPlan(d, "balanced", 500_000_000, "lmstudio");
    expect(b.plan.install).toBe(false);
    expect(b.plan.start).toBe(false);
    expect(b.plan.pulls).toEqual([]);
    expect(b.rows.some((r) => r.kind === "manual")).toBe(true);
  });
});
