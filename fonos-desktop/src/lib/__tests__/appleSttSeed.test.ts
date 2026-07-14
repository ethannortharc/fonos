import { ensureAppleSttDefault, APPLE_STT_PROFILE_ID } from "../appleSttSeed";
import type { AppConfig, ModelProfile } from "../../types";

const cfg = (over: Partial<AppConfig> = {}): AppConfig =>
  ({ model_profiles: [], stt_profile: "", ...over }) as unknown as AppConfig;

describe("ensureAppleSttDefault", () => {
  it("does nothing off macOS", () => {
    expect(ensureAppleSttDefault(cfg(), false)).toBeNull();
  });

  it("does nothing when a default STT profile is already set", () => {
    expect(ensureAppleSttDefault(cfg({ stt_profile: "some-id" }), true)).toBeNull();
  });

  it("does nothing when any profile advertises the stt capability", () => {
    const p: ModelProfile = {
      id: "x", name: "x", provider: "openai", model: "whisper-1", capabilities: ["stt"],
    };
    expect(ensureAppleSttDefault(cfg({ model_profiles: [p] }), true)).toBeNull();
  });

  it("seeds the apple profile and default on a fresh macOS install", () => {
    const patch = ensureAppleSttDefault(cfg(), true);
    expect(patch).not.toBeNull();
    expect(patch!.stt_profile).toBe(APPLE_STT_PROFILE_ID);
    expect(patch!.model_profiles).toHaveLength(1);
    const p = patch!.model_profiles![0];
    expect(p.provider).toBe("apple");
    expect(p.model).toBe("apple-speech");
    expect(p.capabilities).toEqual(["stt"]);
    // Never touches the onboarding flag.
    expect("has_completed_onboarding" in patch!).toBe(false);
  });

  it("reuses an existing apple profile instead of duplicating", () => {
    const weird: ModelProfile = {
      id: "old-apple", name: "Apple", provider: "apple", model: "apple-speech",
    }; // no stt capability and no default → isSttConfigured is false
    const patch = ensureAppleSttDefault(cfg({ model_profiles: [weird] }), true);
    expect(patch).toEqual({ stt_profile: "old-apple" });
  });
});
