# Security Policy

## Scope

Fonos is a local desktop app (Tauri, macOS + Linux) with no server component
of its own. Configuration, API keys, History, notebooks, and generated
artifacts are stored locally. The app talks directly to whatever STT, LLM, and
TTS endpoints you configure — cloud providers (OpenAI, Anthropic, Google,
OpenRouter, ...) or local/self-hosted servers (Ollama, LM Studio, OMLX,
vLLM, ...). There is no Fonos-operated backend that handles your data.

## Reporting a vulnerability

Please report security issues privately via GitHub's
[Security Advisories](https://github.com/ethannortharc/fonos/security/advisories/new)
for this repository — **not** as a public issue. This lets us assess and fix
the problem before it's disclosed.

Include what you found, steps to reproduce, and the affected version. We'll
acknowledge reports as soon as we can and follow up as the fix progresses.

## Supported versions

Only the latest [release](https://github.com/ethannortharc/fonos/releases/latest)
is supported. Please update before reporting, if practical, to confirm the
issue still applies.
