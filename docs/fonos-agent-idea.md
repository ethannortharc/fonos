# Fonos Agent Architecture — Idea Document

## What we're building

Add an Agent capability to fonos alongside the existing mode system. The user speaks, and instead of transforming their text (what modes do), the agent understands the intent and performs actions — running commands, opening apps, querying information, controlling the system.

## Core concept: two processing paths

Fonos currently has one processing path: voice → STT → LLM mode processing → text output at cursor. We're adding a second path: voice → STT → agent reasoning → skill execution → response (text or voice).

These are fundamentally different:
- **Mode**: single-pass text transformation. Input text, template, output text. Stateless. Fast.
- **Agent**: multi-step reasoning loop. Understand intent, choose tools, execute, observe results, maybe execute more, then respond. Stateful within a session. Slower but more powerful.

They share the same entry point (voice → STT) but diverge after that.

## User experience

### UI selection
One flat row of pills in the picker. Modes on the left (Raw, Polish, Formal, Translate, custom modes...), a visual divider, then a single "Agent" pill on the right. That's it — no sub-categories, no agent type selection.

### When a mode is selected
Current behavior, unchanged. User speaks → text appears at cursor. The result panel shows raw input and processed output.

### When Agent is selected
UI switches to a conversational view. User's spoken input appears as a chat bubble. Agent's response appears as a reply bubble. Between them, small status indicators show what the agent did (e.g., "Ran: ifconfig | grep inet"). The conversation context persists within the session — user can ask follow-up questions.

### Switching between mode and agent
Instant. Click a mode pill → you're back to text-at-cursor mode. Click Agent → you're in conversation mode. No confirmation, no data loss (agent conversation stays in memory if you switch away and back).

## Architecture in fonos-core (platform-independent)

### Processor router
A top-level dispatcher that receives STT output and routes it to either the ModeProcessor (existing) or the AgentProcessor (new) based on the user's current selection.

### Agent processor
The agent loop lives entirely in fonos-core. It has three components:

**Planner**: Takes user input + available skill descriptions + conversation history → calls LLM → gets back a plan (which skill to call, with what parameters). The planner also handles the "fast path" — simple commands that can be pattern-matched without an LLM call (e.g., "open Safari" → directly invoke app skill, no LLM needed).

**Executor**: Takes the plan → calls the appropriate skill → returns the raw result. The executor doesn't know what platform it's on — it calls skills through a trait interface.

**Responder**: Takes the raw result + original user input → optionally calls LLM to generate a natural language summary → returns the final response. For simple results (e.g., "app opened"), it can skip the LLM call and return a canned response.

The loop: planner → executor → responder. If the responder determines more steps are needed (rare — only for complex multi-step tasks), it loops back to the planner. Most commands are single-loop.

### Skill system

A skill is the unit of agent capability. Each skill declares:
- A name and description (LLM reads this to decide which skill to use)
- Parameter definitions
- An async execute function

Skills are registered in a SkillRegistry. The planner sends all skill descriptions to the LLM as part of the prompt, and the LLM picks the right one.

**Skill trait** (defined in fonos-core):
- Every skill implements this trait
- The trait is platform-independent — it defines the interface, not the implementation
- fonos-core includes NO concrete platform-specific skills

**Built-in skills** (registered by each platform):
- Desktop (fonos-app/src-tauri): shell execution, AppleScript, app open/switch, clipboard, system info, file operations
- iOS (future fonos-ios): Shortcuts invocation, HTTP API calls, clipboard, system info, iOS-specific actions

**Custom skills** (loaded from config):
- Users define skills in `~/.fonos/skills/*.json`
- Each JSON file specifies: name, description, type (shell/http/script), command template, parameters
- Custom skills are loaded at startup and registered alongside built-in skills
- This works the same on desktop and iOS (except `shell` type skills won't work on iOS — the loader skips incompatible types)

### Conversation context
The agent maintains a conversation history within a session (list of user inputs + agent responses). This is passed to the planner as context so the LLM can understand follow-up questions ("what about the second one?", "do that again but with port 8080"). Context is reset when the user switches away from Agent mode or when the session times out.

## Platform split: what goes where

### In fonos-core (shared, platform-independent)
- ProcessorRouter
- AgentProcessor (planner, executor, responder loop)
- Skill trait definition
- SkillRegistry (loading, matching, dispatching)
- Custom skill loader (reads JSON, creates skill instances)
- Conversation context management
- Fast-path pattern matcher (keyword → skill mapping for common commands)

### In fonos-app/src-tauri (macOS desktop only)
- Desktop skill implementations: ShellSkill, AppleScriptSkill, AppControlSkill, etc.
- Registration: at startup, create instances and register with fonos-core's SkillRegistry
- UI: the conversation view, picker with Agent pill, status indicators

### In fonos-ios (future, iOS only)
- iOS skill implementations: ShortcutsSkill, HTTPSkill, IOSSystemSkill, etc.
- Registration: same pattern — create and register at startup
- UI: SwiftUI version of the same conversation view

### Key principle
The agent engine doesn't know what platform it's on. It just asks the SkillRegistry "what can you do?" and gets a list of descriptions. The registry was populated at startup by the platform layer. This means:
- Adding a new desktop-only skill doesn't touch fonos-core
- Adding a new iOS-only skill doesn't touch fonos-core
- Adding a new cross-platform skill (like an HTTP API caller) goes in fonos-core
- The LLM prompt is automatically correct for each platform because it's generated from the actual registered skills

## Skill extensibility model

### Level 1: JSON config (no code)
User drops a JSON file in `~/.fonos/skills/`. Supports shell commands, HTTP calls, and script execution. Good enough for 80% of custom needs.

Example — a "check weather" skill:
```
name: "check_weather"
description: "Check the weather for a city"
type: "http"
url: "https://wttr.in/{city}?format=3"
parameters: { city: { description: "City name", default: "San Jose" } }
response_template: "Summarize this weather: {output}"
```

### Level 2: Script skill (light code)
JSON config points to a script file (Python, bash). The script receives parameters as environment variables, outputs to stdout. Good for complex logic that doesn't fit a one-liner.

### Level 3: Native skill (Rust, full control)
Developer implements the Skill trait in Rust. Full access to async, error handling, streaming. This is how built-in skills are written. Third-party skills could be loaded as dynamic libraries in the future, but that's not needed now.

## Skills management

No external skills platform needed. Fonos manages its own skills through a built-in UI.

### Skills settings page
The fonos settings/admin interface includes a "Skills" tab. This page is accessible from the desktop app's settings panel and also served as a web page by fonos-service (so you can manage skills from any browser on the local network — phone, tablet, another computer — by visiting the fonos IP/port).

The page shows:
- All installed skills (built-in + custom), each with an on/off toggle, edit button, and delete button (built-in skills can be toggled off but not deleted)
- A "New skill" button that opens a creation form: name, description, type (shell/http/script), command/URL/script path, parameters, response template, and a "speak response" toggle
- An "Import" button: paste a skill JSON or enter a URL pointing to a skill definition file → preview → install
- Each skill shows its type badge, whether it's enabled, and a "Test" button that lets you try it with sample input right from the settings page

### Hot reload
When a skill is created, edited, toggled, or deleted through the UI, the change is written to `~/.fonos/skills/` and the SkillRegistry is updated immediately. No restart needed. The LLM's available tool list updates on the next request.

### Sharing and discovery (future)
A simple skill directory — a public GitHub repo of curated skill JSON files. Users can browse it from the fonos UI's "Import" tab, search by category (system, productivity, development, web), preview the skill definition, and one-click install. Community members submit skills via PR to the repo. No server infrastructure needed — just a repo of JSON files with a simple index.

### Cross-platform sync
Skills stored in `~/.fonos/skills/` can be synced between desktop and iOS via iCloud/CloudKit (same mechanism as mode and config sync). Platform-incompatible skills (e.g., a shell-type skill on iOS) are synced but automatically disabled, with a note explaining why. If the user edits the skill to use an http type instead, it becomes available on iOS too.

## What NOT to build now

- No multi-agent orchestration. One agent, one conversation.
- No agent memory across sessions (conversation context resets). Memory/RAG integration comes later.
- No visual output from agent (no charts, no images). Text and TTS only.
- No proactive agent behavior (agent doesn't initiate actions without user input).
- No iOS implementation — just ensure fonos-core's design supports it.

## Relationship to existing systems

- **Mode system**: Untouched. Agent is a sibling, not a replacement. Modes continue to work exactly as before.
- **TTS**: Agent responses can optionally be spoken via TTS (user preference). Uses existing fonos TTS infrastructure.
- **History/DB**: Agent interactions are stored in the same history database as mode transcriptions. Each entry has a type field (mode vs agent) so they can be filtered.
- **Meeting mode**: A future agent type, not a separate system. When the user says "start recording this meeting", the agent activates BlackHole capture + continuous STT. This is just a long-running skill, not a new architecture.
- **fonos-core extraction**: The agent module is part of fonos-core from the start. It must compile and test independently of Tauri.
