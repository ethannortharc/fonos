# Fonos — Intent 1: Voice API Server

## Goal

Build a local Python voice API server that provides STT (FunASR) and TTS (CosyVoice 3) capabilities through an OpenAI-compatible HTTP API. Runs on macOS (Apple Silicon, M4 Pro 64GB). Serves as the backend for both the Fonos GUI app (Intent 2) and external consumers like OpenClaw/MateClaw.

## Environment

- **Runtime**: Python 3.10+, conda environment `voxclaw`
- **Platform**: macOS arm64 (Apple Silicon), CPU inference
- **Models**:
  - STT: `FunAudioLLM/Fun-ASR-Nano-2512` (primary) or `iic/speech_paraformer-large-vad-punc_asr_nat-zh-cn-16k-common-vocab8404-pytorch` (fallback)
  - TTS: `FunAudioLLM/Fun-CosyVoice3-0.5B-2512`
- **Framework**: FastAPI + uvicorn
- **Port**: 9880 (default, configurable via `VOXCLAW_PORT`)

## API Endpoints

### STT

```
POST /v1/audio/transcriptions
Content-Type: multipart/form-data

file: <audio file> (wav/mp3/flac/m4a)
model: "funasir-nano" (optional, default)
language: "zh" (optional, default)
response_format: "json" (optional)
hotwords: "专有名词1 专有名词2" (optional, space-separated)
```

Response:
```json
{
  "text": "识别出的文本，带标点。",
  "segments": [
    {"start": 0.0, "end": 2.5, "text": "识别出的文本，"},
    {"start": 2.5, "end": 4.0, "text": "带标点。"}
  ],
  "language": "zh",
  "duration": 4.0
}
```

### STT Streaming (WebSocket)

```
WS /v1/audio/transcriptions/stream

Client sends: binary audio chunks (PCM 16kHz 16bit mono)
Server sends: JSON {"text": "...", "is_final": false/true}
```

### TTS

```
POST /v1/audio/speech
Content-Type: application/json

{
  "input": "要合成的文本",
  "voice": "default",           // voice_id or "default"
  "model": "cosyvoice3",       // optional
  "response_format": "wav",     // wav/mp3
  "speed": 1.0,                // 0.5-2.0
  "instruction": ""            // optional: emotion/style instruct prompt
}
```

Response: audio bytes (streaming chunked transfer)

### Zero-Shot Voice Clone

```
POST /v1/audio/voices/clone
Content-Type: multipart/form-data

file: <reference audio> (3-10s, wav/mp3)
name: "my_voice"
description: "optional description"
```

Response:
```json
{
  "voice_id": "voice_abc123",
  "name": "my_voice",
  "status": "ready"
}
```

### Voice Management

```
GET  /v1/audio/voices          → list all voices (built-in + cloned)
GET  /v1/audio/voices/{id}     → voice detail
DELETE /v1/audio/voices/{id}   → delete cloned voice

GET  /v1/health                → service health + model load status
```

## Directory Structure

```
voxclaw/
├── server.py              # FastAPI app, uvicorn entry
├── stt/
│   ├── engine.py          # FunASR model loading + inference
│   └── streaming.py       # WebSocket streaming handler
├── tts/
│   ├── engine.py          # CosyVoice 3 model loading + inference
│   ├── voice_store.py     # Voice registry (cloned voices persisted to SQLite)
│   └── clone.py           # Zero-shot voice cloning logic
├── config.py              # Env-based config (port, model paths, device)
├── models/                # Downloaded model weights (gitignored)
├── voices.db              # SQLite — cloned voice metadata + reference audio paths
├── voice_refs/            # Stored reference audio files
├── requirements.txt
├── setup.sh               # One-shot: create conda env, install deps, download models
├── tests/
│   ├── conftest.py        # Server fixture (startup/shutdown)
│   ├── test_stt.py        # STT endpoint tests
│   ├── test_tts.py        # TTS endpoint tests
│   ├── test_clone.py      # Voice clone + management tests
│   ├── test_streaming.py  # WebSocket STT tests
│   ├── test_health.py     # Health endpoint
│   └── fixtures/
│       ├── zh_sample.wav  # 5s Mandarin test audio
│       ├── en_sample.wav  # 5s English test audio
│       └── ref_voice.wav  # 5s reference for clone test
└── README.md
```

## Invariants (automated, must all pass)

1. **Server starts**: `uvicorn voxclaw.server:app --port 9880` starts without error; `GET /v1/health` returns 200 with `{"status": "ok", "models": {"stt": "loaded", "tts": "loaded"}}` within 120s of launch.

2. **STT produces text**: `POST /v1/audio/transcriptions` with `zh_sample.wav` returns 200, response JSON has non-empty `text` field, `duration` > 0, `segments` is a non-empty array.

3. **STT Chinese accuracy**: Recognized text from `zh_sample.wav` contains at least 80% of expected key phrases (fixture includes ground truth).

4. **TTS produces audio**: `POST /v1/audio/speech` with `{"input": "你好世界", "voice": "default"}` returns 200, response body is valid WAV (starts with RIFF header), audio duration > 0.5s.

5. **TTS streaming**: Response uses chunked transfer-encoding; first chunk arrives within 10s of request.

6. **Voice clone roundtrip**: Upload `ref_voice.wav` via `/v1/audio/voices/clone` → returns `voice_id` → use that `voice_id` in `/v1/audio/speech` → returns valid audio. Then `GET /v1/audio/voices` includes the new voice. Then `DELETE` it → `GET` no longer includes it.

7. **WebSocket STT**: Connect to `/v1/audio/transcriptions/stream`, send PCM chunks from `zh_sample.wav` in 200ms segments, receive at least one `{"text": "...", "is_final": true}` message with non-empty text.

8. **Error handling**: Invalid audio file (random bytes) to STT → 400 with error message. Empty `input` to TTS → 422. Non-existent `voice_id` to TTS → 404.

9. **Concurrent requests**: 3 simultaneous STT requests all return 200 (may be sequential internally, but no crashes or deadlocks).

10. **macOS compatible**: All dependencies install on arm64 macOS without compilation errors. No CUDA-only code paths. No Linux-only system calls. `setup.sh` runs clean on a fresh conda env.

## Quality Dimensions (automated, measured)

| Dimension | Metric | Target | Measurement |
|-----------|--------|--------|-------------|
| STT latency | Time from request to response (5s audio) | < 3s | pytest-benchmark on `zh_sample.wav` |
| TTS latency | Time to first audio chunk (10 chars) | < 8s | Timer in test |
| TTS full generation | Total time for 50-char sentence | < 15s | Timer in test |
| Memory baseline | RSS after model load, idle | < 8GB | `psutil.Process().memory_info().rss` |
| Memory under load | RSS during concurrent STT+TTS | < 12GB | psutil during load test |
| API compatibility | OpenAI SDK `client.audio.transcriptions.create()` works | Pass | Test with `openai` Python package pointed at localhost |

## Preferences (human review queue)

1. **Audio quality**: TTS output for Chinese and English sentences should sound natural, not robotic. (Play samples during review.)

2. **Voice clone fidelity**: Cloned voice should be recognizably similar to the reference audio. (A/B comparison during review.)

3. **Code organization**: Clean separation between STT/TTS engines and HTTP layer. Engine classes should be reusable outside FastAPI (for future Tauri integration via PyO3 or subprocess).

4. **Startup UX**: `setup.sh` should print clear progress — model download can take minutes, user should see what's happening.

5. **Logging**: Structured JSON logs to stdout. Request IDs. Model inference timing logged at INFO level.

## Implementation Notes

- **Model loading**: Lazy-load on first request OR eager-load at startup (configurable). Default: eager. Models stay in memory permanently — no unloading between requests.

- **Device**: Force `device="cpu"` everywhere. Do NOT attempt MPS — too many PyTorch ops fall back silently and cause hangs. Comment the MPS path for future enablement.

- **CosyVoice macOS**: Skip `ttsfrd` package (Linux-only wheel). Use `WeTextProcessing` for text normalization instead. The setup.sh should handle this gracefully.

- **Audio format handling**: Use `torchaudio` or `soundfile` for input decoding. Accept wav/mp3/flac/m4a. Normalize to 16kHz mono PCM internally.

- **Voice storage**: SQLite via `sqlite3` stdlib (no ORM). Schema: `voices(id TEXT PK, name TEXT, description TEXT, ref_audio_path TEXT, created_at TEXT)`. Reference audio files stored in `voice_refs/` directory.

- **OpenAI compatibility**: The endpoint paths and request/response formats should work with the official `openai` Python SDK when configured with `base_url="http://localhost:9880/v1"` and a dummy API key.

- **Graceful shutdown**: Handle SIGTERM — finish in-flight requests, then exit.

## Verification Strategy

All invariants and quality dimensions are tested via `pytest` with a server fixture that starts uvicorn in a subprocess, waits for health check, runs tests, then shuts down. No manual steps required.

```bash
# Full verification cycle
cd voxclaw
./setup.sh                    # One-time: env + deps + models
pytest tests/ -v --tb=short   # All automated checks
```

Test audio fixtures are generated programmatically in `conftest.py` using TTS (for STT round-trip testing) or bundled as small wav files committed to the repo.
