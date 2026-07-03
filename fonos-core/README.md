# fonos-core

Platform-independent voice-dictation engine: pipeline orchestration, STT/LLM
clients, vocabulary correction, storage, and usage statistics. Platform apps
(the Tauri desktop app, iOS, or your own) embed this crate and implement only
the **ports** — audio capture, text delivery, and UI notification.

Every public item carries rustdoc (`#![deny(missing_docs)]` is enforced), so
`cargo doc -p fonos-core --open` is the exhaustive reference. This README is
the guided tour: what each module is for, the interfaces a platform shell must
implement, and how the pieces compose into a dictation.

See also [`../ARCHITECTURE.md`](../ARCHITECTURE.md) for the layering rules and
migration status.

---

## The dictation flow at a glance

```text
 audio bytes (WAV)                                shell owns capture
      │
      ▼
 stt::transcribe_http / transcribe_chat           core: STT client + vocab biasing
      │  transcript
      ▼
 vocab::apply_rules                               core: deterministic corrections
      │
      ▼
 llm::process_text          (LLM modes only)      core: mode prompt + glossary
      │
      ▼
 pipeline::deliver_llm_result                     core: inject / notify, one flow
      │            │
   TextSink     EventSink                         ports — implemented by the shell
```

A shell that provides audio bytes and two small trait impls gets the entire
capture-to-delivery behavior: vocabulary biasing, correction rules, LLM
glossaries, error classification, typed UI events, and latency accounting.

---

## Ports — what a platform shell implements

### `pipeline::EventSink`

```rust
pub trait EventSink: Send + Sync {
    fn emit(&self, event: PipelineEvent);
}
```

Translates typed [`PipelineEvent`]s onto your UI surface:

| Event | Meaning | Desktop pill mapping |
|---|---|---|
| `Processing` | an LLM stage is running | `float:processing` |
| `Delivered(String)` | pipeline finished, text delivered | `float:stop` + text |
| `NoSpeech` | recording had no usable speech | `float:stop` + `""` |
| `Failed(SurfacedError)` | classified failure (see `error_class`) | `float:error` JSON |

### `pipeline::TextSink`

```rust
pub trait TextSink: Send + Sync {
    fn inject(&self, text: &str) -> Result<(), String>;
    fn press_enter(&self) -> Result<(), String>;
}
```

Deliver text at the user's cursor (CGEvent paste/type on macOS, keyboard
extension on iOS, …). Errors are raw strings — core classifies them before
they reach the user.

### Driving the delivery flow

```rust
use fonos_core::pipeline::{deliver_llm_result, DeliveryOutcome, LlmStageOutput};

let stage: Result<LlmStageOutput, String> = run_your_llm_stage().await;
let outcome = deliver_llm_result(stage, &my_event_sink, &my_text_sink).await;
if outcome == DeliveryOutcome::Delivered {
    // e.g. record end-to-end latency (see stats below)
}
```

`deliver_llm_result` guarantees **exactly one terminal event**, honors
`auto_paste` / `auto_press_enter`, and classifies both LLM and injection
failures. Its behavior is pinned by unit tests with fake sinks
(`pipeline::tests`) — extend those when you extend the flow.

---

## Service resolution — `services`

Model profiles live in `AppConfig.model_profiles` (JSON entries with
`id`, `provider`, `api_key`, `model`, `base_url`, `capabilities`, `stt_api`).

```rust
use fonos_core::services;

let stt  = services::resolve_service(&config, "stt");   // active STT profile
let llm  = services::resolve_service(&config, "llm");   // active LLM profile
let one  = services::resolve_profile(&config, "profile-id");
```

All three return [`llm::ServiceConfig`] — the single connection type used by
every client in the crate (`provider`, `api_key`, `model`, `base_url`,
`stt_api`). Empty `base_url` in a profile resolves to the provider's default
endpoint (openai / anthropic / google / openrouter / ollama / omlx).

---

## STT clients — `stt`

```rust
// Whisper-compatible multipart upload (OpenAI, OMLX, faster-whisper, …)
let text = fonos_core::stt::transcribe_http(
    &svc,            // ServiceConfig
    &wav_bytes,      // complete WAV
    "model-name",
    "zh",            // ISO 639-1, "" = auto
    Some(&mode),     // Option<&Mode>: contributes stt_prompt / stt_temperature
    &vocab_terms,    // biasing terms, merged into the prompt (budget-capped)
).await?;

// Chat-completions with base64 audio (OpenRouter / Gemini / Voxtral style)
let text = fonos_core::stt::transcribe_chat(&svc, &wav_bytes, "zh", &vocab_terms).await?;
```

Select between them with `svc.stt_api` (`"whisper"` default, `"chat"`).
Platform-specific engines (e.g. Apple's on-device recognizer) are adapters in
the shell — core only ships network clients.

---

## LLM processing — `llm`

```rust
let service = services::resolve_service(&config, "llm");
let resp = fonos_core::llm::process_text(
    &transcript,
    &mode,                 // Mode: system prompt, user_template, temperature…
    &service,
    caps.as_ref(),         // Option<&ModelCaps> from model_caps probing
    &config.translate_target,
).await?;                  // LlmResponse { text, tokens_in, tokens_out }
```

Dispatches by provider (OpenAI-compatible / Anthropic / Google). Lower-level
`call_openai_compatible` / `call_anthropic` are public for custom message
shapes (the meeting summarizer uses them).

---

## Vocabulary — `vocab`

User-defined books of domain terms + correction rules, referenced by id from
`AppConfig.global_vocab_books` and per-mode `Mode.vocab_books`:

```rust
use fonos_core::vocab;

let books  = vocab::effective_books(&config.vocab_books,
                                    &config.global_vocab_books,
                                    &mode.vocab_books);        // global ∪ mode
let terms  = vocab::collect_terms(&books);

// ① recognition biasing — merge into the STT prompt (~224-token budget)
let prompt = vocab::build_stt_prompt(&mode.stt_prompt, &terms,
                                     vocab::STT_PROMPT_BUDGET_CHARS);
// ② deterministic corrections — literal (word-boundary/CJK-aware) + regex
let fixed  = vocab::apply_rules(&transcript, &books);
// ③ LLM glossary — appended to the mode's system prompt
if let Some(block) = vocab::build_glossary_block(&terms) { /* append */ }
```

Invalid user regexes are skipped, never fatal. `transcribe_http` already calls
`build_stt_prompt` internally when you pass `vocab_terms`.

---

## Errors — `error_class`

```rust
let s = fonos_core::error_class::classify_error(raw);
// SurfacedError { message: String, pane: Option<&'static str> }
```

Turns raw pipeline errors into short actionable messages; `pane` names an OS
settings pane (`"microphone"`, `"accessibility"`, …) for permission errors so
shells can deep-link. Classification rules are pinned by 14 unit tests.

---

## Storage — `storage`

SQLite (`rusqlite`), schema owned by `init_storage_db(&conn)`:

- `Entry` (dictation / note / meeting / agent rows) + `Container`
  (notebook / meeting session), CRUD: `insert_entry`, `get_entries(EntryFilter)`,
  `update_entry`, `delete_entry`, `insert_container`, `get_container_entries`, …
- **Full-text search**: `search_entries(&conn, query, limit)` — FTS5 trigram
  (substring + CJK native). Queries are treated as literal text (apostrophes
  and stray operators can't break the parser); queries under 3 chars fall back
  to `LIKE`.

## Statistics — `stats`

Usage events + latency accounting, schema owned by `init_db(&conn)`:

```rust
stats::record_event(&conn, "stt", input, output, dur, latency, mode, model, …)?;
stats::record_dictation_latency(&conn, e2e_ms, mode, stt_model)?;   // per request
stats::get_dictation_latency(&conn, "2026-07-01", "2026-07-31")?;   // P50/P95 + per-model
stats::get_daily_stats(&conn, from, to)?;   stats::get_today(&conn)?;
```

Latency percentiles are nearest-rank; `dictation` rows never inflate session
counts.

## Config & modes — `config`, `modes`

- `AppConfig::load()` / `save()` — JSON at the platform data dir, atomic
  writes, `#[serde(default)]` so new fields are backward-compatible.
- `modes::all_modes()` — built-ins overlaid by user-defined modes (a custom
  mode with a built-in id shadows it); `save_custom_modes` persists.

---

## Adapter checklist for a new platform

1. Capture audio, hand core a complete WAV (`audio::write_wav` helps).
2. Implement `EventSink` (map `PipelineEvent` to your UI) and `TextSink`
   (deliver text; return raw error strings).
3. Resolve services from the shared `AppConfig` via `services::*`.
4. Wire the flow: `transcribe_* → vocab::apply_rules → llm::process_text
   (LLM modes) → pipeline::deliver_llm_result`.
5. Record `stats::record_dictation_latency` on `DeliveryOutcome::Delivered`.

What stays out of core by design: hotkeys, audio device handling, injection
mechanics, permission prompts, and any UI — those are yours.
