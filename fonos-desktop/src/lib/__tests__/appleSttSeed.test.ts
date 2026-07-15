import { ensureAppleSttDefault, APPLE_STT_PROFILE_ID } from "../appleSttSeed";
import { isSttConfigured } from "../../views/Scenarios";
import type { AppConfig, ModelProfile } from "../../types";

const cfg = (over: Partial<AppConfig> = {}): AppConfig =>
  ({ model_profiles: [], stt_profile: "", ...over }) as unknown as AppConfig;

describe("ensureAppleSttDefault", () => {
  it("does nothing off macOS", () => {
    expect(ensureAppleSttDefault(cfg(), false)).toBeNull();
  });

  it("does nothing when the default STT profile references an existing model profile", () => {
    // Fix 3: "configured" now requires stt_profile to point at a real profile.
    const p: ModelProfile = { id: "some-id", name: "x", provider: "openai", model: "whisper-1" };
    expect(ensureAppleSttDefault(cfg({ stt_profile: "some-id", model_profiles: [p] }), true)).toBeNull();
  });

  it("SEEDS when a profile advertises stt but is not the assigned default (Fix 3 spec change: capability alone is unusable — resolve_service reads only stt_profile)", () => {
    const p: ModelProfile = {
      id: "x", name: "x", provider: "openai", model: "whisper-1", capabilities: ["stt"],
    };
    const patch = ensureAppleSttDefault(cfg({ model_profiles: [p] }), true);
    expect(patch).not.toBeNull();
    expect(patch!.stt_profile).toBe(APPLE_STT_PROFILE_ID);
  });

  it("SEEDS when stt_profile points at a since-deleted profile (Fix 3 spec change: a dangling id resolves to an empty STT service)", () => {
    const patch = ensureAppleSttDefault(cfg({ stt_profile: "ghost" }), true);
    expect(patch).not.toBeNull();
    expect(patch!.stt_profile).toBe(APPLE_STT_PROFILE_ID);
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

// Fix 3: isSttConfigured is the runtime-backed gate — fonos-core's
// resolve_service("stt") reads ONLY config.stt_profile, so "configured" means a
// non-empty stt_profile that still points at an existing profile (matched by
// id). Capability advertising and dangling ids are explicitly NOT enough.
describe("isSttConfigured (Fix 3 runtime-backed semantic)", () => {
  it("false when no default STT profile is set", () => {
    expect(isSttConfigured(cfg())).toBe(false);
  });

  it("false when stt_profile points at a profile that no longer exists", () => {
    expect(isSttConfigured(cfg({ stt_profile: "ghost" }))).toBe(false);
  });

  it("false for an stt-capable profile that isn't assigned as the default", () => {
    const p: ModelProfile = {
      id: "x", name: "x", provider: "openai", model: "whisper-1", capabilities: ["stt"],
    };
    expect(isSttConfigured(cfg({ model_profiles: [p] }))).toBe(false);
  });

  it("true when stt_profile references an existing profile — even one without a capabilities array", () => {
    const p: ModelProfile = { id: "keep", name: "x", provider: "apple", model: "apple-speech" };
    expect(isSttConfigured(cfg({ stt_profile: "keep", model_profiles: [p] }))).toBe(true);
  });
});
