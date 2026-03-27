# Fonos App: Mode System, Model Detection & Shortcuts

## What to build

Add a multi-mode text processing system to fonos-app with automatic model capability detection and a clean shortcut key design.

## 1. Mode System

The app processes STT output through an LLM before injecting text at the cursor. Define these built-in modes plus support for user-created custom modes.

### Built-in modes

```python
BUILT_IN_MODES = {
    "raw": {
        "name": "Raw",
        "description": "No processing, direct STT output",
        "system": None,
        "user_template": None,  # Skip LLM call entirely
        "temperature": 0.0,
    },
    "polish": {
        "name": "Polish",
        "description": "Speech to natural writing, preserves emotion and tone",
        "system": "You are a speech-to-writing assistant.",
        "user_template": (
            "Convert the following spoken text into natural, well-written text. "
            "Preserve the speaker's intent, emotion, and tone intensity — if they are angry, "
            "the output should feel angry; if they are excited, it should feel excited. "
            "Remove only speech artifacts (filler words, false starts, repetitions). "
            "Do not add new ideas. Do not make the tone more formal or neutral unless "
            "the original tone is neutral. "
            "Keep the original language. Output ONLY the polished text.\n\n"
            '"{text}"'
        ),
        "temperature": 0.1,
    },
    "formal": {
        "name": "Formal",
        "description": "Professional business writing",
        "system": "You are a professional writing assistant.",
        "user_template": (
            "Rewrite the following spoken text as professional written communication. "
            "Clear, concise, neutral tone. Remove colloquialisms and emotional expressions. "
            "Keep the original language. Output ONLY the rewritten text.\n\n"
            '"{text}"'
        ),
        "temperature": 0.2,
    },
    "translate_en": {
        "name": "→ English",
        "description": "Translate to English",
        "system": "You are a translator.",
        "user_template": (
            "Translate to English. Preserve the tone and intent. "
            "Output ONLY the translation.\n\n"
            '"{text}"'
        ),
        "temperature": 0.3,
    },
    "translate_zh": {
        "name": "→ 中文",
        "description": "Translate to Chinese",
        "system": "You are a translator.",
        "user_template": (
            "翻译成中文。保持原文的语气和意图。只输出翻译结果。\n\n"
            '"{text}"'
        ),
        "temperature": 0.3,
    },
}
```

### Custom modes

Users can create custom modes in Settings. Each custom mode has:
- `name`: Display name (e.g., "Bullet Points", "Code Review")
- `system`: System prompt (optional, can be empty)
- `user_template`: User prompt template, must contain `{text}` placeholder
- `temperature`: 0.0 - 1.0

Store custom modes in `~/.fonos/modes.json`:

```json
{
  "bullets": {
    "name": "Bullet Points",
    "system": "You are a note-taking assistant.",
    "user_template": "Convert the following speech into concise bullet points. Keep original language. Output ONLY the bullet points.\n\n\"{text}\"",
    "temperature": 0.1
  }
}
```

Load custom modes on app startup, merge with built-in modes. Custom modes appear after built-in modes in all UI lists.

## 2. Model Capability Detection

On first use of a new LLM model, automatically probe its capabilities and cache the results. This determines how prompts are constructed — no user configuration needed.

```python
# ~/.fonos/model_caps.json
# Auto-populated, never edited by user

async def probe_model(model_id: str, llm_client) -> dict:
    """Run once per model, cache result forever."""

    # Test: does the model follow system prompt strictly?
    resp = await llm_client.chat(
        model=model_id,
        system="Reply with exactly one word: BLUE",
        user="What is your favorite color?",
        temperature=0.0,
        max_tokens=10,
    )
    follows_system = "BLUE" in resp.strip().upper()

    # Test: does the model preserve input language?
    resp = await llm_client.chat(
        model=model_id,
        system="Clean the text. Remove filler words. Keep original language. Output only cleaned text.",
        user='"嗯，这个测试可以吗？"',
        temperature=0.0,
        max_tokens=50,
    )
    has_chinese = any('\u4e00' <= c <= '\u9fff' for c in resp)

    return {
        "model_id": model_id,
        "follows_system_prompt": follows_system,
        "preserves_language": has_chinese,
        "probed_at": datetime.now().isoformat(),
    }
```

Use the probe results when building messages:

```python
def build_messages(text: str, mode: dict, model_caps: dict) -> list:
    if mode["system"] is None and mode["user_template"] is None:
        return None  # Raw mode, skip LLM

    if model_caps.get("follows_system_prompt", False):
        # Model is capable: use proper system/user split
        messages = []
        if mode["system"]:
            messages.append({"role": "system", "content": mode["system"]})
        messages.append({
            "role": "user",
            "content": mode["user_template"].format(text=text)
        })
    else:
        # Model is weak at system prompts: merge everything into user
        combined = ""
        if mode["system"]:
            combined += mode["system"] + "\n\n"
        combined += mode["user_template"].format(text=text)
        messages = [{"role": "user", "content": combined}]

    return messages
```

Show probe status in Settings: "Model: Qwen3-4B-Instruct (system prompt: merged)" or "Model: Qwen3.5-9B (system prompt: separate)". Add a "Re-probe" button in case user wants to re-test after model update.

## 3. Shortcut Keys

### Design principle

One key for recording. Mode switching is rare — don't waste keyboard shortcuts on it.

### Recording shortcut

**Cmd+Shift+Space** (default, configurable in Settings)

- Press and hold → recording starts, tray icon turns red, floating indicator appears near cursor
- Release → recording stops, STT processes, LLM post-processes (if not raw mode), text injected at cursor
- Modifier override during recording (hold these BEFORE releasing the recording key):
  - Release normally → use current default mode
  - Hold **Option** while releasing → force Raw (skip LLM, useful for quick "just give me the transcript")
  - Hold **Control** while releasing → force Translate (auto-detect direction: CJK→English, else→Chinese)

These modifier overrides are temporary — they don't change the default mode.

### Mode switch shortcut

**Cmd+Shift+M** — opens a small floating mode picker near the cursor:

```
┌─────────────────────┐
│ ● Polish        (1) │  ← current default (dot indicator)
│   Formal        (2) │
│   → English     (3) │
│   → 中文        (4) │
│   Raw           (5) │
│ ─────────────────── │
│   Bullet Points (6) │  ← custom modes below separator
│   Code Review   (7) │
└─────────────────────┘
```

- Press number key (1-9) to select → picker closes, default mode changes
- Press Escape or click outside → cancel
- Picker disappears after selection, tray icon tooltip shows current mode name

### Settings UI for shortcuts

In Settings, allow customizing:
- Recording shortcut (key combo picker)
- Mode switch shortcut (key combo picker)
- Option override behavior (which mode does Option trigger)
- Control override behavior (which mode does Control trigger)

Do NOT allow per-mode individual shortcuts — this leads to shortcut explosion.

## 4. Config File

All settings persist in `~/.fonos/config.json`:

```json
{
  "shortcuts": {
    "record": "cmd+shift+space",
    "mode_switch": "cmd+shift+m",
    "modifier_option": "raw",
    "modifier_control": "translate_auto"
  },
  "default_mode": "polish",
  "llm": {
    "provider": "omlx",
    "base_url": "http://localhost:8000/v1",
    "model": "Qwen3-4B-Instruct-2507-MLX-6bit",
    "api_key": ""
  },
  "stt": {
    "provider": "omlx",
    "base_url": "http://localhost:8000/v1",
    "model": "Qwen3-ASR-0.6B-4bit"
  }
}
```

`translate_auto` means: detect input language, if CJK → translate to English, else → translate to Chinese.

## 5. Floating Indicator

During and after recording, show a small floating indicator near the cursor:

- **Recording**: red dot + waveform animation + current mode name
- **Processing STT**: spinner + "Transcribing..."
- **Processing LLM**: spinner + "Polishing..." (or mode name)
- **Done**: green check, auto-dismiss after 0.5s
- **Error**: red X + brief error message, dismiss on click

The indicator should never steal focus from the active app.

## What NOT to do

- Do not add per-mode shortcut keys
- Do not require user to configure prompt strategy (system vs merged) — auto-detect handles this
- Do not make the mode picker a full settings panel — it's a quick floating list, keyboard-driven
- Do not add mode switching to the tray menu — tray shows current mode but switching is via Cmd+Shift+M only
- Do not store model probe results inside config.json — separate file (model_caps.json) so config stays clean
