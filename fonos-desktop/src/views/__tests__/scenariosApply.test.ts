// buildUpdates is pure over (existing profiles, source, specs) — no mocks
// needed. Covers Ruling 1 (P3 Task 7 fix round 1): partial specs must omit
// the role-default keys they didn't touch instead of writing "" over them,
// since the backend's save_config merges by key (see
// src-tauri/src/commands/config.rs — absent keys keep their current value).

import { buildUpdates } from "../Scenarios";
import type { ProfileSpec, RoleKey } from "../Scenarios";
import type { ModelProfile } from "../../types";

const spec = (over: Partial<ProfileSpec> = {}): ProfileSpec => ({
  provider: "openai",
  base_url: "https://api.openai.com",
  model: "gpt-4o-mini",
  api_key: "sk-x",
  capabilities: ["llm"],
  ...over,
});

describe("buildUpdates", () => {
  it("full bundle sets all five role-default fields", () => {
    const specs: { role: RoleKey; spec: ProfileSpec }[] = [
      { role: "stt", spec: spec({ model: "whisper-1", capabilities: ["stt"], stt_api: "whisper" }) },
      { role: "llm", spec: spec({ model: "gpt-4o-mini", capabilities: ["llm"] }) },
      { role: "conv", spec: spec({ model: "gpt-4o-mini-tts", capabilities: ["tts"] }) },
      { role: "listen", spec: spec({ model: "gpt-4o-mini-tts", capabilities: ["tts"] }) },
    ];
    const updates = buildUpdates([], "openai", specs);
    expect(updates.stt_profile).toBeTruthy();
    expect(updates.llm_profile).toBeTruthy();
    expect(updates.tts_profile).toBeTruthy();
    expect(updates.sts_voice_profile).toBeTruthy();
    expect(updates.listen_voice_profile).toBeTruthy();
    expect(updates.has_completed_onboarding).toBe(true);
  });

  it("LLM-only specs (e.g. Anthropic apply) omit the other role keys instead of clearing them", () => {
    const specs: { role: RoleKey; spec: ProfileSpec }[] = [
      { role: "llm", spec: spec({ provider: "anthropic", model: "claude-sonnet-4-5", capabilities: ["llm"] }) },
    ];
    const updates = buildUpdates([], "anthropic", specs);
    expect(updates.llm_profile).toBeTruthy();
    expect(updates).not.toHaveProperty("stt_profile");
    expect(updates).not.toHaveProperty("tts_profile");
    expect(updates).not.toHaveProperty("sts_voice_profile");
    expect(updates).not.toHaveProperty("listen_voice_profile");
    expect(updates.has_completed_onboarding).toBe(true);
  });

  it("empty specs only set has_completed_onboarding (plus the untouched profile pool)", () => {
    const updates = buildUpdates([], "cloud", []);
    expect(updates).not.toHaveProperty("stt_profile");
    expect(updates).not.toHaveProperty("llm_profile");
    expect(updates).not.toHaveProperty("tts_profile");
    expect(updates).not.toHaveProperty("sts_voice_profile");
    expect(updates).not.toHaveProperty("listen_voice_profile");
    expect(updates.has_completed_onboarding).toBe(true);
    expect(updates.model_profiles).toEqual([]);
  });

  it("reuses an existing profile by base_url::model instead of duplicating", () => {
    const existing: ModelProfile[] = [
      {
        id: "existing-1",
        name: "gpt-4o-mini",
        provider: "openai",
        base_url: "https://api.openai.com",
        model: "gpt-4o-mini",
        capabilities: ["llm"],
      },
    ];
    const specs: { role: RoleKey; spec: ProfileSpec }[] = [
      { role: "llm", spec: spec({ model: "gpt-4o-mini", capabilities: ["llm"] }) },
    ];
    const updates = buildUpdates(existing, "openai", specs);
    expect(updates.model_profiles).toHaveLength(1);
    expect(updates.llm_profile).toBe("existing-1");
  });
});
