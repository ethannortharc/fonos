# Fonos UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the entire Fonos frontend with Warm Dark theme, sidebar navigation, and model profile CRUD in Settings.

**Architecture:** Replace the horizontal tab bar with a 54px icon-only sidebar. Restyle all 5 views with amber accent on warm near-black backgrounds. Restructure Settings into sub-tabs with a provider-first model add flow. Float pill gets matching warm theme.

**Tech Stack:** React 19, TypeScript, Tailwind CSS 4, Tauri 2 IPC, vanilla HTML/CSS/JS (float pill only)

**Spec:** `docs/superpowers/specs/2026-03-22-ui-redesign-design.md`

---

## File Structure

| File | Responsibility |
|---|---|
| `fonos-app/src/index.css` | Tailwind directives + CSS custom properties for design tokens |
| `fonos-app/src/App.tsx` | Sidebar shell + view router |
| `fonos-app/src/views/Dictation.tsx` | Recording view with waveform, mode chips, transcript |
| `fonos-app/src/views/Voice.tsx` | TTS synthesis + voice library |
| `fonos-app/src/views/History.tsx` | Event history with dot indicators |
| `fonos-app/src/views/Stats.tsx` | Usage statistics with amber chart |
| `fonos-app/src/views/Settings.tsx` | Tabbed settings with model CRUD |
| `fonos-app/src/float.html` | Floating pill indicator (standalone HTML) |
| `fonos-core/src/config.rs` | Remove hardcoded model defaults |

---

### Task 1: Design tokens + backend config change

**Files:**
- Modify: `fonos-app/src/index.css`
- Modify: `fonos-core/src/config.rs:49-86`

- [ ] **Step 1: Add CSS custom properties to index.css**

Replace the contents of `fonos-app/src/index.css` with Tailwind directives and design tokens:

```css
@import "tailwindcss";

:root {
  --bg: #1a1917;
  --bg-sidebar: #151413;
  --bg-card: rgba(255,255,255,0.02);
  --border: rgba(255,255,255,0.05);
  --border-hover: rgba(255,255,255,0.08);
  --text-primary: #fafaf9;
  --text-secondary: rgba(255,255,255,0.35);
  --text-muted: rgba(255,255,255,0.2);
  --accent: #fbbf24;
  --accent-from: #f59e0b;
  --accent-to: #d97706;
  --accent-bg: rgba(245,158,11,0.12);
  --type-stt: #fbbf24;
  --type-tts: #c4b5fd;
  --type-llm: #86efac;
  --danger: #ef4444;
}

body {
  font-family: -apple-system, 'SF Pro Display', system-ui, sans-serif;
  background: var(--bg);
  color: var(--text-primary);
  -webkit-font-smoothing: antialiased;
}
```

- [ ] **Step 2: Remove hardcoded local model defaults from config.rs**

In `fonos-core/src/config.rs`, change the `Default` impl to use empty model profiles:

```rust
model_profiles: vec![],
stt_profile: String::new(),
tts_profile: String::new(),
llm_profile: String::new(),
```

Remove the three `serde_json::json!({...})` entries for `local-fast`, `local-accurate`, and `local-tts`.

- [ ] **Step 3: Verify build**

Run: `cargo build --workspace && cd fonos-app && npm run build`
Expected: Both succeed.

- [ ] **Step 4: Commit**

```bash
git add fonos-app/src/index.css fonos-core/src/config.rs
git commit -m "feat: add design tokens and remove hardcoded local model defaults"
```

---

### Task 2: App shell — sidebar navigation

**Files:**
- Modify: `fonos-app/src/App.tsx`

- [ ] **Step 1: Rewrite App.tsx with sidebar layout**

Replace the horizontal tab bar with a sidebar. The sidebar has:
- 54px width, `bg-[#151413]`, border-right
- `f` lettermark in amber gradient (30x30, rounded-[9px]) at top
- 5 nav items as SVG icon buttons (38x38, rounded-[10px])
- Active: amber stroke + `bg-[rgba(245,158,11,0.12)]`
- Inactive: `stroke-[rgba(255,255,255,0.22)]`
- Settings pinned to bottom with `flex-1` spacer

SVG icons (all 24x24 viewBox, stroke-width 1.8, no fill):
- **Dictation** (mic): `<path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/>`
- **Voice** (speaker): `<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>`
- **History** (clock): `<path d="M12 8v4l3 3"/><circle cx="12" cy="12" r="10"/>`
- **Stats** (chart): `<path d="M18 20V10"/><path d="M12 20V4"/><path d="M6 20v-6"/>`
- **Settings** (gear): `<circle cx="12" cy="12" r="3"/><path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/>`

Layout: `flex h-screen` with sidebar + content area (`flex-1 overflow-hidden`).

- [ ] **Step 2: Verify build**

Run: `cd fonos-app && npm run build`
Expected: Success.

- [ ] **Step 3: Commit**

```bash
git add fonos-app/src/App.tsx
git commit -m "feat: replace tab bar with sidebar navigation"
```

---

### Task 3: Dictation view redesign

**Files:**
- Modify: `fonos-app/src/views/Dictation.tsx`

- [ ] **Step 1: Restyle Dictation.tsx**

Key changes to apply:
- Root: remove `bg-[#1c1c1e]`, use `bg-[var(--bg)]`
- Mode chips: replace `rounded-full bg-[#0a84ff]` with `rounded-lg bg-[var(--accent-bg)] text-[var(--accent)]` for active, `bg-[rgba(255,255,255,0.04)] text-[rgba(255,255,255,0.35)]` for inactive
- Waveform: replace sine-wave canvas with bar visualization using vertical `<div>` bars, amber gradient colors. Container: `rounded-xl bg-[var(--bg-card)] border border-[var(--border)]`
- Record button: 48px round, `bg-gradient-to-br from-[var(--accent-from)] to-[var(--accent-to)]` idle, `from-red-500 to-red-600` recording. Use SVG mic icon idle, square stop icon recording.
- Transcript card: `bg-[var(--bg-card)] border-[var(--border)]`, amber label instead of blue
- LLM result card: `bg-[var(--accent-bg)] border-[rgba(245,158,11,0.15)]`
- Status bar: model name left, hotkey hint right, `text-[var(--text-muted)]`
- Remove all `#0a84ff` references

- [ ] **Step 2: Verify build**

Run: `cd fonos-app && npm run build`
Expected: Success.

- [ ] **Step 3: Commit**

```bash
git add fonos-app/src/views/Dictation.tsx
git commit -m "feat: redesign Dictation view with warm dark theme"
```

---

### Task 4: Voice view redesign

**Files:**
- Modify: `fonos-app/src/views/Voice.tsx`

- [ ] **Step 1: Restyle Voice.tsx**

Key changes to apply:
- Root: `bg-[var(--bg)]`
- Voice chips: same styling as Dictation mode chips (amber active)
- Add `+ Clone` chip with dashed border: `border border-dashed border-[rgba(245,158,11,0.2)] text-[rgba(251,191,36,0.5)]`
- Textarea: `bg-[rgba(255,255,255,0.03)] border-[rgba(255,255,255,0.06)]`, focus border `border-[rgba(245,158,11,0.3)]`
- Speed slider: replace native range input with custom styled slider — amber track (`bg-gradient-to-r from-[rgba(251,191,36,0.3)] to-[var(--accent)]`), amber knob (12px circle, `bg-[var(--accent)]` with shadow)
- Generate button: full width, `bg-gradient-to-br from-[var(--accent-from)] to-[var(--accent-to)] text-[#1a1917]` (dark text on amber)
- Result card: warm theme styling
- Remove all `#0a84ff` references

- [ ] **Step 2: Verify build**

Run: `cd fonos-app && npm run build`
Expected: Success.

- [ ] **Step 3: Commit**

```bash
git add fonos-app/src/views/Voice.tsx
git commit -m "feat: redesign Voice view with warm dark theme"
```

---

### Task 5: History view redesign

**Files:**
- Modify: `fonos-app/src/views/History.tsx`

- [ ] **Step 1: Restyle History.tsx**

Key changes to apply:
- Root: `bg-[var(--bg)]`
- Filter chips: amber active state, same chip styling as other views
- Replace type badge chips (`bg-blue-500/20 text-blue-300` etc.) with 6px colored dots: `w-1.5 h-1.5 rounded-full` with `bg-[var(--type-stt)]` / `bg-[var(--type-tts)]` / `bg-[var(--type-llm)]`
- Add relative timestamp formatting: "Today, HH:MM" / "Yesterday, HH:MM" / "Mar 20, HH:MM"
- Event row layout: dot + timestamp + flex spacer + metadata (duration · latency)
- Text preview on its own line below
- Row container: `rounded-lg bg-[var(--bg-card)] border border-[var(--border)]`
- Remove all `#0a84ff` references

Helper function for relative time:
```typescript
function relativeTime(isoDate: string): string {
  const d = new Date(isoDate);
  const now = new Date();
  const today = now.toDateString();
  const yesterday = new Date(now.getTime() - 86400000).toDateString();
  const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (d.toDateString() === today) return `Today, ${time}`;
  if (d.toDateString() === yesterday) return `Yesterday, ${time}`;
  return `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })}, ${time}`;
}
```

- [ ] **Step 2: Verify build**

Run: `cd fonos-app && npm run build`
Expected: Success.

- [ ] **Step 3: Commit**

```bash
git add fonos-app/src/views/History.tsx
git commit -m "feat: redesign History view with dots and relative timestamps"
```

---

### Task 6: Stats view redesign

**Files:**
- Modify: `fonos-app/src/views/Stats.tsx`

- [ ] **Step 1: Restyle Stats.tsx**

Key changes to apply:
- Root: `bg-[var(--bg)]`
- Period chips: amber active state
- Summary tiles: 3-column grid, `bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px]`. Label (10px, uppercase, secondary) / Value (18px, 600, primary) / Sub (10px, muted)
- Bar chart: replace canvas with CSS bars (`<div>` per day). Each bar: `bg-[var(--accent)]` with opacity varying by value (0.2 min, 1.0 max), `rounded-t-[3px]`. Date labels below in muted text.
- Breakdown: replace card-wrapped sections with inline row of colored dots + text: `<div class="type-dot type-stt"></div> 42 STT · 3,840 words`
- Remove all `#0a84ff` references, replace blue with amber

- [ ] **Step 2: Verify build**

Run: `cd fonos-app && npm run build`
Expected: Success.

- [ ] **Step 3: Commit**

```bash
git add fonos-app/src/views/Stats.tsx
git commit -m "feat: redesign Stats view with amber chart and inline breakdown"
```

---

### Task 7: Settings view — sub-tabs + model CRUD

**Files:**
- Modify: `fonos-app/src/views/Settings.tsx`

This is the largest task. Settings is restructured into 5 sub-tabs.

- [ ] **Step 1: Add tab state and tab bar**

Add `settingsTab` state: `"models" | "services" | "hotkeys" | "modes" | "language"`. Render tab bar with amber active styling (text + 2px bottom border).

- [ ] **Step 2: Implement Models tab — list view**

Show `config.model_profiles` as cards. Each card shows:
- Model name (12px, 500 weight)
- Provider · base_url (10px, muted)
- Capability badges: colored per type (`bg-[rgba(245,158,11,0.1)] text-[rgba(251,191,36,0.7)]` for STT, purple for TTS, green for LLM)
- Edit/delete buttons (visible on hover)
- "+ Add Model" button at bottom

- [ ] **Step 3: Implement Models tab — add/edit form**

Provider-first flow:
1. Provider picker grid: OpenAI, Anthropic, Google, Ollama, LM Studio, OMLX, Custom
2. On select, show form with pre-filled `base_url`:
   - OpenAI: `https://api.openai.com`
   - Anthropic: `https://api.anthropic.com`
   - Google: `https://generativelanguage.googleapis.com`
   - Ollama: `http://localhost:11434`
   - LM Studio: `http://localhost:1234`
   - OMLX: `http://localhost:8000`
   - Custom: empty
3. Form fields: Name (text), Model ID (text), API Key (password, optional), Base URL (pre-filled, editable), Capabilities (multi-select checkboxes: STT/TTS/LLM)
4. Save generates a UUID-like `id` (`provider-timestamp`), adds to `config.model_profiles`, calls `saveConfig`
5. Edit pre-fills the form from existing profile

- [ ] **Step 4: Implement Services tab**

Three rows (STT, TTS, LLM). Each is a label + `<select>` dropdown.
- STT dropdown: shows only profiles with `"stt"` in `capabilities`
- TTS dropdown: shows only profiles with `"tts"` in `capabilities`
- LLM dropdown: shows only profiles with `"llm"` in `capabilities`
- Each has a "None" option. Selection calls `handleSave({ stt_profile: id })`.

- [ ] **Step 5: Restyle Hotkeys, Modes, Language tabs**

Same functionality as current, just restyled:
- Replace all `bg-[#0a84ff]` with amber gradient
- Replace all `border-[#0a84ff]` with `border-[rgba(245,158,11,0.3)]`
- Replace all `text-[#0a84ff]` with `text-[var(--accent)]`
- Card/input backgrounds: `bg-[rgba(255,255,255,0.03)]`, borders `rgba(255,255,255,0.06)`

- [ ] **Step 6: Verify build**

Run: `cd fonos-app && npm run build`
Expected: Success.

- [ ] **Step 7: Commit**

```bash
git add fonos-app/src/views/Settings.tsx
git commit -m "feat: redesign Settings with sub-tabs and model profile CRUD"
```

---

### Task 8: Float pill redesign

**Files:**
- Modify: `fonos-app/src/float.html`

- [ ] **Step 1: Update float.html colors and styling**

Replace all color values:
- Cyan `rgba(0,212,255,...)` → amber `rgba(251,191,36,...)`
- Cool gray `rgba(22,22,26,...)` → warm dark `rgba(26,25,23,...)`
- Background: `rgba(26,25,23,0.75)` idle, `rgba(26,25,23,0.9)` recording
- Border: `rgba(255,255,255,0.06)` idle, `rgba(255,69,58,0.15)` recording
- FONOS text: `rgba(251,191,36,0.35)`
- Dots: `rgba(251,191,36,0.2)`
- Waveform stroke: `rgba(251,191,36,0.5)`
- Hover toolbar border: `rgba(255,255,255,0.12)`
- Active item color: `rgba(251,191,36,0.9)` (was `rgba(0,212,255,0.9)`)
- Add item color: `rgba(251,191,36,0.5)` (was `rgba(0,212,255,0.5)`)

- [ ] **Step 2: Verify build**

Run: `cd fonos-app && npm run build`
Expected: Success (float.html is static, but verify nothing breaks).

- [ ] **Step 3: Commit**

```bash
git add fonos-app/src/float.html
git commit -m "feat: redesign float pill with warm amber theme"
```

---

### Task 9: Final verification

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace && cd fonos-app && npm run build`
Expected: Both succeed.

- [ ] **Step 2: TypeScript check**

Run: `cd fonos-app && npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 3: Run tests**

Run: `cargo test --workspace --features ci`
Expected: All pass.

- [ ] **Step 4: Verify no old blue references remain**

Run: `grep -r "0a84ff" fonos-app/src/ --include="*.tsx" --include="*.css" --include="*.html" | head -20`
Expected: No matches (all blue replaced with amber).

- [ ] **Step 5: Commit any remaining fixes**

If any issues found, fix and commit.
