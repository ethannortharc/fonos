// Silent Apple-STT default for macOS first runs (onboarding P1, spec §P1-2):
// the playground must transcribe with zero configuration, so when no STT is
// configured we seed the same profile the Zero card's Apply would create.
// Pure function — returns the config patch to persist, or null when nothing
// should change. Never touches has_completed_onboarding.

import type { AppConfig, ModelProfile } from "../types";

/** Same id base the Zero card's Apply mints, so the two paths converge on
 *  one profile instead of duplicating. */
export const APPLE_STT_PROFILE_ID = "scenario-apple-stt";

/** Config patch that makes Apple Speech the default STT, or null when not on
 *  macOS / STT is already configured. `alreadyConfigured` is the caller's
 *  runtime-backed answer to "will dictation already transcribe?" (the backend
 *  `sttConfigured` command) — the seed no longer re-derives it here, keeping the
 *  one STT-gate rule in fonos-core so it can't drift from the pipeline. */
export function ensureAppleSttDefault(
  cfg: AppConfig,
  isMac: boolean,
  alreadyConfigured: boolean
): Partial<AppConfig> | null {
  if (!isMac) return null;
  if (alreadyConfigured) return null;
  const existing = cfg.model_profiles ?? [];
  const apple = existing.find(
    (p) => p.provider === "apple" && p.model === "apple-speech"
  );
  if (apple) return { stt_profile: apple.id };
  const profile: ModelProfile = {
    id: APPLE_STT_PROFILE_ID,
    name: "Apple Speech (on-device first)",
    provider: "apple",
    model: "apple-speech",
    capabilities: ["stt"],
  };
  return { model_profiles: [...existing, profile], stt_profile: profile.id };
}
