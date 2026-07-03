# Fonos Architecture

Fonos is evolving from a desktop app into **app + platform**: `fonos-core` is a
platform-independent engine (pipeline orchestration, data flow, service
invocation); platform apps implement display, input, and OS integration as
adapters. Internal-first; a public library is the end state (tracked in #21).

## Layout (hexagonal / ports & adapters)

```
fonos-core                     platform-independent engine
├── pipeline    dictation orchestration state machine + PipelineEvent enum
├── stt         STT clients (whisper-HTTP, chat-completions) + prompt biasing
├── llm         LLM clients (openai-compatible / anthropic / google)
├── services    model-profile → ServiceConfig resolution
├── error_class pipeline error classification (bad key / rate limit / permission …)
├── vocab       vocab books: terms, correction rules, glossaries
├── stats       usage events, latency percentiles
├── storage     entries/containers + FTS search
├── config      AppConfig (serde JSON, atomic save)
└── modes       dictation modes (raw / polish / …)

fonos-desktop                  Tauri adapter + assembly
├── injection   CGEvent/xdotool text delivery        (TextSink adapter)
├── audio       cpal capture, preprocessing          (AudioSource adapter)
├── hotkey      CGEventTap global hotkeys
├── error_surface  PipelineEvent/classified error → float:* Tauri events (EventSink adapter)
└── commands/*  thin Tauri command wrappers around core

fonos-ios                      second consumer of the same core (validates ports)
```

## Rules

1. **fonos-core never imports tauri, cocoa, or process-spawning platform code.**
   Anything touching CGEvent, osascript, xdotool, Swift helpers, or webviews is
   an adapter and lives in the platform crate.
2. **The pipeline speaks `PipelineEvent`, not UI strings.** Adapters translate
   typed events into their surface (Tauri `float:*` events, iOS notifications).
3. **One flow, three entry points.** Hold / toggle / Linux hotkey paths and the
   in-app view are wirings of the same `core::pipeline` flow — behavior changes
   are made once, in core, under unit tests with fake adapters.
4. **Errors are classified in core** (`error_class`); adapters only render them.

## Dependency audit (2026-07, basis for the refactor)

| Module (desktop) | tauri refs | platform refs | Verdict |
|---|---|---|---|
| commands/dictation.rs (1048 ln) | 19 | 9 (apple helper spawn) | orchestration + STT clients → core; capture/apple stay |
| main.rs (1617 ln) | 75 | 15 | 3× duplicated post-stop flow → core pipeline |
| error_surface.rs | 3 (emit only) | 0 | classification → core; emit stays |
| commands/mod.rs (profile resolution) | 20 | 0 | pure resolution → core::services |
| injection.rs / hotkey.rs / selection.rs / audio/capture.rs | 0 | 23–32 | true adapters — stay |
| commands/{storage,stats,config,llm,…}.rs | 6–24 | 0 | already thin wrappers over core |

## Migration status

- Phase 0 — audit + this document: **done**
- Phase 1 — mechanical moves (STT clients, error classification, service resolution → core): see #21
- Phase 2 — ports + `PipelineEvent` + `core::pipeline`: see #21
- Phase 3 — iOS on the same pipeline; public API (semver, docs): follow-up after #21
