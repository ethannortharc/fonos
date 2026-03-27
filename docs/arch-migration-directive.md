# Fonos Architecture Migration — Claude Code Directive

## What You're Doing

Refactor the fonos-app codebase into a two-layer architecture. This is a **pure refactor — no new features, no behavior changes**. Everything that works today must work identically after.

## Two Changes

### 1. Extract `fonos-core` Rust crate

Create `fonos/fonos-core/` as a **platform-independent** Rust library crate. Move ALL business logic out of `fonos-app/src-tauri/` into this crate.

**What moves to fonos-core** — everything that is NOT macOS-specific and NOT Tauri-specific:
- Config management (load, save, defaults, validation)
- Mode definitions and CRUD (built-in + custom)
- Stats tracking
- LLM client (HTTP calls to OpenAI-compat endpoints)
- Model capability detection
- Voice/STT/TTS API clients
- Database operations, history storage/query
- Any other business logic you find in src-tauri

**What stays in src-tauri** — only macOS platform bindings:
- CoreAudio capture/playback
- CGEvent hotkey registration
- AXUIElement text injection
- NSStatusBar tray icon
- Process spawn/kill (server manager)
- Tauri `#[command]` wrappers (thin bridge: deserialize → call fonos-core → serialize)

**Hard constraints on fonos-core:**
- Zero `tauri` dependency anywhere (not in Cargo.toml, not in any source file)
- Zero `#[cfg(target_os)]` or any platform-specific code
- All public types derive `Serialize, Deserialize`
- All public functions return `Result<T, fonos_core::Error>`
- All I/O is async (tokio + reqwest)
- Every `pub fn` and `pub struct` has `///` doc comments

**Workspace layout:**
```
fonos/                     # Cargo workspace root
├── Cargo.toml             # [workspace] members = ["fonos-core", "fonos-app/src-tauri"]
├── fonos-core/            # NEW — platform-independent library
│   ├── Cargo.toml
│   └── src/
└── fonos-app/
    ├── src-tauri/          # Tauri shell — thin platform adapter
    │   └── Cargo.toml     # depends on fonos-core = { path = "../../fonos-core" }
    └── src/                # Frontend (see below)
```

### 2. Migrate frontend from vanilla JS to React + TypeScript

Replace all vanilla HTML/JS/CSS in `fonos-app/src/` with React + TypeScript + Tailwind CSS + Vite.

**Stack:**
- React 19 + TypeScript (strict mode)
- Vite as bundler (Tauri's recommended setup)
- Tailwind CSS for styling
- No external state management library (use React hooks + context)
- No component library yet (raw Tailwind)

**Requirements:**
- Every existing view/panel/feature must have a corresponding React component
- All Tauri invoke calls must be wrapped in typed functions (no raw `invoke()` with string literals)
- TypeScript types must match the Rust types from fonos-core
- Zero `any` types
- All existing e2e tests must be adapted and passing

## How to Execute

### Step 0 — Analyze current state

Before writing any code, read the entire codebase and produce a migration plan:

1. List every file in `fonos-app/src-tauri/src/` and categorize each as "moves to fonos-core" or "stays in src-tauri"
2. List every file in `fonos-app/src/` and map each to a React component
3. Identify all Tauri commands currently exposed and their signatures
4. Identify all database tables/schemas currently in use
5. Identify all config/state files on disk
6. Output this analysis before proceeding

### Step 1 — Extract fonos-core (Rust only)

- Create workspace Cargo.toml at `fonos/Cargo.toml`
- Create fonos-core crate
- Move business logic module by module
- Keep Tauri commands as thin wrappers calling fonos-core
- Run `cargo test --workspace` after each module migration — must stay green
- Verify: `grep -r "tauri" fonos-core/` returns nothing
- Verify: `grep -r "cfg(target_os" fonos-core/src/` returns nothing

### Step 2 — Migrate frontend to React

- Backup current `fonos-app/src/` to `fonos-app/src-old/`
- Initialize Vite + React + TypeScript
- Create typed Tauri invoke wrappers
- Migrate views one by one, referencing `src-old/` for behavior
- After each view: run e2e tests for that feature
- After all views: delete `src-old/`

### Step 3 — Integration verification

- `cargo test --workspace` — all pass
- `cd fonos-app && npm run build` — zero TS errors
- `cd fonos-app && cargo tauri build --debug` — app launches, tray icon appears
- `cd fonos-app && npx playwright test` — all e2e pass
- Manual smoke: dictation, mode switch, history, TTS, settings persist across restart

## Verification Checklist

After migration, ALL of these must be true:

- [ ] `cd fonos-core && cargo build && cargo test` passes
- [ ] `grep -r "tauri" fonos-core/` returns empty
- [ ] `grep -r "cfg(target_os" fonos-core/src/` returns empty
- [ ] `cargo test --workspace` passes
- [ ] `cd fonos-app && npm run build` succeeds (tsc + vite, zero errors)
- [ ] `cd fonos-app && cargo tauri build --debug` succeeds, app launches
- [ ] All existing e2e tests pass (adapted for React selectors)
- [ ] Config at `~/Library/Application Support/com.fonos.app/` loads correctly (backward compat)
- [ ] Custom modes at `~/.fonos/modes.json` load correctly
- [ ] Database and history data remain accessible
- [ ] No vanilla JS files remain in `fonos-app/src/` (excluding node_modules/dist)
- [ ] Every `pub` item in fonos-core has doc comments

## Why This Architecture

fonos-core will be shared with a future iOS app (SwiftUI + UniFFI bridge). Design the public API with that in mind — a Swift developer calling these functions through UniFFI bindings should find the API intuitive. But do NOT implement UniFFI or iOS anything in this intent.
