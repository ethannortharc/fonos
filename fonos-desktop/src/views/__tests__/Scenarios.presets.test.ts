// Provider-preset invariants (pure constants — no render/mocks). Covers the
// Cerebras cloud preset (R1), the live-verified OpenRouter free default (R2),
// the Fireworks STT prefill + role-coverage metadata (R4), the Custom entries
// (R3), and the ProviderKey ⇄ CLOUD_PROVIDER_KEYS parity that keeps the derived
// union in sync with the runtime array.

import {
  CLOUD_PROVIDERS,
  CLOUD_PROVIDER_KEYS,
  KEY_PROVIDERS,
  OPENROUTER_FREE_LLM,
} from "../Scenarios";

const byKey = (k: string) => CLOUD_PROVIDERS.find((p) => p.key === k);

describe("Scenarios · provider presets", () => {
  it("derives ProviderKey from the same array the UI iterates (parity both ways)", () => {
    const defKeys = CLOUD_PROVIDERS.map((p) => p.key).sort();
    const unionKeys = [...CLOUD_PROVIDER_KEYS].sort();
    expect(defKeys).toEqual(unionKeys);
    // no duplicate keys
    expect(new Set(defKeys).size).toBe(defKeys.length);
  });

  it("adds the Cerebras cloud preset — OpenAI-compatible LLM-only bundle (R1)", () => {
    expect(CLOUD_PROVIDER_KEYS).toContain("cerebras");
    const cer = byKey("cerebras");
    expect(cer).toBeDefined();
    expect(cer!.name).toBe("Cerebras");
    expect(cer!.baseUrl).toBe("https://api.cerebras.ai/v1");
    expect(cer!.bundle.llm).toBe("qwen-3-32b");
    // LLM-only: no real cloud STT/TTS model, Apple STT fallback on macOS.
    expect(cer!.bundle.stt ?? null).toBeNull();
    expect(cer!.bundle.tts ?? null).toBeNull();
    expect(cer!.bundle.sttApple).toBe(true);
    // Cerebras needs a key (shows the preview chip).
    expect(KEY_PROVIDERS.has("cerebras")).toBe(true);
  });

  it("uses the live-verified OpenRouter free LLM default (R2)", () => {
    expect(OPENROUTER_FREE_LLM).toBe("qwen/qwen3-next-80b-a3b-instruct:free");
    expect(OPENROUTER_FREE_LLM.endsWith(":free")).toBe(true);
  });

  it("keeps the Fireworks STT prefilled and marks TTS genuinely absent (R4)", () => {
    const fw = byKey("fireworks");
    expect(fw!.bundle.stt?.model).toBe("whisper-v3-turbo");
    expect(fw!.bundle.stt?.stt_api).toBe("whisper");
    expect(fw!.bundle.tts).toBeNull();
  });

  it("marks TTS null on every LLM-only cloud provider (drives the no-tts hint)", () => {
    for (const k of ["openrouter", "anthropic", "google", "cerebras"]) {
      expect(byKey(k)!.bundle.tts).toBeNull();
    }
    // OpenAI is 3/3 — TTS stays a real model, not a null marker.
    expect(byKey("openai")!.bundle.tts).toBe("gpt-4o-mini-tts");
  });

  it("adds the Custom cloud entry — empty base URL, empty bundle, not key-gated (R3)", () => {
    expect(CLOUD_PROVIDER_KEYS).toContain("custom");
    const custom = byKey("custom");
    expect(custom).toBeDefined();
    expect(custom!.name).toBe("Custom");
    expect(custom!.baseUrl).toBe("");
    expect(custom!.bundle).toEqual({});
    // Keyless LAN servers are valid — no "needs key" preview chip for custom.
    expect(KEY_PROVIDERS.has("custom")).toBe(false);
  });
});
