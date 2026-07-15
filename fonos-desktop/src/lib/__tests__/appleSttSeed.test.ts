import { ensureAppleSttDefault, APPLE_STT_PROFILE_ID } from "../appleSttSeed";
import type { AppConfig, ModelProfile } from "../../types";

const cfg = (over: Partial<AppConfig> = {}): AppConfig =>
  ({ model_profiles: [], stt_profile: "", ...over }) as unknown as AppConfig;

// Fix A: the "is STT usable?" rule moved to fonos-core
// (services::is_stt_effectively_configured, covered by Rust tests). The seed no
// longer derives it — the caller passes `alreadyConfigured` (from the backend
// `sttConfigured` command), so these tests only pin the seed's own behavior.
describe("ensureAppleSttDefault", () => {
  it("does nothing off macOS", () => {
    expect(ensureAppleSttDefault(cfg(), false, false)).toBeNull();
  });

  it("does nothing when STT is already configured (caller's runtime-backed flag)", () => {
    const p: ModelProfile = { id: "some-id", name: "x", provider: "openai", model: "whisper-1" };
    expect(
      ensureAppleSttDefault(cfg({ stt_profile: "some-id", model_profiles: [p] }), true, true)
    ).toBeNull();
  });

  it("seeds the apple profile and default on a fresh macOS install", () => {
    const patch = ensureAppleSttDefault(cfg(), true, false);
    expect(patch).not.toBeNull();
    expect(patch!.stt_profile).toBe(APPLE_STT_PROFILE_ID);
    expect(patch!.model_profiles).toHaveLength(1);
    const p = patch!.model_profiles![0];
    expect(p.provider).toBe("apple");
    expect(p.model).toBe("apple-speech");
    expect(p.name).toBe("Apple Speech (on-device first)");
    expect(p.capabilities).toEqual(["stt"]);
    // Never touches the onboarding flag.
    expect("has_completed_onboarding" in patch!).toBe(false);
  });

  it("reuses an existing apple profile instead of duplicating", () => {
    const weird: ModelProfile = {
      id: "old-apple", name: "Apple", provider: "apple", model: "apple-speech",
    };
    const patch = ensureAppleSttDefault(cfg({ model_profiles: [weird] }), true, false);
    expect(patch).toEqual({ stt_profile: "old-apple" });
  });
});
