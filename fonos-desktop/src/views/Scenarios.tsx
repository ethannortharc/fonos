// Scenario-based setup (issue #29) — replaces the first-run wizard and also
// mounts as an overlay from Settings › Models for switching configurations.
//
// Three plain-language scenario cards (Local / Cloud / Zero-cost). Selecting a
// card expands step 2 on the same screen: a base-URL + key row, a Probe button,
// a probe-result banner, and an auto-assigned role→model plan whose rows are
// swappable, then an amber Apply button that writes ordinary model profiles and
// default assignments (never overwriting existing profiles). A "Saved setups"
// section lists switchable bundles with apply / delete / import / export.

import { useCallback, useEffect, useRef, useState } from "react";
import {
  checkDiskSpace,
  detectHardware,
  engineDetect,
  getConfig,
  saveConfig,
  scenarioProbe,
} from "../lib/api";
import type {
  AppConfig,
  EngineDetection,
  HardwareInfo,
  ModelProfile,
  ScenarioProbe,
} from "../types";
import { buildSetupPlan, type BuiltPlan, type EngineKey, type Tier } from "../lib/engineSetup";
import EngineSetupReview from "../components/EngineSetupReview";
import { t, td, useT, type TKey } from "../lib/i18n";
import { isMacOS } from "../lib/platform";

export const errStr = (e: unknown) => (e instanceof Error ? e.message : String(e));

// Shared control styling for every input/select in the scenario flow —
// comfortable padding, rounded-lg, amber focus border. Append `font-mono` and
// width utilities (`w-full`, `flex-1 min-w-0`) at the call site. Exported so the
// Settings › Scenarios tab reuses the same visual language.
export const control =
  "bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.06)] rounded-lg px-2.5 py-1.5 text-[11px] text-[#fafaf9] focus:outline-none focus:border-[rgba(242,184,75,0.35)] transition-colors";

// The former `isSttConfigured` lived here; it moved into fonos-core (Fix A) as
// `services::is_stt_effectively_configured`, exposed via the `stt_configured`
// command and read through `api.sttConfigured()`. Keeping the STT gate on the
// Rust side means it can't drift from the dictation pipeline's own resolution.

// ── scenario / engine / provider definitions ────────────────────────────────

type ScenarioKey = "local" | "cloud" | "zero";
// EngineKey is imported from ../lib/engineSetup (single source of truth shared
// with buildSetupPlan / EngineSetupReview). "custom" is a frontend-only local
// tile (R3) — a manual LAN/base-URL entry with no detection, no install/start
// orchestration, and no EngineSpec on the Rust side — so it widens the local
// key here without touching engineSetup's backend-mirroring EngineKey.
type LocalKey = EngineKey | "custom";
// Single source of truth for the provider-key union — every CLOUD_PROVIDERS
// entry's `key` must appear here (a Scenarios test enforces parity). Exported so
// tests can assert cerebras/custom presence + ProviderKey derivation.
export const CLOUD_PROVIDER_KEYS = [
  "openai",
  "openrouter",
  "anthropic",
  "google",
  "fireworks",
  "cerebras",
  "custom",
] as const;
type ProviderKey = (typeof CLOUD_PROVIDER_KEYS)[number];

interface EngineDef {
  key: LocalKey;
  name: string;
  url: string;
  /** Full pipeline (STT+LLM+TTS probing). False = LLM-only server. */
  full: boolean;
}

const LOCAL_ENGINES: EngineDef[] = [
  { key: "omlx", name: "OMLX", url: "http://localhost:8000", full: true },
  { key: "lmstudio", name: "LM Studio", url: "http://localhost:1234", full: false },
  { key: "ollama", name: "Ollama", url: "http://localhost:11434", full: false },
  { key: "vllm", name: "vLLM", url: "http://localhost:8000", full: true },
  // Manual entry: user types the LAN/base URL; probed as a potential full
  // pipeline. No detection badge (shows a muted "manual" line) and no CTA.
  { key: "custom", name: "Custom", url: "", full: true },
];

// Detection-evidence token → i18n label. Backend tokens ("path"/"app"/
// "process"/"port") are mapped here so the local step can show *why* an engine
// reads installed/running under the state badge.
const EVIDENCE_KEY: Record<string, TKey> = {
  path: "scen.evidence.path",
  app: "scen.evidence.app",
  process: "scen.evidence.process",
  port: "scen.evidence.port",
};

// Provider label for created local profiles. omlx/ollama skip the LLM api-key
// requirement, which keyless local servers need; base_url is always set
// explicitly so the provider's default URL is never used.
const ENGINE_PROVIDER: Record<LocalKey, string> = {
  omlx: "omlx",
  vllm: "omlx",
  ollama: "ollama",
  lmstudio: "omlx",
  // A custom LAN/base-URL server: keep the api-key requirement (some gateways
  // need one) and route through fonos-core's OpenAI-compatible fallthrough.
  custom: "custom",
};

interface CloudBundle {
  stt?: { model: string; stt_api: "whisper" | "chat" };
  sttApple?: boolean;
  llm?: string;
  // Role-coverage metadata (R4): a real model id = prefilled default; `null` =
  // the provider genuinely lacks this role (drives an explanatory placeholder);
  // absent/`undefined` = unknown (e.g. Custom, where the user types anything).
  tts?: string | null;
}
interface ProviderDef {
  key: ProviderKey;
  name: string;
  baseUrl: string;
  bundle: CloudBundle;
}

// Exported so tests can assert provider presence + ProviderKey/keys parity.
export const CLOUD_PROVIDERS: ProviderDef[] = [
  {
    key: "openai",
    name: "OpenAI",
    baseUrl: "https://api.openai.com",
    bundle: {
      stt: { model: "gpt-4o-mini-transcribe", stt_api: "whisper" },
      // GPT-5.6 family's budget tier (GA 2026-07-09) — newest generation at
      // roughly the old mini price, much stronger for polish/translate.
      llm: "gpt-5.6-luna",
      tts: "gpt-4o-mini-tts",
    },
  },
  {
    key: "openrouter",
    name: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    // Current flash generation: fast, cheap, strong multilingual — the right
    // profile for the polish/translate/summarize roles a keyed OR user runs.
    bundle: { llm: "google/gemini-3.5-flash", sttApple: true, tts: null },
  },
  {
    key: "anthropic",
    name: "Anthropic",
    baseUrl: "https://api.anthropic.com",
    bundle: { llm: "claude-sonnet-5", sttApple: true, tts: null },
  },
  {
    key: "google",
    name: "Google",
    baseUrl: "https://generativelanguage.googleapis.com",
    bundle: { llm: "gemini-3.5-flash", sttApple: true, tts: null },
  },
  {
    key: "fireworks",
    name: "Fireworks",
    baseUrl: "https://api.fireworks.ai/inference/v1",
    bundle: {
      // Fireworks serves an OpenAI-compatible Whisper endpoint; "whisper-v3-turbo"
      // is their documented id — the row stays editable if it drifts.
      stt: { model: "whisper-v3-turbo", stt_api: "whisper" },
      // Kimi K2.6 — Fireworks' current default general model ("p" is their
      // point-release id convention: k2p6 == K2.6).
      llm: "accounts/fireworks/models/kimi-k2p6",
      tts: null, // no Fireworks TTS today → no-tts hint on the voice rows
    },
  },
  {
    // LLM-only. Cerebras is OpenAI-compatible, so the generic fallthrough in
    // fonos-core (llm.rs → call_openai_compatible; not caught by the
    // anthropic/google branches) drives it — no Rust change needed.
    // "gemma-4-31b" is their documented current Gemma (verified against
    // inference-docs.cerebras.ai 2026-07-16); the row stays editable so
    // model drift is user-fixable. macOS keeps the Apple STT fallback.
    key: "cerebras",
    name: "Cerebras",
    baseUrl: "https://api.cerebras.ai/v1",
    bundle: { llm: "gemma-4-31b", sttApple: true, tts: null },
  },
  {
    // Any OpenAI-compatible endpoint (self-hosted, LAN gateway, a provider we
    // don't preset). No roles preset — every row is an editable input with the
    // "type anything" hint; the in-card base-URL field is where the user points
    // it. The apply gate is relaxed for custom (baseUrl + ≥1 row, no key) since
    // LAN servers are often keyless — see canApply.
    key: "custom",
    name: "Custom",
    baseUrl: "",
    bundle: {},
  },
];

// live-verified against openrouter.ai/api/v1/models on 2026-07-16; rows stay editable if the pool rotates.
// Gemma 4 31B — the strongest general/multilingual model in the current free
// pool (same model Cerebras serves as its default here).
export const OPENROUTER_FREE_LLM = "google/gemma-4-31b-it:free";

/** Placeholder i18n key for a cloud plan row, by provider bundle + role
 *  (R4 role-coverage). PRINCIPLE: prefill is a default, never a lock — every
 *  row is an editable input; the placeholder just explains *why* a role is
 *  blank so the user knows where to point it instead. A real model id → the
 *  generic model-id hint; an explicit `null` → the role is genuinely absent
 *  (no-tts / off-macOS no-stt); absent metadata → Custom, type anything. */
export function cloudRowPlaceholder(b: CloudBundle, role: RoleKey): TKey {
  if (role === "conv" || role === "listen") {
    if (b.tts === null) return "scen.cloud.ph.no-tts";
    return b.tts ? "scen.cloud.row.ph" : "scen.cloud.ph.custom";
  }
  if (role === "stt") {
    if (b.stt) return "scen.cloud.row.ph";
    // No real cloud STT model: sttApple providers explain the on-device / local
    // route; a bare bundle (Custom) just invites a model id.
    return b.sttApple ? "scen.cloud.ph.no-stt" : "scen.cloud.ph.custom";
  }
  return b.llm ? "scen.cloud.row.ph" : "scen.cloud.ph.custom";
}

const CARD_META: Record<
  ScenarioKey,
  { icon: string; nameKey: TKey; descKey: TKey; privacy: number; speed: number; cost: "free" | "metered"; reqKey: TKey }
> = {
  local: { icon: "🏠", nameKey: "scen.local.name", descKey: "scen.local.desc", privacy: 3, speed: 2, cost: "free", reqKey: "scen.local.req" },
  cloud: { icon: "⚡", nameKey: "scen.cloud.name", descKey: "scen.cloud.desc", privacy: 1, speed: 3, cost: "metered", reqKey: "scen.cloud.req" },
  zero: { icon: "🪶", nameKey: "scen.zero.name", descKey: "scen.zero.desc", privacy: 2, speed: 2, cost: "free", reqKey: "scen.zero.req" },
};

// ── profile-build helpers (Apply) ────────────────────────────────────────────

export interface ProfileSpec {
  provider: string;
  base_url: string;
  model: string;
  api_key: string;
  capabilities: string[];
  stt_api?: "whisper" | "chat";
}
export type RoleKey = "stt" | "llm" | "conv" | "listen";

const normUrl = (u: string) => u.replace(/\/+$/, "");

const appleSpec = (): ProfileSpec => ({
  provider: "apple",
  base_url: "",
  model: "apple-speech",
  api_key: "",
  capabilities: ["stt"],
});

function slugModel(model: string): string {
  const s = model.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  return (s || "model").slice(0, 40);
}

function uniqueId(base: string, taken: Set<string>): string {
  if (!taken.has(base)) return base;
  let i = 2;
  while (taken.has(`${base}-${i}`)) i += 1;
  return `${base}-${i}`;
}

/** Build the merged profile array + default-field updates for a set of desired
 *  role specs, reusing existing profiles by base_url+model and never mutating
 *  them. Returns a partial AppConfig to persist.
 *
 *  Only includes a role-default key when `specs` actually assigned that role
 *  — partial bundles (e.g. an Anthropic apply that's LLM-only) must never
 *  clear role defaults the specs didn't touch, since the backend's save_config
 *  merges by key (absent keys keep their current value; see
 *  src-tauri/src/commands/config.rs). A full bundle (all four roles present)
 *  still sets all five fields, same as before. */
export function buildUpdates(
  existing: ModelProfile[],
  source: string,
  specs: { role: RoleKey; spec: ProfileSpec }[]
): Partial<AppConfig> {
  const profiles: ModelProfile[] = [...existing];
  const takenIds = new Set(profiles.map((p) => p.id));
  const createdByKey = new Map<string, string>(); // dedup within this apply
  const roleToId: Partial<Record<RoleKey, string>> = {};

  const keyOf = (spec: ProfileSpec) =>
    spec.provider === "apple"
      ? "apple::apple-speech"
      : `${normUrl(spec.base_url)}::${spec.model}`;

  for (const { role, spec } of specs) {
    const key = keyOf(spec);

    // Already created in this apply → reuse and union capabilities.
    const created = createdByKey.get(key);
    if (created) {
      const p = profiles.find((x) => x.id === created);
      if (p) {
        const caps = new Set([...(p.capabilities ?? []), ...spec.capabilities]);
        p.capabilities = [...caps];
      }
      roleToId[role] = created;
      continue;
    }

    // Reuse an existing profile (non-destructive — don't touch its fields).
    const match = profiles.find((p) => {
      if (spec.provider === "apple") return p.provider === "apple" && p.model === "apple-speech";
      return normUrl(p.base_url ?? "") === normUrl(spec.base_url) && p.model === spec.model;
    });
    if (match) {
      roleToId[role] = match.id;
      createdByKey.set(key, match.id);
      continue;
    }

    // Create a fresh profile.
    const id = uniqueId(
      spec.provider === "apple" ? "scenario-apple-stt" : `scenario-${source}-${slugModel(spec.model)}`,
      takenIds
    );
    takenIds.add(id);
    const profile: ModelProfile = {
      id,
      name: spec.provider === "apple" ? "Apple Speech (on-device first)" : spec.model,
      provider: spec.provider,
      model: spec.model,
      capabilities: [...spec.capabilities],
    };
    if (spec.base_url) profile.base_url = spec.base_url;
    if (spec.api_key) profile.api_key = spec.api_key;
    if (spec.stt_api) profile.stt_api = spec.stt_api;
    profiles.push(profile);
    createdByKey.set(key, id);
    roleToId[role] = id;
  }

  const conv = roleToId.conv ?? roleToId.listen;
  const listen = roleToId.listen ?? roleToId.conv;
  const updates: Partial<AppConfig> = {
    model_profiles: profiles,
    has_completed_onboarding: true,
  };
  if (roleToId.stt) updates.stt_profile = roleToId.stt;
  if (roleToId.llm) updates.llm_profile = roleToId.llm;
  if (conv) {
    updates.tts_profile = conv;
    updates.sts_voice_profile = conv;
  }
  if (listen) updates.listen_voice_profile = listen;
  return updates;
}

// ── small presentational pieces ──────────────────────────────────────────────

function Dots({ n }: { n: number }) {
  return (
    <span className="tracking-[2px] text-[8px] font-mono">
      {[0, 1, 2].map((i) => (
        <span key={i} className={i < n ? "text-[var(--accent)]" : "text-[rgba(255,255,255,0.14)]"}>
          ●
        </span>
      ))}
    </span>
  );
}

// ── main component ───────────────────────────────────────────────────────────

export default function Scenarios({
  mode,
  onDone,
}: {
  mode: "fullscreen" | "overlay";
  onDone: () => void;
}) {
  useT();
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [selected, setSelected] = useState<ScenarioKey | null>(null);
  // Platform-optimal default: macOS ships the OMLX full pipeline; Linux has no
  // Apple STT, so Ollama (LLM-only + auto-installable) is the sensible start.
  const [engine, setEngine] = useState<LocalKey>(isMacOS ? "omlx" : "ollama");
  const [provider, setProvider] = useState<ProviderKey>("openai");
  const [detected, setDetected] = useState<Partial<Record<LocalKey, EngineDetection | null>>>({});
  const [hardware, setHardware] = useState<HardwareInfo | null>(null);
  const [diskKb, setDiskKb] = useState(0);
  const [tierOverride, setTierOverride] = useState<Tier | null>(null);
  const [review, setReview] = useState<BuiltPlan | null>(null);

  // Step 2 (local probe) state.
  const [baseUrl, setBaseUrl] = useState("http://localhost:8000");
  const [apiKey, setApiKey] = useState("");
  const [probing, setProbing] = useState(false);
  const [probe, setProbe] = useState<ScenarioProbe | null>(null);
  const [probeErr, setProbeErr] = useState("");
  const [planSel, setPlanSel] = useState<Record<RoleKey, string>>({ stt: "", llm: "", conv: "", listen: "" });

  // Cloud / zero key inputs.
  const [cloudKey, setCloudKey] = useState("");
  const [zeroKey, setZeroKey] = useState("");
  // Cloud plan rows — editable copies of the provider bundle (spec: 每步有缺省、逐项可改).
  const [cloudSel, setCloudSel] = useState<Record<RoleKey, string>>({ stt: "", llm: "", conv: "", listen: "" });
  // Editable endpoint — defaults to the provider's URL; hand-editing it IS the
  // "custom OpenAI-compatible endpoint" path (spec: 自定义 endpoint 就在卡内).
  const [cloudBaseUrl, setCloudBaseUrl] = useState(CLOUD_PROVIDERS[0].baseUrl);

  const [applying, setApplying] = useState(false);
  const [applied, setApplied] = useState(false);
  const [error, setError] = useState("");

  const reloadConfig = useCallback(async () => {
    try {
      const cfg = await getConfig();
      setConfig(cfg);
    } catch {
      /* non-Tauri / demo — leave null */
    }
  }, []);

  useEffect(() => {
    reloadConfig();
  }, [reloadConfig]);

  // Two-layer engine detection + hardware/disk facts on mount (best-effort).
  // `autoTakeoverDone` fires the running-engine take-over below at most once,
  // so it never fights a later, deliberate user engine choice.
  const autoTakeoverDone = useRef(false);
  useEffect(() => {
    let alive = true;
    setDetected(Object.fromEntries(LOCAL_ENGINES.map((e) => [e.key, null])));
    engineDetect()
      .then((list) => {
        if (!alive) return;
        setDetected(Object.fromEntries(list.map((d) => [d.engine, d])));
        // Running-engine auto-take-over (spec: 在跑的服务直接接管) — if the
        // currently-selected engine isn't running but another local engine
        // already is, switch to it via the normal engine-switch path (so
        // baseUrl etc. follow), once, on mount.
        if (!autoTakeoverDone.current) {
          autoTakeoverDone.current = true;
          const selectedDet = list.find((d) => d.engine === engine);
          if (!selectedDet?.running) {
            // Cross-reference against LOCAL_ENGINES (not the detection list
            // directly) so the match comes back typed as EngineKey rather
            // than EngineDetection's backend-mirroring `string`.
            const runningOther = LOCAL_ENGINES.find(
              (e) => e.key !== engine && list.find((d) => d.engine === e.key)?.running
            );
            if (runningOther) selectEngine(runningOther.key);
          }
        }
      })
      .catch(() => alive && setDetected({}));
    detectHardware().then((h) => alive && setHardware(h)).catch(() => {});
    checkDiskSpace().then((d) => alive && setDiskKb(d.available_kb)).catch(() => {});
    return () => {
      alive = false;
    };
    // Mount-only: intentionally ignores `engine`/`selectEngine` churn — the
    // ref above enforces the single-shot semantics, not the dep array.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const currentEngine = LOCAL_ENGINES.find((e) => e.key === engine)!;
  const currentProvider = CLOUD_PROVIDERS.find((p) => p.key === provider)!;

  // Select a card / engine / provider → reset step-2 state.
  const selectScenario = (s: ScenarioKey) => {
    setSelected(s);
    setProbe(null);
    setProbeErr("");
    setError("");
    if (s === "local") {
      setBaseUrl(currentEngine.url);
      setApiKey("");
    }
    if (s === "cloud") {
      setCloudSel(bundleToSel(provider));
      setCloudBaseUrl(currentProvider.baseUrl);
    }
  };
  const selectEngine = (k: LocalKey) => {
    setEngine(k);
    setProbe(null);
    setProbeErr("");
    setReview(null);
    setTierOverride(null);
    const e = LOCAL_ENGINES.find((x) => x.key === k)!;
    setBaseUrl(e.url);
    setApiKey("");
  };
  const bundleToSel = (key: ProviderKey): Record<RoleKey, string> => {
    const b = CLOUD_PROVIDERS.find((p) => p.key === key)!.bundle;
    return {
      stt: b.stt?.model ?? (b.sttApple && isMacOS ? "apple" : ""),
      llm: b.llm ?? "",
      conv: b.tts ?? "",
      listen: b.tts ?? "",
    };
  };
  const selectProvider = (k: ProviderKey) => {
    setProvider(k);
    setCloudSel(bundleToSel(k));
    setCloudBaseUrl(CLOUD_PROVIDERS.find((p) => p.key === k)!.baseUrl);
  };

  const runProbe = useCallback(async () => {
    setProbing(true);
    setProbeErr("");
    setError("");
    try {
      const result = await scenarioProbe(baseUrl, apiKey);
      setProbe(result);
      if (!result.reachable) {
        setProbeErr(t("scen.unreachable"));
        return;
      }
      const p = result.plan;
      setPlanSel({
        // Apple on-device Speech is macOS-only (the backend errors explicitly
        // off-platform) — never sentinel-fall to it on Linux, for full or
        // non-full engines alike (mirrors the cloud-branch guard below).
        stt: currentEngine.full ? p.stt ?? (isMacOS ? "apple" : "") : isMacOS ? "apple" : "",
        llm: p.llm ?? "",
        conv: p.conversation_tts ?? "",
        listen: p.listen_tts ?? "",
      });
    } catch (e) {
      setProbeErr(errStr(e));
    } finally {
      setProbing(false);
    }
  }, [baseUrl, apiKey, currentEngine.full]);

  // ── one-click setup: detection → review card → auto-probe ────────────────────
  // A user-chosen tier (from a disk/failure downgrade) wins; else the hardware's
  // recommendation; else the safe floor.
  const tier: Tier = tierOverride ?? hardware?.tier ?? "light";
  const openReview = (forTier: Tier) => {
    // Custom is manual — no install/start plan, no review card (the CTA never
    // renders for it). Guarding here also narrows `engine` to EngineKey for
    // buildSetupPlan, whose backend-mirroring type has no "custom".
    if (engine === "custom") return;
    const det = detected[engine];
    if (!det) return;
    setTierOverride(forTier);
    setReview(buildSetupPlan(det, forTier, diskKb, engine));
  };
  const reviewDone = () => {
    setReview(null);
    // Engine came up — re-detect and hand off to the normal probe flow.
    engineDetect()
      .then((list) => setDetected(Object.fromEntries(list.map((d) => [d.engine, d]))))
      .catch(() => {});
    runProbe();
  };

  // Build the desired role specs for the current selection.
  const buildSpecs = (): { source: string; specs: { role: RoleKey; spec: ProfileSpec }[] } | null => {
    if (selected === "local" && probe?.reachable) {
      const specs: { role: RoleKey; spec: ProfileSpec }[] = [];
      const local = (model: string, caps: string[], stt_api?: "whisper" | "chat"): ProfileSpec => ({
        provider: ENGINE_PROVIDER[engine],
        base_url: baseUrl,
        model,
        api_key: apiKey,
        capabilities: caps,
        stt_api,
      });
      if (planSel.stt === "apple") {
        if (isMacOS) specs.push({ role: "stt", spec: appleSpec() });
      } else if (planSel.stt) specs.push({ role: "stt", spec: local(planSel.stt, ["stt"], "whisper") });
      if (planSel.llm) specs.push({ role: "llm", spec: local(planSel.llm, ["llm"]) });
      if (planSel.conv) specs.push({ role: "conv", spec: local(planSel.conv, ["tts"]) });
      if (planSel.listen) specs.push({ role: "listen", spec: local(planSel.listen, ["tts"]) });
      return { source: engine, specs };
    }
    if (selected === "cloud") {
      const specs: { role: RoleKey; spec: ProfileSpec }[] = [];
      const cloud = (model: string, caps: string[], stt_api?: "whisper" | "chat"): ProfileSpec => ({
        provider: currentProvider.key,
        base_url: cloudBaseUrl.trim() || currentProvider.baseUrl,
        model,
        api_key: cloudKey,
        capabilities: caps,
        stt_api,
      });
      if (cloudSel.stt === "apple") {
        if (isMacOS) specs.push({ role: "stt", spec: appleSpec() });
      } else if (cloudSel.stt) {
        specs.push({ role: "stt", spec: cloud(cloudSel.stt, ["stt"], currentProvider.bundle.stt?.stt_api ?? "whisper") });
      }
      if (cloudSel.llm) specs.push({ role: "llm", spec: cloud(cloudSel.llm, ["llm"]) });
      if (cloudSel.conv) specs.push({ role: "conv", spec: cloud(cloudSel.conv, ["tts"]) });
      if (cloudSel.listen) specs.push({ role: "listen", spec: cloud(cloudSel.listen, ["tts"]) });
      return { source: currentProvider.key, specs };
    }
    if (selected === "zero") {
      const specs: { role: RoleKey; spec: ProfileSpec }[] = [{ role: "stt", spec: appleSpec() }];
      if (zeroKey.trim()) {
        specs.push({
          role: "llm",
          spec: {
            provider: "openrouter",
            base_url: "https://openrouter.ai/api/v1",
            model: OPENROUTER_FREE_LLM,
            api_key: zeroKey.trim(),
            capabilities: ["llm"],
          },
        });
      }
      return { source: "free", specs };
    }
    return null;
  };

  const canApply = (): boolean => {
    if (selected === "local") return !!probe?.reachable;
    if (selected === "cloud") {
      const hasRole = (Object.keys(cloudSel) as RoleKey[]).some(
        (r) => cloudSel[r].trim().length > 0
      );
      // Custom endpoints are often keyless LAN servers — require a base URL and
      // ≥1 model row instead of an API key (R3). Preset providers still gate on
      // the key.
      if (provider === "custom") return cloudBaseUrl.trim().length > 0 && hasRole;
      return cloudKey.trim().length > 0 && hasRole;
    }
    if (selected === "zero") return true;
    return false;
  };

  const apply = useCallback(async () => {
    const built = buildSpecs();
    if (!built) return;
    setApplying(true);
    setError("");
    try {
      const cfg = await getConfig().catch(() => config);
      const existing = (cfg?.model_profiles ?? []) as ModelProfile[];
      const updates = buildUpdates(existing, built.source, built.specs);
      await saveConfig(JSON.stringify(updates));
      setApplied(true);
      await reloadConfig();
    } catch (e) {
      setError(errStr(e));
    } finally {
      setApplying(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected, engine, baseUrl, apiKey, planSel, provider, cloudKey, cloudSel, cloudBaseUrl, zeroKey, probe, config, reloadConfig]);

  const skip = useCallback(async () => {
    try {
      await saveConfig(JSON.stringify({ has_completed_onboarding: true }));
    } catch {
      /* ignore */
    }
    onDone();
  }, [onDone]);

  // ── applied summary ─────────────────────────────────────────────────────────
  if (applied) {
    return (
      <Shell mode={mode} onClose={onDone}>
        <div className="flex flex-col items-center gap-4 py-10 text-center">
          <div className="w-14 h-14 rounded-full bg-[rgba(74,222,128,0.12)] flex items-center justify-center text-[#4ade80] text-[24px]">
            ✓
          </div>
          <div className="text-[16px] font-semibold text-[#fafaf9]">
            {t("scen.done.title")}
          </div>
          <p className="text-[12px] text-[rgba(255,255,255,0.5)] max-w-[380px]">{t("scen.done.desc")}</p>
          {error && <div className="text-[11px] text-[#f87171]">{error}</div>}
          <button
            onClick={onDone}
            className="mt-2 px-6 py-2 rounded-lg bg-[rgba(242,184,75,0.14)] border border-[rgba(242,184,75,0.35)] text-[var(--accent)] text-[12px] font-semibold hover:bg-[rgba(242,184,75,0.2)] transition-colors"
          >
            {mode === "fullscreen" ? t("scen.done.start") : t("scen.done.close")}
          </button>
        </div>
      </Shell>
    );
  }

  return (
    <Shell mode={mode} onClose={onDone}>
      {/* Header */}
      <div className="text-center mb-1">
        <div className="text-[18px] font-semibold text-[#fafaf9]">
          {mode === "overlay" ? t("scen.overlay.title") : t("scen.title")}
        </div>
        <div className="text-[11.5px] text-[rgba(255,255,255,0.45)] mt-1">{t("scen.subtitle")}</div>
      </div>

      {error && <div className="text-[11px] text-[#f87171] text-center">{error}</div>}

      {/* Cards */}
      <div
        className={[
          "grid grid-cols-1 gap-2.5 mt-4",
          isMacOS ? "sm:grid-cols-3" : "sm:grid-cols-2",
        ].join(" ")}
      >
        {(Object.keys(CARD_META) as ScenarioKey[])
          // Zero relies on Apple on-device Speech — macOS only (spec §P1
          // Linux 差异; the backend errors explicitly off-macOS anyway).
          .filter((key) => key !== "zero" || isMacOS)
          .map((key) => {
          const m = CARD_META[key];
          const active = selected === key;
          return (
            <button
              key={key}
              onClick={() => selectScenario(key)}
              className={[
                "relative text-left rounded-xl border p-4 flex flex-col gap-2.5 transition-colors",
                active
                  ? "border-[rgba(242,184,75,0.45)] bg-[rgba(242,184,75,0.04)]"
                  : "border-[rgba(255,255,255,0.07)] bg-[rgba(255,255,255,0.03)] hover:border-[rgba(242,184,75,0.3)]",
              ].join(" ")}
            >
              {active && (
                <span className="absolute top-3 right-3 w-4 h-4 rounded-full bg-[var(--accent)] text-[#1a1917] text-[10px] font-extrabold flex items-center justify-center">
                  ✓
                </span>
              )}
              <div className="w-9 h-9 rounded-[9px] bg-[rgba(242,184,75,0.12)] flex items-center justify-center text-[16px]">
                {m.icon}
              </div>
              <div className="text-[13px] font-semibold text-[#fafaf9]">{t(m.nameKey)}</div>
              <p className="text-[10.5px] text-[rgba(255,255,255,0.4)] leading-snug">{t(m.descKey)}</p>
              <div className="flex flex-col gap-1 mt-0.5">
                <div className="flex justify-between items-center text-[9.5px] text-[rgba(255,255,255,0.4)]">
                  <span>{t("scen.meter.privacy")}</span>
                  <Dots n={m.privacy} />
                </div>
                <div className="flex justify-between items-center text-[9.5px] text-[rgba(255,255,255,0.4)]">
                  <span>{t("scen.meter.speed")}</span>
                  <Dots n={m.speed} />
                </div>
                <div className="flex justify-between items-center text-[9.5px] text-[rgba(255,255,255,0.4)]">
                  <span>{t("scen.meter.cost")}</span>
                  {m.cost === "free" ? (
                    <span className="text-[#4ade80] text-[9.5px] font-semibold">{t("scen.cost.free")}</span>
                  ) : (
                    <span className="text-[rgba(255,255,255,0.55)] text-[9.5px]">{t("scen.cost.metered")}</span>
                  )}
                </div>
              </div>
              <div className="mt-auto pt-2 border-t border-dashed border-[rgba(255,255,255,0.08)] text-[9.5px] text-[rgba(255,255,255,0.4)]">
                {t(m.reqKey)}
              </div>
            </button>
          );
        })}
      </div>

      {/* Step 2 — expands under the cards */}
      {selected === "local" && (
        <LocalStep
          engine={engine}
          detected={detected}
          onEngine={selectEngine}
          baseUrl={baseUrl}
          apiKey={apiKey}
          setBaseUrl={setBaseUrl}
          setApiKey={setApiKey}
          probing={probing}
          probe={probe}
          probeErr={probeErr}
          onProbe={runProbe}
          planSel={planSel}
          setPlanSel={setPlanSel}
          full={currentEngine.full}
          onSetup={() => openReview(tier)}
          review={
            review && (
              <EngineSetupReview
                // Remount when the plan's engine/tier changes (e.g. a disk
                // downgrade) so the editable pull list reseeds from the new
                // plan; in-place edits keep the same key and persist.
                key={`${engine}-${tier}`}
                built={review}
                engineName={currentEngine.name}
                tier={tier}
                diskAvailableKb={diskKb}
                onRetier={openReview}
                onCancel={() => setReview(null)}
                onDone={reviewDone}
              />
            )
          }
        />
      )}

      {selected === "cloud" && (
        <CloudStep
          provider={provider}
          onProvider={selectProvider}
          cloudKey={cloudKey}
          setCloudKey={setCloudKey}
          cloudSel={cloudSel}
          setCloudSel={setCloudSel}
          baseUrl={cloudBaseUrl}
          setBaseUrl={setCloudBaseUrl}
        />
      )}

      {selected === "zero" && (
        <div className="mt-4 rounded-xl border border-[rgba(255,255,255,0.07)] bg-[rgba(255,255,255,0.02)] p-4 flex flex-col gap-3">
          <PlanRowStatic role="stt" value={t("scen.apple")} />
          <div className="flex flex-col gap-1.5">
            <span className="text-[10px] text-[rgba(255,255,255,0.4)]">{t("scen.optkey")}</span>
            <input
              value={zeroKey}
              onChange={(e) => setZeroKey(e.target.value)}
              placeholder={t("scen.apikey.ph")}
              className={`${control} font-mono w-full`}
            />
            {zeroKey.trim() ? (
              <PlanRowStatic role="llm" value={OPENROUTER_FREE_LLM} />
            ) : (
              <span className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("scen.optkey.note")}</span>
            )}
          </div>
        </div>
      )}

      {/* Apply bar */}
      {selected && (
        <div className="mt-3 flex items-center gap-3 flex-wrap">
          <button
            onClick={apply}
            disabled={!canApply() || applying}
            className="px-5 py-2 rounded-lg bg-[rgba(242,184,75,0.14)] border border-[rgba(242,184,75,0.35)] text-[var(--accent)] text-[12px] font-semibold hover:bg-[rgba(242,184,75,0.2)] transition-colors disabled:opacity-40"
          >
            {applying ? t("scen.applying") : t("scen.apply")}
          </button>
          <span className="text-[10px] text-[rgba(255,255,255,0.35)]">
            {canApply()
              ? t("scen.apply.note")
              : selected === "cloud"
              ? provider === "custom"
                ? t("scen.custom.needurl")
                : t("scen.needkey")
              : t("scen.apply.note")}
          </span>
        </div>
      )}

      {/* Skip (first-run only) */}
      {mode === "fullscreen" && (
        <div className="text-center mt-5">
          <button
            onClick={skip}
            className="text-[11px] text-[rgba(255,255,255,0.35)] underline underline-offset-2 hover:text-[rgba(255,255,255,0.6)] transition-colors"
          >
            {t("scen.skip")}
          </button>
        </div>
      )}
    </Shell>
  );
}

// ── shell (fullscreen vs overlay) ────────────────────────────────────────────

function Shell({
  mode,
  onClose,
  children,
}: {
  mode: "fullscreen" | "overlay";
  onClose: () => void;
  children: React.ReactNode;
}) {
  if (mode === "overlay") {
    return (
      <div className="fixed inset-0 z-50 bg-black/60 flex items-start justify-center overflow-y-auto py-10 px-4">
        <div className="relative w-full max-w-[760px] bg-[var(--bg)] border border-[rgba(255,255,255,0.09)] rounded-2xl shadow-2xl p-6">
          <button
            onClick={onClose}
            className="absolute top-4 right-4 w-7 h-7 rounded-lg flex items-center justify-center text-[rgba(255,255,255,0.4)] hover:bg-[rgba(255,255,255,0.06)] hover:text-[rgba(255,255,255,0.8)] transition-colors"
            aria-label="Close"
          >
            ✕
          </button>
          {children}
        </div>
      </div>
    );
  }
  return (
    <div className="fixed inset-0 z-50 bg-[var(--bg)] overflow-y-auto">
      <div
        className="h-[38px] w-full flex-shrink-0 bg-[#151413]"
        data-tauri-drag-region=""
      />
      <div className="max-w-[760px] mx-auto px-6 pb-16 pt-4">{children}</div>
    </div>
  );
}

// ── local step ───────────────────────────────────────────────────────────────

function LocalStep({
  engine,
  detected,
  onEngine,
  baseUrl,
  apiKey,
  setBaseUrl,
  setApiKey,
  probing,
  probe,
  probeErr,
  onProbe,
  planSel,
  setPlanSel,
  full,
  onSetup,
  review,
}: {
  engine: LocalKey;
  detected: Partial<Record<LocalKey, EngineDetection | null>>;
  onEngine: (k: LocalKey) => void;
  baseUrl: string;
  apiKey: string;
  setBaseUrl: (v: string) => void;
  setApiKey: (v: string) => void;
  probing: boolean;
  probe: ScenarioProbe | null;
  probeErr: string;
  onProbe: () => void;
  planSel: Record<RoleKey, string>;
  setPlanSel: (v: Record<RoleKey, string>) => void;
  full: boolean;
  onSetup: () => void;
  review: React.ReactNode;
}) {
  const set = (role: RoleKey, v: string) => setPlanSel({ ...planSel, [role]: v });
  const ttsCandidates = probe?.classified.tts ?? [];
  const noTts = full ? ttsCandidates.length === 0 : true;

  return (
    <div className="mt-4 rounded-xl border border-[rgba(255,255,255,0.07)] bg-[rgba(255,255,255,0.02)] p-4 flex flex-col gap-3">
      {/* Engine picker */}
      <div className="flex flex-col gap-1.5">
        <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{t("scen.engine")}</span>
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-1.5">
          {LOCAL_ENGINES.map((e) => {
            const on = e.key === engine;
            const det = detected[e.key];
            // Custom is manual — no detection three-state and no evidence line,
            // just a muted "manual" label (the URL/probe path drives it).
            const isCustom = e.key === "custom";
            return (
              <button
                key={e.key}
                onClick={() => onEngine(e.key)}
                className={[
                  "rounded-lg border px-2.5 py-2 text-left transition-colors",
                  on
                    ? "border-[rgba(242,184,75,0.4)] bg-[rgba(242,184,75,0.06)]"
                    : "border-[rgba(255,255,255,0.07)] hover:border-[rgba(255,255,255,0.15)]",
                ].join(" ")}
              >
                <div className={["text-[11px] font-medium", on ? "text-[var(--accent)]" : "text-[rgba(255,255,255,0.7)]"].join(" ")}>
                  {e.name}
                </div>
                <div className="text-[8.5px] mt-0.5">
                  {isCustom ? (
                    <span data-testid="engine-manual-custom" className="text-[rgba(255,255,255,0.3)]">{t("scen.manual")}</span>
                  ) : det === null || det === undefined ? (
                    <span className="text-[rgba(255,255,255,0.3)]">{t("scen.detecting")}</span>
                  ) : det.running ? (
                    <span className="text-[#4ade80]">{t("scen.detected")}</span>
                  ) : det.installed ? (
                    <span className="text-[var(--accent)]">{t("scen.installed.stopped")}</span>
                  ) : (
                    <span className="text-[rgba(255,255,255,0.25)]">{t("scen.notdetected")}</span>
                  )}
                </div>
                {!isCustom && det && det.evidence.length > 0 && (
                  <div
                    data-testid={`engine-evidence-${e.key}`}
                    className="text-[8px] mt-0.5 text-[rgba(255,255,255,0.3)] tabular-nums"
                  >
                    {det.evidence
                      .filter((ev) => EVIDENCE_KEY[ev])
                      .map((ev) => t(EVIDENCE_KEY[ev]))
                      .join(" · ")}
                  </div>
                )}
              </button>
            );
          })}
        </div>
      </div>

      {/* URL + key + probe */}
      <div className="flex items-end gap-2 flex-wrap">
        <label className="flex flex-col gap-1 flex-1 min-w-[200px]">
          <span className="text-[9px] text-[rgba(255,255,255,0.35)]">{t("scen.baseurl")}</span>
          <input
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            className={`${control} font-mono w-full`}
          />
        </label>
        <label className="flex flex-col gap-1 w-[168px]">
          <span className="text-[9px] text-[rgba(255,255,255,0.35)]">{t("scen.apikey")}</span>
          <input
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="—"
            className={`${control} font-mono w-full`}
          />
        </label>
        <button
          onClick={onProbe}
          disabled={probing}
          className="px-3.5 py-2 rounded-lg border border-[rgba(255,255,255,0.1)] bg-[rgba(255,255,255,0.04)] text-[11px] text-[rgba(255,255,255,0.7)] hover:text-[#fafaf9] transition-colors disabled:opacity-50"
        >
          {probing ? t("scen.probing") : probe ? t("scen.reprobe") : t("scen.probe")}
        </button>
      </div>

      {/* One-click setup CTA — only while the selected engine isn't running.
          Opens the pre-execution review card (rendered via the `review` slot). */}
      {(() => {
        // Custom has no install/start orchestration — never a CTA (the manual
        // URL + Probe path is the whole flow).
        if (engine === "custom") return null;
        const d = detected[engine];
        if (!d || d.running) return null;
        return (
          <div className="flex items-center gap-2 flex-wrap">
            <button
              data-testid="engine-setup-cta"
              onClick={onSetup}
              className="px-4 py-2 rounded-lg bg-[rgba(242,184,75,0.14)] border border-[rgba(242,184,75,0.35)] text-[var(--accent)] text-[11px] font-semibold hover:bg-[rgba(242,184,75,0.2)] transition-colors"
            >
              {d.installed ? t("scen.setup.start") : t("scen.setup.install")}
            </button>
            <span className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("scen.setup.note")}</span>
          </div>
        );
      })()}
      {review}

      {/* Banner */}
      {probeErr && (
        <div className="text-[11px] text-[#f87171] px-3 py-2 rounded-lg bg-[rgba(248,113,113,0.06)] border border-[rgba(248,113,113,0.15)]">
          {probeErr}
        </div>
      )}
      {probe?.reachable && !probeErr && (
        <div className="flex items-center gap-2 text-[11px] text-[#4ade80] px-3 py-2 rounded-lg bg-[rgba(74,222,128,0.06)] border border-[rgba(74,222,128,0.15)]">
          <span>✓</span>
          <span>
            {Object.keys(probe.tts_rtfs).length > 0
              ? td("scen.connected", [String(probe.models.length)])
              : td("scen.connected.nott", [String(probe.models.length)])}
          </span>
          <span className="ml-auto text-[10px] text-[rgba(255,255,255,0.35)] tabular-nums">{probe.latency_ms}ms</span>
        </div>
      )}

      {/* Plan rows */}
      {probe?.reachable && (
        <div className="rounded-lg border border-[rgba(255,255,255,0.06)] divide-y divide-[rgba(255,255,255,0.04)]">
          <PlanRowSelect
            role="stt"
            value={planSel.stt}
            options={[
              // Apple on-device Speech is macOS-only — don't offer it as a
              // choice off-macOS (mirrors the `key !== "zero" || isMacOS`
              // scenario-card filter above).
              ...(isMacOS ? [{ value: "apple", label: t("scen.apple") }] : []),
              ...(probe.classified.stt.map((m) => ({ value: m, label: m }))),
            ]}
            onChange={(v) => set("stt", v)}
          />
          <PlanRowSelect
            role="llm"
            value={planSel.llm}
            options={probe.classified.llm.map((m) => ({ value: m, label: m }))}
            onChange={(v) => set("llm", v)}
          />
          {!noTts && (
            <>
              <PlanRowSelect
                role="conv"
                value={planSel.conv}
                options={ttsCandidates.map((m) => ({ value: m, label: m }))}
                onChange={(v) => set("conv", v)}
                tag={
                  planSel.conv && probe.tts_rtfs[planSel.conv] !== undefined
                    ? { kind: "fast", text: td("scen.tag.fast", [probe.tts_rtfs[planSel.conv].toFixed(1)]) }
                    : undefined
                }
              />
              <PlanRowSelect
                role="listen"
                value={planSel.listen}
                options={ttsCandidates.map((m) => ({ value: m, label: m }))}
                onChange={(v) => set("listen", v)}
                tag={{ kind: "hq", text: t("scen.tag.hq") }}
              />
            </>
          )}
          {noTts && <PlanRowStatic role="conv" value={t("scen.unassigned")} note={t("scen.tts.note")} />}
        </div>
      )}
    </div>
  );
}

// ── cloud step ───────────────────────────────────────────────────────────────

function CloudStep({
  provider,
  onProvider,
  cloudKey,
  setCloudKey,
  cloudSel,
  setCloudSel,
  baseUrl,
  setBaseUrl,
}: {
  provider: ProviderKey;
  onProvider: (k: ProviderKey) => void;
  cloudKey: string;
  setCloudKey: (v: string) => void;
  cloudSel: Record<RoleKey, string>;
  setCloudSel: (v: Record<RoleKey, string>) => void;
  baseUrl: string;
  setBaseUrl: (v: string) => void;
}) {
  const set = (role: RoleKey, v: string) => setCloudSel({ ...cloudSel, [role]: v });
  const bundle = CLOUD_PROVIDERS.find((p) => p.key === provider)!.bundle;
  const ph = (role: RoleKey) => t(cloudRowPlaceholder(bundle, role));
  return (
    <div className="mt-4 rounded-xl border border-[rgba(255,255,255,0.07)] bg-[rgba(255,255,255,0.02)] p-4 flex flex-col gap-3">
      <div className="flex flex-col gap-1.5">
        <span className="text-[10px] uppercase tracking-wider text-[rgba(255,255,255,0.3)]">{t("scen.provider")}</span>
        <div className="grid grid-cols-2 sm:grid-cols-3 gap-1.5 max-w-[420px]">
          {CLOUD_PROVIDERS.map((p) => {
            const on = p.key === provider;
            return (
              <button
                key={p.key}
                onClick={() => onProvider(p.key)}
                className={[
                  "rounded-lg border px-3 py-2 text-[11px] font-medium transition-colors",
                  on
                    ? "border-[rgba(242,184,75,0.4)] bg-[rgba(242,184,75,0.06)] text-[var(--accent)]"
                    : "border-[rgba(255,255,255,0.07)] text-[rgba(255,255,255,0.7)] hover:border-[rgba(255,255,255,0.15)]",
                ].join(" ")}
              >
                {p.name}
              </button>
            );
          })}
        </div>
      </div>

      <div className="flex items-end gap-2 flex-wrap">
        <label className="flex flex-col gap-1 flex-1 min-w-[220px]">
          <span className="text-[9px] text-[rgba(255,255,255,0.35)]">{t("scen.baseurl")}</span>
          <input
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            className={`${control} font-mono w-full`}
            data-testid="cloud-base-url"
          />
        </label>
        <label className="flex flex-col gap-1 w-[200px]">
          <span className="text-[9px] text-[rgba(255,255,255,0.35)]">{t("scen.apikey")}</span>
          <input
            value={cloudKey}
            onChange={(e) => setCloudKey(e.target.value)}
            placeholder={t("scen.apikey.ph")}
            className={`${control} font-mono w-full`}
          />
        </label>
      </div>

      <div className="rounded-lg border border-[rgba(255,255,255,0.06)] divide-y divide-[rgba(255,255,255,0.04)]">
        {cloudSel.stt === "apple" ? (
          <PlanRowStatic role="stt" value={t("scen.apple")} />
        ) : (
          <PlanRowInput role="stt" value={cloudSel.stt} onChange={(v) => set("stt", v)} placeholder={ph("stt")} />
        )}
        <PlanRowInput role="llm" value={cloudSel.llm} onChange={(v) => set("llm", v)} placeholder={ph("llm")} />
        <PlanRowInput role="conv" value={cloudSel.conv} onChange={(v) => set("conv", v)} placeholder={ph("conv")} />
        <PlanRowInput role="listen" value={cloudSel.listen} onChange={(v) => set("listen", v)} placeholder={ph("listen")} />
      </div>
      <span className="text-[10px] text-[rgba(255,255,255,0.35)]">{t("scen.cloud.editable.note")}</span>
    </div>
  );
}

// ── plan rows ────────────────────────────────────────────────────────────────

export const ROLE_LABEL: Record<RoleKey, TKey> = {
  stt: "scen.role.stt",
  llm: "scen.role.llm",
  conv: "scen.role.conv",
  listen: "scen.role.listen",
};

function PlanRowSelect({
  role,
  value,
  options,
  onChange,
  tag,
}: {
  role: RoleKey;
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
  tag?: { kind: "fast" | "hq"; text: string };
}) {
  const selectedLabel = options.find((o) => o.value === value)?.label ?? value;
  return (
    <div className="flex items-center gap-2.5 px-3 py-2.5">
      <span className="w-[92px] flex-none text-[10.5px] text-[rgba(255,255,255,0.4)]">{t(ROLE_LABEL[role])}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        title={selectedLabel}
        className={`${control} font-mono flex-1 min-w-0 truncate`}
      >
        {options.length === 0 && <option value="">—</option>}
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      {tag && (
        <span
          className={[
            "flex-none text-[9px] px-1.5 py-0.5 rounded-full",
            tag.kind === "fast" ? "bg-[rgba(74,222,128,0.1)] text-[#4ade80]" : "bg-[rgba(192,132,252,0.12)] text-[#c084fc]",
          ].join(" ")}
        >
          {tag.text}
        </span>
      )}
    </div>
  );
}

function PlanRowStatic({ role, value, note }: { role: RoleKey; value: string; note?: string }) {
  return (
    <div className="flex items-center gap-2.5 px-3 py-2.5">
      <span className="w-[92px] flex-none text-[10.5px] text-[rgba(255,255,255,0.4)]">{t(ROLE_LABEL[role])}</span>
      <span className="flex-1 min-w-0 text-[11px] text-[#fafaf9] font-mono truncate">{value}</span>
      {note && <span className="flex-none text-[9px] text-[rgba(255,255,255,0.35)] max-w-[240px] truncate">{note}</span>}
    </div>
  );
}

/** Editable plan row: role label + free-text mono input (cloud bundles have
 *  no candidate list to select from — every default stays hand-editable). */
function PlanRowInput({
  role,
  value,
  onChange,
  placeholder,
}: {
  role: RoleKey;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <div className="flex items-center gap-2.5 px-3 py-2.5">
      <span className="w-[92px] flex-none text-[10.5px] text-[rgba(255,255,255,0.4)]">{t(ROLE_LABEL[role])}</span>
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder ?? "—"}
        className={`${control} font-mono flex-1 min-w-0`}
        data-testid={`cloud-row-${role}`}
      />
    </div>
  );
}

// ── saved-scenario preview helpers (shared with Settings › Scenarios) ─────────

/** Relative "saved N ago" label from an epoch-seconds string. */
export function relDate(epochSecs: string): string {
  const n = parseInt(epochSecs, 10);
  if (!n) return "";
  const diff = Math.max(0, Math.floor(Date.now() / 1000) - n);
  if (diff < 60) return t("scen.saved.now");
  if (diff < 3600) return td("scen.saved.min", [String(Math.floor(diff / 60))]);
  if (diff < 86400) return td("scen.saved.hr", [String(Math.floor(diff / 3600))]);
  return td("scen.saved.day", [String(Math.floor(diff / 86400))]);
}

// Providers whose saved-scenario preview shows a "needs key" chip when the key
// is blank. "custom" is intentionally absent — keyless LAN servers are valid.
export const KEY_PROVIDERS = new Set(["openai", "openrouter", "anthropic", "google", "fireworks", "cerebras"]);

/** Host (with port) of a base URL — "http://localhost:8000" → "localhost:8000",
 *  "https://api.openai.com" → "api.openai.com". Empty for keyless/local. */
export function hostOf(url?: string): string {
  const u = (url ?? "").trim();
  if (!u) return "";
  try {
    return new URL(u).host;
  } catch {
    return u.replace(/^[a-z]+:\/\//i, "").replace(/\/.*$/, "");
  }
}

/** One role row inside a saved-scenario preview — mirrors the step-2 plan rows:
 *  role label (muted, fixed width) → model (mono) + base-URL host (muted) +
 *  optional voice name, with an amber "needs key" chip when a key is missing.
 *  Exported so the Settings › Scenarios tab renders the same preview rows. */
export function PreviewRow({
  role,
  profile,
  voice,
}: {
  role: RoleKey;
  profile: ModelProfile;
  voice?: string;
}) {
  const host = hostOf(profile.base_url);
  const needsKey = KEY_PROVIDERS.has(profile.provider) && !(profile.api_key ?? "");
  const showVoice = (role === "conv" || role === "listen") && !!voice && voice !== "default";
  return (
    <div className="flex items-center gap-2 px-3 py-2">
      <span className="w-[92px] flex-none text-[10px] text-[rgba(255,255,255,0.4)]">{t(ROLE_LABEL[role])}</span>
      <span className="min-w-0 text-[11px] text-[#fafaf9] font-mono truncate">{profile.model || profile.name || "—"}</span>
      {host && <span className="flex-none text-[9px] text-[rgba(255,255,255,0.3)] font-mono">{host}</span>}
      {showVoice && <span className="flex-none text-[9px] text-[rgba(255,255,255,0.3)]">{voice}</span>}
      {needsKey && (
        <span className="ml-auto flex-none text-[8.5px] px-1.5 py-0.5 rounded-full bg-[rgba(242,184,75,0.12)] text-[var(--accent)]">
          {t("scen.saved.needkey")}
        </span>
      )}
    </div>
  );
}
