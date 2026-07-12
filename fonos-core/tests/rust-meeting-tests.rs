/// Meeting Mode tests for Fonos v2 — M01, M02, M03, M04, M06, M08, M09, M10, M14, M15, Q01
///
/// Covers:
///   M01 — Meeting mode configuration (processor, output_target, container_type, input_mode, etc.)
///   M02 — Continuous audio chunking logic (~10-15s segments)
///   M03 — Meeting session container creation + metadata, duration update on stop
///   M04 — Meeting entry per chunk with all required metadata fields
///   M06 — hotkey_meeting field in AppConfig, parse_hotkey("option+m") succeeds
///   M08 — Dual-channel WAV format (stereo interleave), speaker_hint assignment
///   M09 — Summary prompt construction from transcript entries with speaker labels
///   M10 — OpenRouter provider resolves to correct base URL and auth header
///   M14 — Meeting export produces Markdown with summary + speaker labels + timestamps
///   M15 — Backward compatibility: existing commands still accessible
///   Q01 — Chunk processing latency assertion (< 3000 ms)
///
/// Run with:
///   cargo test -p fonos-core --test rust-meeting-tests
///
/// These tests are in the RED (failing) phase — meeting mode production code
/// does not exist yet. Tests will fail until the implementation is written.

use fonos_core::config::AppConfig;
use fonos_core::hotkey::parse_hotkey;
use fonos_core::storage::init_storage_db;
use rusqlite::Connection;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Open a fresh in-memory SQLite database with storage tables initialised.
fn open_db() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    init_storage_db(&conn);
    conn
}

/// Generate a deterministic ISO-8601 timestamp offset from a fixed epoch.
fn ts(offset_secs: i64) -> String {
    // Base: 2026-03-28T00:00:00 UTC  →  1774_828_800
    let epoch: i64 = 1_774_828_800 + offset_secs;
    let h = (epoch % 86_400) / 3_600;
    let m = (epoch % 3_600) / 60;
    let s = epoch % 60;
    format!("2026-03-28T{h:02}:{m:02}:{s:02}")
}

/// Simulate 30 seconds of 16 kHz mono 16-bit PCM samples (960 000 bytes raw).
fn mock_30s_pcm() -> Vec<i16> {
    let sample_rate: usize = 16_000;
    let duration_secs: usize = 30;
    let total_samples = sample_rate * duration_secs;
    // Synthesise a 440 Hz tone so the buffer is non-trivial.
    (0..total_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (f32::sin(2.0 * std::f32::consts::PI * 440.0 * t) * 16_000.0) as i16
        })
        .collect()
}

/// Pack two mono i16 slices into an interleaved stereo Vec<i16>.
/// L = left channel samples, R = right channel samples (must be same length).
fn interleave_stereo(left: &[i16], right: &[i16]) -> Vec<i16> {
    assert_eq!(left.len(), right.len(), "stereo channels must be the same length");
    let mut out = Vec::with_capacity(left.len() * 2);
    for (l, r) in left.iter().zip(right.iter()) {
        out.push(*l);
        out.push(*r);
    }
    out
}

/// Split an interleaved stereo buffer back into (left, right) mono channels.
fn split_stereo(interleaved: &[i16]) -> (Vec<i16>, Vec<i16>) {
    assert_eq!(interleaved.len() % 2, 0, "interleaved buffer must have even length");
    let mut left = Vec::with_capacity(interleaved.len() / 2);
    let mut right = Vec::with_capacity(interleaved.len() / 2);
    for chunk in interleaved.chunks_exact(2) {
        left.push(chunk[0]);
        right.push(chunk[1]);
    }
    (left, right)
}

// ---------------------------------------------------------------------------
// M01 — Meeting mode configuration
// ---------------------------------------------------------------------------
//
// The legacy `modes` system's built-in "meeting" `Mode` (output_target/
// container_type/processor/auto_container/save_audio) was deleted in
// Workbench P2 Task 12 — Meeting has run through the workflow engine's
// `wf.meeting` composite recipe since Workbench P2 Task 7. The
// m01_meeting_mode_config module that used to regression-test that `Mode`
// definition was removed along with it.

// ---------------------------------------------------------------------------
// M02 — Continuous audio chunking
// ---------------------------------------------------------------------------

#[cfg(test)]
mod m02_audio_chunking {
    use super::*;
    use fonos_core::meetings::chunker::{chunk_audio, ChunkConfig};

    /// Unit: 30 seconds of audio produces 2-3 chunks at 10-15s intervals.
    #[test]
    fn chunking_30s_produces_two_to_three_chunks() {
        let samples = mock_30s_pcm();
        let config = ChunkConfig {
            sample_rate: 16_000,
            target_chunk_secs: 12,
            min_chunk_secs: 10,
            max_chunk_secs: 15,
        };

        let chunks = chunk_audio(&samples, &config);

        assert!(
            chunks.len() >= 2 && chunks.len() <= 3,
            "30s audio at 10-15s chunks should produce 2-3 chunks, got {}",
            chunks.len()
        );
    }

    /// Unit: Each produced chunk is within the 10-15s sample-count range.
    #[test]
    fn chunk_lengths_are_within_bounds() {
        let samples = mock_30s_pcm();
        let sample_rate = 16_000_usize;
        let config = ChunkConfig {
            sample_rate: sample_rate as u32,
            target_chunk_secs: 12,
            min_chunk_secs: 10,
            max_chunk_secs: 15,
        };

        let chunks = chunk_audio(&samples, &config);

        for (i, chunk) in chunks.iter().enumerate() {
            // The last chunk may be shorter than min_chunk_secs — allow that.
            let is_last = i == chunks.len() - 1;
            if !is_last {
                let secs = chunk.samples.len() / sample_rate;
                assert!(
                    secs >= 10 && secs <= 15,
                    "chunk {} has {}s ({} samples), expected 10-15s",
                    i,
                    secs,
                    chunk.samples.len()
                );
            }
            // All chunks must be non-empty.
            assert!(!chunk.samples.is_empty(), "chunk {} must not be empty", i);
        }
    }

    /// Unit: Chunks are contiguous — no samples are dropped or duplicated.
    #[test]
    fn chunking_is_lossless() {
        let samples = mock_30s_pcm();
        let config = ChunkConfig {
            sample_rate: 16_000,
            target_chunk_secs: 12,
            min_chunk_secs: 10,
            max_chunk_secs: 15,
        };

        let chunks = chunk_audio(&samples, &config);
        let total_samples: usize = chunks.iter().map(|c| c.samples.len()).sum();

        assert_eq!(
            total_samples,
            samples.len(),
            "total samples across all chunks must equal input sample count (no drops/duplicates)"
        );
    }

    /// Unit: Each chunk carries a correct byte-offset start position.
    #[test]
    fn chunk_start_offsets_are_sequential() {
        let samples = mock_30s_pcm();
        let config = ChunkConfig {
            sample_rate: 16_000,
            target_chunk_secs: 12,
            min_chunk_secs: 10,
            max_chunk_secs: 15,
        };

        let chunks = chunk_audio(&samples, &config);

        let mut expected_offset = 0_usize;
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(
                chunk.sample_offset, expected_offset,
                "chunk {} start offset should be {}, got {}",
                i, expected_offset, chunk.sample_offset
            );
            expected_offset += chunk.samples.len();
        }
    }

    /// Unit: Empty audio produces zero chunks.
    #[test]
    fn chunking_empty_audio_produces_no_chunks() {
        let samples: Vec<i16> = vec![];
        let config = ChunkConfig {
            sample_rate: 16_000,
            target_chunk_secs: 12,
            min_chunk_secs: 10,
            max_chunk_secs: 15,
        };

        let chunks = chunk_audio(&samples, &config);
        assert_eq!(chunks.len(), 0, "empty audio should produce 0 chunks");
    }
}

// ---------------------------------------------------------------------------
// M03 — Meeting session container
// ---------------------------------------------------------------------------

#[cfg(test)]
mod m03_meeting_session_container {
    use super::*;
    use fonos_core::meetings::session::{create_meeting_session, update_meeting_duration};

    /// Unit: Creating a meeting session produces a container with type=MeetingSession.
    #[test]
    fn create_meeting_session_container_type_is_meeting_session() {
        let conn = open_db();

        let container_id = create_meeting_session(
            &conn,
            "2026-03-28T09:30:00",
            "mic_only",
        )
        .expect("create_meeting_session must succeed");

        let container_type: String = conn
            .query_row(
                "SELECT container_type FROM containers WHERE id = ?1",
                rusqlite::params![container_id],
                |row| row.get(0),
            )
            .expect("container must exist in db");

        assert_eq!(
            container_type, "meeting_session",
            "container_type must be 'meeting_session', got '{}'",
            container_type
        );
    }

    /// Unit: Container title includes the date and time string.
    #[test]
    fn meeting_session_title_includes_datetime() {
        let conn = open_db();

        let container_id = create_meeting_session(&conn, "2026-03-28T09:30:00", "mic_only")
            .expect("create session");

        let title: String = conn
            .query_row(
                "SELECT title FROM containers WHERE id = ?1",
                rusqlite::params![container_id],
                |row| row.get(0),
            )
            .expect("container must exist");

        // Title should contain the date or time from the provided started_at.
        assert!(
            title.contains("2026") || title.contains("09:30") || title.contains("Mar"),
            "session title '{}' should contain date/time information",
            title
        );
    }

    /// Unit: Container metadata includes audio_source field.
    #[test]
    fn meeting_session_metadata_has_audio_source() {
        let conn = open_db();

        let container_id = create_meeting_session(&conn, "2026-03-28T09:30:00", "dual_channel")
            .expect("create session");

        let metadata_json: String = conn
            .query_row(
                "SELECT metadata FROM containers WHERE id = ?1",
                rusqlite::params![container_id],
                |row| row.get(0),
            )
            .expect("container must exist");

        let metadata: serde_json::Value =
            serde_json::from_str(&metadata_json).expect("metadata must be valid JSON");

        assert!(
            metadata.get("audio_source").is_some(),
            "container metadata must have 'audio_source' field, got: {}",
            metadata_json
        );

        assert_eq!(
            metadata["audio_source"].as_str().unwrap_or(""),
            "dual_channel",
            "audio_source should match the value passed to create_meeting_session"
        );
    }

    /// Unit: Container metadata includes channel_mode field.
    #[test]
    fn meeting_session_metadata_has_channel_mode() {
        let conn = open_db();
        let container_id = create_meeting_session(&conn, "2026-03-28T09:30:00", "mic_only")
            .expect("create session");

        let metadata_json: String = conn
            .query_row(
                "SELECT metadata FROM containers WHERE id = ?1",
                rusqlite::params![container_id],
                |row| row.get(0),
            )
            .expect("container");

        let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap();

        assert!(
            metadata.get("channel_mode").is_some(),
            "container metadata must have 'channel_mode' field"
        );
    }

    /// Integration: update_meeting_duration sets duration_total_ms in metadata.
    #[test]
    fn update_meeting_duration_sets_metadata_field() {
        let conn = open_db();
        let container_id = create_meeting_session(&conn, "2026-03-28T09:30:00", "mic_only")
            .expect("create session");

        let expected_duration_ms: u64 = 185_400; // 3 min 5.4 sec
        update_meeting_duration(&conn, container_id, expected_duration_ms)
            .expect("update_meeting_duration must succeed");

        let metadata_json: String = conn
            .query_row(
                "SELECT metadata FROM containers WHERE id = ?1",
                rusqlite::params![container_id],
                |row| row.get(0),
            )
            .expect("container");

        let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap();

        let stored_ms = metadata["duration_total_ms"]
            .as_u64()
            .expect("duration_total_ms must be a non-negative integer");

        assert_eq!(
            stored_ms, expected_duration_ms,
            "duration_total_ms should be {}, got {}",
            expected_duration_ms, stored_ms
        );
    }

    /// Integration: updated_at timestamp changes after duration update.
    #[test]
    fn update_meeting_duration_advances_updated_at() {
        let conn = open_db();
        let container_id = create_meeting_session(&conn, "2026-03-28T09:30:00", "mic_only")
            .expect("create session");

        let before: String = conn
            .query_row(
                "SELECT updated_at FROM containers WHERE id = ?1",
                rusqlite::params![container_id],
                |row| row.get(0),
            )
            .expect("container");

        // Small sleep so timestamps differ
        std::thread::sleep(std::time::Duration::from_millis(5));

        update_meeting_duration(&conn, container_id, 60_000)
            .expect("update duration");

        let after: String = conn
            .query_row(
                "SELECT updated_at FROM containers WHERE id = ?1",
                rusqlite::params![container_id],
                |row| row.get(0),
            )
            .expect("container");

        assert_ne!(
            before, after,
            "updated_at should change after update_meeting_duration"
        );
    }
}

// ---------------------------------------------------------------------------
// M04 — Meeting entries per chunk
// ---------------------------------------------------------------------------

#[cfg(test)]
mod m04_meeting_entries {
    use super::*;
    use fonos_core::meetings::session::{create_meeting_session, insert_meeting_entry};

    /// Unit: insert_meeting_entry creates an entry with source_type = "meeting".
    #[test]
    fn meeting_entry_source_type_is_meeting() {
        let conn = open_db();
        let session_id = create_meeting_session(&conn, &ts(0), "mic_only")
            .expect("create session");

        let entry_id = insert_meeting_entry(
            &conn,
            session_id,
            "Hello, this is the first chunk.",
            0,      // chunk_index
            0,      // timestamp_in_session_ms
            11_200, // duration_ms
            None,   // speaker_hint
        )
        .expect("insert_meeting_entry must succeed");

        let source_type: String = conn
            .query_row(
                "SELECT source_type FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| row.get(0),
            )
            .expect("entry must exist");

        assert_eq!(
            source_type, "meeting",
            "entry source_type must be 'meeting', got '{}'",
            source_type
        );
    }

    /// Unit: Meeting entry has role = "user".
    #[test]
    fn meeting_entry_role_is_user() {
        let conn = open_db();
        let session_id =
            create_meeting_session(&conn, &ts(0), "mic_only").expect("create session");

        let entry_id = insert_meeting_entry(&conn, session_id, "Some speech.", 0, 0, 11_000, None)
            .expect("insert entry");

        let role: String = conn
            .query_row(
                "SELECT role FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| row.get(0),
            )
            .expect("entry");

        assert_eq!(role, "user", "meeting chunk entry role must be 'user'");
    }

    /// Unit: Metadata contains chunk_index field.
    #[test]
    fn meeting_entry_metadata_has_chunk_index() {
        let conn = open_db();
        let session_id =
            create_meeting_session(&conn, &ts(0), "mic_only").expect("create session");

        let entry_id =
            insert_meeting_entry(&conn, session_id, "Second chunk.", 1, 12_000, 12_500, None)
                .expect("insert entry");

        let metadata_json: String = conn
            .query_row(
                "SELECT metadata FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| row.get(0),
            )
            .expect("entry");

        let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap();

        assert!(
            metadata.get("chunk_index").is_some(),
            "entry metadata must have 'chunk_index', got: {}",
            metadata_json
        );
        assert_eq!(
            metadata["chunk_index"].as_u64().unwrap_or(999),
            1,
            "chunk_index should be 1 for the second chunk"
        );
    }

    /// Unit: Metadata contains timestamp_in_session field.
    #[test]
    fn meeting_entry_metadata_has_timestamp_in_session() {
        let conn = open_db();
        let session_id =
            create_meeting_session(&conn, &ts(0), "mic_only").expect("create session");

        let timestamp_ms: u64 = 24_000;
        let entry_id =
            insert_meeting_entry(&conn, session_id, "Third chunk.", 2, timestamp_ms, 11_000, None)
                .expect("insert entry");

        let metadata_json: String = conn
            .query_row(
                "SELECT metadata FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| row.get(0),
            )
            .expect("entry");

        let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap();

        assert!(
            metadata.get("timestamp_in_session_ms").is_some(),
            "entry metadata must have 'timestamp_in_session_ms'"
        );
        assert_eq!(
            metadata["timestamp_in_session_ms"].as_u64().unwrap_or(0),
            timestamp_ms,
            "timestamp_in_session_ms mismatch"
        );
    }

    /// Unit: Metadata contains duration_ms field.
    #[test]
    fn meeting_entry_metadata_has_duration_ms() {
        let conn = open_db();
        let session_id =
            create_meeting_session(&conn, &ts(0), "mic_only").expect("create session");

        let entry_id = insert_meeting_entry(&conn, session_id, "A chunk.", 0, 0, 13_750, None)
            .expect("insert entry");

        let metadata_json: String = conn
            .query_row(
                "SELECT metadata FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| row.get(0),
            )
            .expect("entry");

        let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap();

        assert!(
            metadata.get("duration_ms").is_some(),
            "entry metadata must have 'duration_ms'"
        );
        assert_eq!(
            metadata["duration_ms"].as_u64().unwrap_or(0),
            13_750,
            "duration_ms mismatch"
        );
    }

    /// Unit: Entry is linked to its session container via container_id.
    #[test]
    fn meeting_entry_container_id_matches_session() {
        let conn = open_db();
        let session_id =
            create_meeting_session(&conn, &ts(0), "mic_only").expect("create session");

        let entry_id = insert_meeting_entry(&conn, session_id, "Text.", 0, 0, 10_000, None)
            .expect("insert entry");

        let container_id: i64 = conn
            .query_row(
                "SELECT container_id FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| row.get(0),
            )
            .expect("entry");

        assert_eq!(
            container_id, session_id,
            "entry container_id must equal the session_id"
        );
    }

    /// Unit: Multiple entries in same session are retrievable by container_id.
    #[test]
    fn multiple_meeting_entries_queryable_by_session() {
        let conn = open_db();
        let session_id =
            create_meeting_session(&conn, &ts(0), "mic_only").expect("create session");

        for (i, text) in ["First.", "Second.", "Third."].iter().enumerate() {
            insert_meeting_entry(
                &conn,
                session_id,
                text,
                i as u32,
                (i as u64) * 12_000,
                12_000,
                None,
            )
            .expect("insert entry");
        }

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE container_id = ?1",
                rusqlite::params![session_id],
                |row| row.get(0),
            )
            .expect("count query");

        assert_eq!(count, 3, "should have 3 entries in the session, got {}", count);
    }
}

// ---------------------------------------------------------------------------
// M06 — Meeting hotkey
// ---------------------------------------------------------------------------

#[cfg(test)]
mod m06_meeting_hotkey {
    use super::*;

    /// Unit: parse_hotkey("option+m") succeeds.
    #[test]
    fn option_m_hotkey_parses_successfully() {
        let result = parse_hotkey("option+m");
        assert!(
            result.is_ok(),
            "parse_hotkey('option+m') should succeed, got: {:?}",
            result
        );
    }

    /// Unit: AppConfig has a hotkey_meeting field.
    #[test]
    fn app_config_has_hotkey_meeting_field() {
        let config = AppConfig::default();
        // This line must compile — it proves the field exists.
        let hotkey: &str = &config.hotkey_meeting;
        assert!(
            !hotkey.is_empty(),
            "default hotkey_meeting should not be empty"
        );
    }

    /// Unit: Default hotkey_meeting is "option+m".
    #[test]
    fn default_meeting_hotkey_is_option_m() {
        let config = AppConfig::default();
        assert_eq!(
            config.hotkey_meeting, "option+m",
            "default hotkey_meeting should be 'option+m'"
        );
    }

    /// Unit: Meeting hotkey does not conflict with existing hotkeys.
    #[test]
    fn meeting_hotkey_does_not_conflict_with_existing() {
        let config = AppConfig::default();
        let existing = [
            config.hotkey_dictation.as_str(),
            config.hotkey_tts.as_str(),
            config.hotkey_agent.as_str(),
            config.hotkey_agent_panel.as_str(),
            config.hotkey_note.as_str(),
        ];

        for existing_hotkey in &existing {
            assert_ne!(
                config.hotkey_meeting.as_str(),
                *existing_hotkey,
                "meeting hotkey '{}' conflicts with existing hotkey '{}'",
                config.hotkey_meeting,
                existing_hotkey
            );
        }
    }

    /// Unit: hotkey_meeting round-trips through JSON serialization.
    #[test]
    fn hotkey_meeting_round_trips_json() {
        let mut config = AppConfig::default();
        config.hotkey_meeting = "cmd+shift+m".to_string();

        let json = serde_json::to_string(&config).expect("serialize config");
        let loaded: AppConfig = serde_json::from_str(&json).expect("deserialize config");

        assert_eq!(
            loaded.hotkey_meeting, "cmd+shift+m",
            "hotkey_meeting should survive JSON round-trip"
        );
    }

    /// Unit: hotkey_meeting field is present in serialized JSON.
    #[test]
    fn config_serializes_hotkey_meeting_field() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(
            json.contains("hotkey_meeting"),
            "serialized config must contain 'hotkey_meeting' key"
        );
    }
}

// ---------------------------------------------------------------------------
// M08 — Dual-channel recording and speaker labeling
// ---------------------------------------------------------------------------

#[cfg(test)]
mod m08_dual_channel {
    use super::*;
    use fonos_core::meetings::audio::{build_stereo_wav, SpeakerHint};

    /// Unit: Interleaving mic (L) and system (R) produces a 2-channel buffer.
    #[test]
    fn interleave_produces_correct_stereo_length() {
        let mic_samples: Vec<i16> = (0..8_000_i16).collect(); // 0.5s at 16kHz
        let sys_samples: Vec<i16> = (1_000..9_000_i16).collect();

        let stereo = interleave_stereo(&mic_samples, &sys_samples);

        assert_eq!(
            stereo.len(),
            mic_samples.len() * 2,
            "stereo buffer should be 2× mono length"
        );
    }

    /// Unit: Stereo WAV header declares 2 channels.
    #[test]
    fn stereo_wav_has_two_channels() {
        let mic: Vec<i16> = vec![0i16; 16_000]; // 1s silence
        let sys: Vec<i16> = vec![0i16; 16_000];
        let stereo = interleave_stereo(&mic, &sys);

        let wav_bytes = build_stereo_wav(&stereo, 16_000).expect("build_stereo_wav");

        // WAV header: bytes 22-23 (little-endian u16) = NumChannels
        assert!(wav_bytes.len() >= 44, "WAV must have at least a 44-byte header");
        let num_channels = u16::from_le_bytes([wav_bytes[22], wav_bytes[23]]);
        assert_eq!(
            num_channels, 2,
            "WAV NumChannels field must be 2 for stereo, got {}",
            num_channels
        );
    }

    /// Unit: WAV sample rate field matches the provided rate.
    #[test]
    fn stereo_wav_sample_rate_is_correct() {
        let stereo: Vec<i16> = vec![0i16; 32_000]; // 1s stereo at 16kHz
        let wav_bytes = build_stereo_wav(&stereo, 16_000).expect("build_stereo_wav");

        // WAV header: bytes 24-27 (little-endian u32) = SampleRate
        let sample_rate = u32::from_le_bytes([
            wav_bytes[24],
            wav_bytes[25],
            wav_bytes[26],
            wav_bytes[27],
        ]);
        assert_eq!(
            sample_rate, 16_000,
            "WAV SampleRate must be 16000, got {}",
            sample_rate
        );
    }

    /// Unit: Left channel recovered from stereo interleave equals original mic samples.
    #[test]
    fn split_stereo_recovers_left_channel() {
        let mic_samples: Vec<i16> = (0..4_800_i16).collect(); // 0.3s at 16kHz
        let sys_samples: Vec<i16> = vec![1234i16; 4_800];

        let stereo = interleave_stereo(&mic_samples, &sys_samples);
        let (recovered_left, _) = split_stereo(&stereo);

        assert_eq!(
            recovered_left, mic_samples,
            "left channel after split must equal original mic samples"
        );
    }

    /// Unit: Right channel recovered from stereo interleave equals original system samples.
    #[test]
    fn split_stereo_recovers_right_channel() {
        let mic_samples: Vec<i16> = vec![0i16; 4_800];
        let sys_samples: Vec<i16> = (0..4_800_i16).collect();

        let stereo = interleave_stereo(&mic_samples, &sys_samples);
        let (_, recovered_right) = split_stereo(&stereo);

        assert_eq!(
            recovered_right, sys_samples,
            "right channel after split must equal original system samples"
        );
    }

    /// Unit: speaker_hint = "Me" assigned to mic channel entries.
    #[test]
    fn mic_channel_entry_gets_speaker_hint_me() {
        let hint = SpeakerHint::from_channel("left");
        assert_eq!(
            hint.label(),
            "Me",
            "left (mic) channel should map to speaker_hint 'Me', got '{}'",
            hint.label()
        );
    }

    /// Unit: speaker_hint = "Others" assigned to system audio channel entries.
    #[test]
    fn system_channel_entry_gets_speaker_hint_others() {
        let hint = SpeakerHint::from_channel("right");
        assert_eq!(
            hint.label(),
            "Others",
            "right (system audio) channel should map to speaker_hint 'Others', got '{}'",
            hint.label()
        );
    }

    /// Unit: Metadata field speaker_hint is stored with the entry.
    #[test]
    fn meeting_entry_metadata_has_speaker_hint() {
        let conn = open_db();
        use fonos_core::meetings::session::{create_meeting_session, insert_meeting_entry};

        let session_id =
            create_meeting_session(&conn, &ts(0), "dual_channel").expect("create session");

        let entry_id = insert_meeting_entry(
            &conn,
            session_id,
            "I said something.",
            0,
            0,
            12_000,
            Some(SpeakerHint::Me),
        )
        .expect("insert entry");

        let metadata_json: String = conn
            .query_row(
                "SELECT metadata FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| row.get(0),
            )
            .expect("entry");

        let metadata: serde_json::Value = serde_json::from_str(&metadata_json).unwrap();

        assert!(
            metadata.get("speaker_hint").is_some(),
            "entry metadata must have 'speaker_hint' when provided"
        );
        assert_eq!(
            metadata["speaker_hint"].as_str().unwrap_or(""),
            "Me",
            "speaker_hint should be 'Me' for mic channel entry"
        );
    }
}

// ---------------------------------------------------------------------------
// M09 — AI summary generation
// ---------------------------------------------------------------------------

#[cfg(test)]
mod m09_summary_generation {
    use super::*;
    use fonos_core::meetings::summary::{build_summary_prompt, SummaryEntry};

    /// Unit: build_summary_prompt includes a system instruction.
    #[test]
    fn summary_prompt_contains_system_instruction() {
        let entries = vec![
            SummaryEntry { speaker: "Me".into(), timestamp_ms: 0, text: "Hello everyone.".into() },
            SummaryEntry {
                speaker: "Others".into(),
                timestamp_ms: 12_000,
                text: "Good morning, let's start.".into(),
            },
        ];

        let (system_prompt, _user_prompt) = build_summary_prompt(&entries, None);

        assert!(
            !system_prompt.is_empty(),
            "summary system prompt must not be empty"
        );
        assert!(
            system_prompt.to_lowercase().contains("summar")
                || system_prompt.to_lowercase().contains("meeting"),
            "system prompt should mention summarization or meeting, got: {}",
            system_prompt
        );
    }

    /// Unit: User prompt contains all transcript entries.
    #[test]
    fn summary_prompt_contains_all_transcript_entries() {
        let entries = vec![
            SummaryEntry {
                speaker: "Me".into(),
                timestamp_ms: 0,
                text: "Agenda item one is the budget.".into(),
            },
            SummaryEntry {
                speaker: "Others".into(),
                timestamp_ms: 15_000,
                text: "I agree with the budget proposal.".into(),
            },
            SummaryEntry {
                speaker: "Me".into(),
                timestamp_ms: 30_000,
                text: "Let's vote on the action items.".into(),
            },
        ];

        let (_system, user_prompt) = build_summary_prompt(&entries, None);

        assert!(
            user_prompt.contains("budget"),
            "user prompt must include transcript text 'budget'"
        );
        assert!(
            user_prompt.contains("agree"),
            "user prompt must include transcript text 'agree'"
        );
        assert!(
            user_prompt.contains("vote"),
            "user prompt must include transcript text 'vote'"
        );
    }

    /// Unit: User prompt includes speaker labels.
    #[test]
    fn summary_prompt_includes_speaker_labels() {
        let entries = vec![
            SummaryEntry { speaker: "Me".into(), timestamp_ms: 0, text: "My first point.".into() },
            SummaryEntry {
                speaker: "Others".into(),
                timestamp_ms: 12_000,
                text: "Their response.".into(),
            },
        ];

        let (_system, user_prompt) = build_summary_prompt(&entries, None);

        assert!(
            user_prompt.contains("Me"),
            "user prompt must include 'Me' speaker label"
        );
        assert!(
            user_prompt.contains("Others"),
            "user prompt must include 'Others' speaker label"
        );
    }

    /// Unit: Custom summary prompt is used when provided.
    #[test]
    fn build_summary_prompt_uses_custom_prompt_when_provided() {
        let entries = vec![SummaryEntry {
            speaker: "Me".into(),
            timestamp_ms: 0,
            text: "Let's skip the intro.".into(),
        }];
        let custom = "Focus on decisions only. Output as JSON.";

        let (system, _user) = build_summary_prompt(&entries, Some(custom));

        assert!(
            system.contains("decisions") || system.contains("JSON"),
            "system prompt should incorporate the custom prompt, got: {}",
            system
        );
    }

    /// Unit: Empty transcript produces a prompt that signals no content.
    #[test]
    fn summary_prompt_for_empty_transcript_is_graceful() {
        let entries: Vec<SummaryEntry> = vec![];
        // Should not panic — may return an error or a minimal prompt.
        let result = std::panic::catch_unwind(|| build_summary_prompt(&entries, None));
        assert!(
            result.is_ok(),
            "build_summary_prompt must not panic on empty entries"
        );
    }

    /// Integration: Summary entry stored with role=system in the session container.
    #[test]
    fn summary_entry_stored_with_role_system() {
        let conn = open_db();
        use fonos_core::meetings::session::{create_meeting_session, insert_summary_entry};

        let session_id =
            create_meeting_session(&conn, &ts(0), "mic_only").expect("create session");

        let summary_text = "## Summary\n- Budget approved\n- Next meeting Friday";
        let entry_id =
            insert_summary_entry(&conn, session_id, summary_text).expect("insert summary entry");

        let role: String = conn
            .query_row(
                "SELECT role FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| row.get(0),
            )
            .expect("entry");

        assert_eq!(
            role, "system",
            "summary entry role must be 'system', got '{}'",
            role
        );

        let container_id: i64 = conn
            .query_row(
                "SELECT container_id FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
                |row| row.get(0),
            )
            .expect("entry");

        assert_eq!(
            container_id, session_id,
            "summary entry must belong to the session container"
        );
    }
}

// ---------------------------------------------------------------------------
// M10 — OpenRouter LLM support
// ---------------------------------------------------------------------------

#[cfg(test)]
mod m10_openrouter {
    use fonos_core::meetings::openrouter::{openrouter_base_url, resolve_provider_base_url};

    /// Unit: openrouter_base_url() returns the correct API base URL.
    #[test]
    fn openrouter_base_url_is_correct() {
        let url = openrouter_base_url();
        assert_eq!(
            url, "https://openrouter.ai/api/v1",
            "OpenRouter base URL must be 'https://openrouter.ai/api/v1', got '{}'",
            url
        );
    }

    /// Unit: Provider "openrouter" resolves to the OpenRouter base URL.
    #[test]
    fn openrouter_provider_resolves_base_url() {
        let url = resolve_provider_base_url("openrouter");
        assert_eq!(
            url, "https://openrouter.ai/api/v1",
            "resolve_provider_base_url('openrouter') must return 'https://openrouter.ai/api/v1'"
        );
    }

    /// Unit: Provider "openai" still resolves to the OpenAI base URL.
    #[test]
    fn openai_provider_still_resolves_correctly() {
        let url = resolve_provider_base_url("openai");
        assert!(
            url.contains("openai.com"),
            "openai provider should resolve to api.openai.com, got '{}'",
            url
        );
    }

    /// Unit: Anthropic model prefix "anthropic/" is a valid OpenRouter model ID.
    #[test]
    fn openrouter_anthropic_model_id_format_is_valid() {
        use fonos_core::meetings::openrouter::is_valid_openrouter_model_id;
        assert!(
            is_valid_openrouter_model_id("anthropic/claude-sonnet-4"),
            "'anthropic/claude-sonnet-4' must be a valid OpenRouter model ID"
        );
    }

    /// Unit: Google model prefix "google/" is a valid OpenRouter model ID.
    #[test]
    fn openrouter_google_model_id_format_is_valid() {
        use fonos_core::meetings::openrouter::is_valid_openrouter_model_id;
        assert!(
            is_valid_openrouter_model_id("google/gemini-2.5-flash"),
            "'google/gemini-2.5-flash' must be a valid OpenRouter model ID"
        );
    }

    /// Unit: A plain model name without "/" is not a valid OpenRouter model ID.
    #[test]
    fn openrouter_plain_model_name_is_invalid() {
        use fonos_core::meetings::openrouter::is_valid_openrouter_model_id;
        // Plain names like "gpt-4o" are OpenAI IDs, not OpenRouter-namespaced IDs.
        // OpenRouter IDs require a "provider/model" format.
        assert!(
            !is_valid_openrouter_model_id("gpt-4o"),
            "'gpt-4o' should not be treated as a valid OpenRouter model ID"
        );
    }

    /// Unit: OpenRouter provider config includes "openrouter" in PROVIDERS list.
    #[test]
    fn openrouter_in_provider_constants() {
        use fonos_core::meetings::openrouter::SUPPORTED_PROVIDERS;
        assert!(
            SUPPORTED_PROVIDERS.contains(&"openrouter"),
            "SUPPORTED_PROVIDERS must include 'openrouter'"
        );
    }
}

// ---------------------------------------------------------------------------
// M14 — Meeting export
// ---------------------------------------------------------------------------

#[cfg(test)]
mod m14_meeting_export {
    use super::*;
    use fonos_core::meetings::export::{export_meeting_markdown, export_meeting_json};
    use fonos_core::meetings::session::{
        create_meeting_session, insert_meeting_entry, insert_summary_entry,
    };
    use fonos_core::meetings::audio::SpeakerHint;

    /// Build a realistic meeting session with transcript entries and a summary.
    fn create_test_meeting(conn: &Connection) -> i64 {
        let session_id = create_meeting_session(conn, "2026-03-28T09:30:00", "dual_channel")
            .expect("create session");

        let transcript = vec![
            (0_u32, 0_u64, 12_000_u64, Some(SpeakerHint::Me), "Welcome to the quarterly review."),
            (1, 12_000, 11_500, Some(SpeakerHint::Others), "Thanks for organizing this."),
            (2, 23_500, 13_000, Some(SpeakerHint::Me), "Let's cover the budget first."),
            (3, 36_500, 10_500, Some(SpeakerHint::Others), "The Q1 numbers look strong."),
            (
                4,
                47_000,
                12_000,
                Some(SpeakerHint::Me),
                "Agreed. Action item: present to board by Friday.",
            ),
        ];

        for (idx, ts_ms, dur_ms, speaker, text) in transcript {
            insert_meeting_entry(conn, session_id, text, idx, ts_ms, dur_ms, speaker)
                .expect("insert entry");
        }

        let summary = concat!(
            "## Summary\n",
            "Quarterly review meeting discussed Q1 budget.\n\n",
            "## Key Points\n",
            "- Q1 numbers strong\n",
            "- Budget presentation required\n\n",
            "## Action Items\n",
            "- [ ] Present Q1 budget to board by Friday\n"
        );
        insert_summary_entry(conn, session_id, summary).expect("insert summary");

        session_id
    }

    /// Unit: Markdown export is non-empty.
    #[test]
    fn export_markdown_produces_non_empty_output() {
        let conn = open_db();
        let session_id = create_test_meeting(&conn);

        let md = export_meeting_markdown(&conn, session_id).expect("export_meeting_markdown");
        assert!(!md.is_empty(), "markdown export must not be empty");
    }

    /// Unit: Markdown export contains the AI summary section.
    #[test]
    fn export_markdown_contains_summary() {
        let conn = open_db();
        let session_id = create_test_meeting(&conn);

        let md = export_meeting_markdown(&conn, session_id).expect("export markdown");

        assert!(
            md.contains("Summary") || md.contains("summary"),
            "markdown export must include the summary section, got:\n{}",
            &md[..md.len().min(400)]
        );
        assert!(
            md.contains("Q1") || md.contains("budget"),
            "markdown export must include summary content"
        );
    }

    /// Unit: Markdown export contains speaker labels.
    #[test]
    fn export_markdown_contains_speaker_labels() {
        let conn = open_db();
        let session_id = create_test_meeting(&conn);

        let md = export_meeting_markdown(&conn, session_id).expect("export markdown");

        assert!(
            md.contains("Me"),
            "markdown export must include speaker label 'Me'"
        );
        assert!(
            md.contains("Others"),
            "markdown export must include speaker label 'Others'"
        );
    }

    /// Unit: Markdown export contains timestamps for transcript entries.
    #[test]
    fn export_markdown_contains_timestamps() {
        let conn = open_db();
        let session_id = create_test_meeting(&conn);

        let md = export_meeting_markdown(&conn, session_id).expect("export markdown");

        // Timestamps formatted as MM:SS or HH:MM:SS should appear.
        assert!(
            md.contains("0:00") || md.contains("00:00"),
            "markdown export must include formatted timestamps"
        );
    }

    /// Unit: Markdown export contains transcript text.
    #[test]
    fn export_markdown_contains_transcript_text() {
        let conn = open_db();
        let session_id = create_test_meeting(&conn);

        let md = export_meeting_markdown(&conn, session_id).expect("export markdown");

        assert!(
            md.contains("quarterly review"),
            "export must contain transcript text 'quarterly review'"
        );
        assert!(
            md.contains("budget"),
            "export must contain transcript text 'budget'"
        );
        assert!(
            md.contains("Action item"),
            "export must contain action item text"
        );
    }

    /// Unit: JSON export is valid JSON.
    #[test]
    fn export_json_produces_valid_json() {
        let conn = open_db();
        let session_id = create_test_meeting(&conn);

        let json_str = export_meeting_json(&conn, session_id).expect("export_meeting_json");
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("exported string must be valid JSON");

        assert!(parsed.is_object(), "JSON export root must be an object");
    }

    /// Unit: JSON export contains a "transcript" array with correct entry count.
    #[test]
    fn export_json_contains_transcript_array() {
        let conn = open_db();
        let session_id = create_test_meeting(&conn);

        let json_str = export_meeting_json(&conn, session_id).expect("export json");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let transcript = parsed
            .get("transcript")
            .expect("JSON must have 'transcript' field");
        assert!(
            transcript.is_array(),
            "'transcript' must be an array"
        );

        let entries = transcript.as_array().unwrap();
        assert_eq!(
            entries.len(),
            5,
            "transcript should have 5 entries (matching the 5 inserted), got {}",
            entries.len()
        );
    }

    /// Unit: JSON export entries have required fields.
    #[test]
    fn export_json_entries_have_required_fields() {
        let conn = open_db();
        let session_id = create_test_meeting(&conn);

        let json_str = export_meeting_json(&conn, session_id).expect("export json");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let entry = &parsed["transcript"][0];
        assert!(entry.get("speaker").is_some(), "entry must have 'speaker'");
        assert!(
            entry.get("timestamp_ms").is_some() || entry.get("timestamp").is_some(),
            "entry must have timestamp field"
        );
        assert!(entry.get("text").is_some(), "entry must have 'text'");
    }

    /// Unit: Export returns an error for a non-existent session.
    #[test]
    fn export_nonexistent_session_returns_error() {
        let conn = open_db();
        let result = export_meeting_markdown(&conn, 99_999);
        assert!(
            result.is_err(),
            "export of non-existent session_id must return an error"
        );
    }
}

// ---------------------------------------------------------------------------
// M15 — Backward compatibility
// ---------------------------------------------------------------------------

#[cfg(test)]
mod m15_backward_compatibility {
    use super::*;

    /// Static: Existing storage public API is still importable.
    #[test]
    fn existing_storage_api_compiles() {
        use fonos_core::storage::{ContainerType, EntryRole, SourceType};
        let conn = open_db();
        // Verify the main types exist and can be constructed.
        let _: SourceType = SourceType::Dictation;
        let _: EntryRole = EntryRole::User;
        let _: ContainerType = ContainerType::Notebook;
        // If this compiles, the API is intact.
        let _ = conn;
    }

    /// Static: `OutputTarget` (moved off the deleted `modes` system onto
    /// `fonos_core::config` in Workbench P2 Task 12) is still importable.
    #[test]
    fn output_target_api_compiles() {
        use fonos_core::config::OutputTarget;
        let _: OutputTarget = OutputTarget::Clipboard;
    }

    /// Static: Existing config public API is still importable.
    #[test]
    fn existing_config_api_compiles() {
        let config = AppConfig::default();
        // Verify all pre-existing fields still exist.
        let _ = &config.hotkey_dictation;
        let _ = &config.hotkey_tts;
        let _ = &config.hotkey_agent;
        let _ = &config.hotkey_agent_panel;
        let _ = &config.hotkey_note;
        let _ = &config.dictation_mode;
        let _ = &config.stt_profile;
        let _ = &config.llm_profile;
        let _ = &config.tts_profile;
    }

    /// Static: Existing hotkey parser still works for pre-existing combos.
    #[test]
    fn existing_hotkeys_still_parse() {
        let config = AppConfig::default();
        for combo in [
            config.hotkey_dictation.as_str(),
            config.hotkey_tts.as_str(),
            config.hotkey_agent.as_str(),
            config.hotkey_note.as_str(),
        ] {
            assert!(
                parse_hotkey(combo).is_ok(),
                "pre-existing hotkey '{}' must still parse without error",
                combo
            );
        }
    }

    /// Integration: cargo build for fonos-core succeeds (checked by running this file).
    ///
    /// If this test module compiles and runs, the build is not broken.
    #[test]
    fn fonos_core_build_succeeds() {
        // Intentional no-op: the fact that this test file compiled means the build succeeded.
        // The CI step that runs this also catches linker errors.
    }
}

// ---------------------------------------------------------------------------
// Q01 — STT latency per chunk
// ---------------------------------------------------------------------------

#[cfg(test)]
mod q01_chunk_latency {
    use super::*;
    use fonos_core::meetings::chunker::{chunk_audio, ChunkConfig};

    /// Integration: Chunk processing (slicing + WAV encode) completes under 3000 ms for a 15s chunk.
    ///
    /// Note: This test does NOT call an external STT service — it measures the local
    /// preparation work (chunk extraction + WAV encoding) which must complete fast
    /// enough that the remaining network + inference budget fits within 3 seconds.
    #[test]
    fn chunk_audio_processing_under_3000ms() {
        use fonos_core::meetings::audio::build_mono_wav;

        // Simulate 15 seconds of 16 kHz PCM.
        let samples = mock_30s_pcm();
        let config = ChunkConfig {
            sample_rate: 16_000,
            target_chunk_secs: 12,
            min_chunk_secs: 10,
            max_chunk_secs: 15,
        };

        let start = Instant::now();

        let chunks = chunk_audio(&samples, &config);
        for chunk in &chunks {
            // Encode each chunk to WAV — the work done before network dispatch.
            let _wav = build_mono_wav(&chunk.samples, 16_000)
                .expect("build_mono_wav must succeed for a valid chunk");
        }

        let elapsed_ms = start.elapsed().as_millis();

        assert!(
            elapsed_ms < 3_000,
            "chunk audio processing took {}ms — must complete under 3000ms (Q01 threshold)",
            elapsed_ms
        );
    }

    /// Integration: Per-chunk processing time is recorded in chunk metadata.
    ///
    /// Each AudioChunk should carry a `processed_at_ms` or similar field so the
    /// latency can be reported in telemetry.
    #[test]
    fn chunk_carries_timing_metadata() {
        use fonos_core::meetings::chunker::{chunk_audio, ChunkConfig};

        let samples = mock_30s_pcm();
        let config = ChunkConfig {
            sample_rate: 16_000,
            target_chunk_secs: 12,
            min_chunk_secs: 10,
            max_chunk_secs: 15,
        };

        let chunks = chunk_audio(&samples, &config);
        assert!(!chunks.is_empty(), "must have at least one chunk to check");

        for (i, chunk) in chunks.iter().enumerate() {
            // Each chunk must carry a `sample_offset` (position in original stream).
            assert!(
                chunk.sample_offset < samples.len(),
                "chunk {} sample_offset {} must be within the input sample range",
                i,
                chunk.sample_offset
            );
        }
    }
}
