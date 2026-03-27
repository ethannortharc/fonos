# Fonos: Persistent Stats & History

## Goal

Replace the current Status page with a **Stats** (统计) page, and make **History** persistent. Both backed by a local SQLite database, giving users visibility into their voice productivity gains.

## Why

- Current Status page shows service health (already removed) — provides no user value
- Current History is in-memory, lost on app restart — user effort is discarded
- Users want to see: "How much time did voice input save me this week?"

## Database

**SQLite** via `rusqlite` crate, stored at `~/Library/Application Support/com.fonos.app/fonos.db`.

### Schema

```sql
CREATE TABLE events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    type        TEXT NOT NULL,          -- 'stt', 'tts', 'llm'
    created_at  TEXT NOT NULL,          -- ISO 8601
    date        TEXT NOT NULL,          -- 'YYYY-MM-DD' for grouping
    input_text  TEXT DEFAULT '',        -- raw STT text / TTS input / LLM input
    output_text TEXT DEFAULT '',        -- processed text (LLM output, empty for raw STT)
    words_in    INTEGER DEFAULT 0,      -- word count of input
    words_out   INTEGER DEFAULT 0,      -- word count of output
    duration_secs REAL DEFAULT 0,       -- recording length (STT) or audio length (TTS)
    latency_ms  INTEGER DEFAULT 0,      -- API response time
    mode        TEXT DEFAULT '',        -- dictation mode used
    model       TEXT DEFAULT '',        -- model name
    voice       TEXT DEFAULT '',        -- TTS voice ID
    audio_path  TEXT DEFAULT ''         -- path to audio file
);

CREATE INDEX idx_events_date ON events(date);
CREATE INDEX idx_events_type ON events(type);

-- Convenience view for daily aggregation
CREATE VIEW daily_stats AS
SELECT
    date,
    COUNT(*) FILTER (WHERE type = 'stt') AS stt_count,
    COALESCE(SUM(duration_secs) FILTER (WHERE type = 'stt'), 0) AS stt_seconds,
    COALESCE(SUM(words_in) FILTER (WHERE type = 'stt'), 0) AS stt_words,
    COUNT(*) FILTER (WHERE type = 'tts') AS tts_count,
    COALESCE(SUM(words_out) FILTER (WHERE type = 'tts'), 0) AS tts_words,
    COUNT(*) FILTER (WHERE type = 'llm') AS llm_count,
    COALESCE(SUM(latency_ms) FILTER (WHERE type = 'llm'), 0) AS llm_latency_total,
    -- Time saved estimate: voice input ~150 WPM vs typing ~40 WPM
    -- Each dictated word saves ~1.1 seconds vs typing
    ROUND(COALESCE(SUM(words_in) FILTER (WHERE type = 'stt'), 0) * 1.1, 0) AS time_saved_secs
FROM events
GROUP BY date;
```

Note: SQLite doesn't support FILTER syntax. Use `SUM(CASE WHEN type='stt' THEN ... END)` instead. The view above is conceptual.

### Rust Module: `src/stats.rs`

```rust
// Responsibilities:
// 1. Initialize DB + run migrations on app start
// 2. record_event(type, input, output, duration, latency, mode, model, voice, audio_path)
// 3. get_daily_stats(date_from, date_to) -> Vec<DailyStat>
// 4. get_events(limit, offset, type_filter) -> Vec<Event>
// 5. get_today_summary() -> TodaySummary
```

### Tauri Commands

| Command | Purpose |
|---------|---------|
| `record_event` | Insert an event (called after STT/TTS/LLM completes) |
| `get_stats` | Return daily stats for a date range |
| `get_history` | Paginated event list with optional type filter |
| `get_today` | Quick summary for the stats dashboard header |

### Instrumentation Points

Where to call `record_event`:

1. **STT** — `stop_recording()` in dictation.rs, after successful transcription
   - type: "stt", input_text: transcript, duration_secs, latency_ms

2. **LLM** — `process_with_llm()` in llm.rs, after successful processing
   - type: "llm", input_text: original, output_text: processed, mode, latency_ms

3. **TTS** — `generate_and_play()` in tts.rs, after successful generation
   - type: "tts", input_text: text, duration_secs, voice

## Stats Page Design

### Today Card (top)
Three metric tiles:
- **Time Saved** — estimated seconds saved by voice vs typing (large number, e.g. "12 min")
- **Words** — total words dictated + generated today
- **Sessions** — total STT + TTS + LLM calls today

### Weekly Chart (middle)
Simple bar chart (7 bars, Mon-Sun) showing daily word count or time saved.
Pure CSS/canvas, no charting library.

### Breakdown (bottom)
Per-type stats with progress bars:
- STT: N recordings, N words, N seconds of audio
- TTS: N generations, N words
- LLM: N calls, avg latency

### Period Selector
Toggle: Today / This Week / This Month / All Time

## History Page Enhancement

### Changes from current in-memory list:
- Load from SQLite with pagination (50 per page, scroll to load more)
- Filter tabs: All / STT / TTS
- Each entry shows: time, type badge, text preview, duration/latency
- Click to expand: full text, audio path, mode used
- Persist across app restarts

## Implementation Approach

### Work Packages

1. **WP-1: SQLite foundation** — Add `rusqlite` dependency, create `stats.rs` module with DB init + migrations + CRUD functions. Add `record_event` / `get_stats` / `get_history` / `get_today` commands. Register in main.rs.

2. **WP-2: Instrument existing flows** — Add `record_event` calls to `stop_recording`, `process_with_llm`, and `generate_and_play`. Wire up word counting.

3. **WP-3: Stats view** — Replace `status.js` with `stats.js`. Today card, weekly chart, breakdown section. Rename tab "Status" → "Stats" in index.html and app.js.

4. **WP-4: History persistence** — Rewrite `history.js` to load from DB via `get_history` command. Add pagination, type filter, expand/collapse. Remove in-memory array.

5. **WP-5: API bridge** — Update `api.js` with new commands. Remove old server status commands if unused.

### Dependencies
- rusqlite = "0.31" (with bundled feature for zero-config SQLite)
- WP-1 → WP-2 → WP-3 (parallel with WP-4) → WP-5
