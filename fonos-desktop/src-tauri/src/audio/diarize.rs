//! Speaker-diarization sidecar: spawns the `fonos-diarize` Swift helper and
//! folds its NDJSON segment stream into a queryable [`SpeakerTimeline`].
//! Compiled on all platforms; on Linux (or when the helper/models are
//! missing) callers simply never get a session — same runtime-degradation
//! philosophy as `system_capture`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn models_dir() -> PathBuf {
    fonos_core::config::AppConfig::config_dir().join("models").join("diarization")
}

#[derive(Debug, PartialEq)]
pub enum DiarizeEvent {
    Ready,
    Segment { speaker: String, start_ms: u64, end_ms: u64 },
    Error { message: String },
}

pub fn parse_event(line: &str) -> Option<DiarizeEvent> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    match v.get("type")?.as_str()? {
        "ready" => Some(DiarizeEvent::Ready),
        "segment" => Some(DiarizeEvent::Segment {
            speaker: v.get("speaker")?.as_str()?.to_string(),
            start_ms: v.get("start_ms")?.as_u64()?,
            end_ms: v.get("end_ms")?.as_u64()?,
        }),
        "error" => Some(DiarizeEvent::Error {
            message: v.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string(),
        }),
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct Seg { speaker: String, start_ms: u64, end_ms: u64 }

#[derive(Default)]
pub struct SpeakerTimeline { segs: Vec<Seg> }

impl SpeakerTimeline {
    /// 同 (speaker, start_ms) 视为延长（end_ms 取 max），否则追加。
    pub fn upsert(&mut self, speaker: &str, start_ms: u64, end_ms: u64) {
        if let Some(s) = self.segs.iter_mut()
            .find(|s| s.speaker == speaker && s.start_ms == start_ms) {
            s.end_ms = s.end_ms.max(end_ms);
            return;
        }
        self.segs.push(Seg { speaker: speaker.to_string(), start_ms, end_ms });
    }

    pub fn len(&self) -> usize { self.segs.len() }

    /// [start_ms, end_ms) 内累计重叠时长最大的 speaker；平手取字典序小者；零重叠 None。
    pub fn dominant_speaker(&self, start_ms: u64, end_ms: u64) -> Option<String> {
        use std::collections::HashMap;
        let mut overlap: HashMap<&str, u64> = HashMap::new();
        for s in &self.segs {
            let lo = s.start_ms.max(start_ms);
            let hi = s.end_ms.min(end_ms);
            if hi > lo { *overlap.entry(s.speaker.as_str()).or_insert(0) += hi - lo; }
        }
        overlap.into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(sp, _)| sp.to_string())
    }
}

pub struct DiarizeStatus { pub available: bool, pub models_present: bool }

/// 跑 `fonos-diarize check`；helper 缺失 → Err（调用方降级）。
pub fn check(models_dir: &Path) -> Result<DiarizeStatus, String> {
    let bin = find_diarize_binary().ok_or("fonos-diarize binary not found")?;
    let out = Command::new(&bin)
        .args(["check", "--models-dir"]).arg(models_dir)
        .output().map_err(|e| format!("run check: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("check output not json: {e}: {stdout}"))?;
    Ok(DiarizeStatus {
        available: v.get("available").and_then(|b| b.as_bool()).unwrap_or(false),
        models_present: v.get("models_present").and_then(|b| b.as_bool()).unwrap_or(false),
    })
}

pub struct DiarizeSession {
    child: Child,
    stdin: Option<ChildStdin>,
    timeline: Arc<Mutex<SpeakerTimeline>>,
    dead: Arc<AtomicBool>,
}

impl DiarizeSession {
    pub fn spawn(models_dir: &Path) -> Result<Self, String> {
        let bin = find_diarize_binary().ok_or("fonos-diarize binary not found")?;
        let mut child = Command::new(&bin)
            .args(["stream", "--models-dir"]).arg(models_dir)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit())
            .spawn().map_err(|e| format!("spawn fonos-diarize: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let timeline = Arc::new(Mutex::new(SpeakerTimeline::default()));
        let dead = Arc::new(AtomicBool::new(false));
        let (tl, dd) = (Arc::clone(&timeline), Arc::clone(&dead));
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match parse_event(&line) {
                    Some(DiarizeEvent::Segment { speaker, start_ms, end_ms }) => {
                        if let Ok(mut t) = tl.lock() { t.upsert(&speaker, start_ms, end_ms); }
                    }
                    Some(DiarizeEvent::Error { message }) => {
                        eprintln!("fonos: fonos-diarize error: {message}");
                        dd.store(true, Ordering::SeqCst);
                    }
                    _ => {}
                }
            }
            dd.store(true, Ordering::SeqCst); // stdout 关闭 = 进程退出
        });
        Ok(Self { child, stdin: Some(stdin), timeline, dead })
    }

    pub fn feed(&mut self, samples: &[i16]) {
        if self.is_dead() { return; }
        let Some(stdin) = self.stdin.as_mut() else { return };
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for s in samples { bytes.extend_from_slice(&s.to_le_bytes()); }
        if stdin.write_all(&bytes).is_err() {
            eprintln!("fonos: fonos-diarize stdin write failed — degrading");
            self.dead.store(true, Ordering::SeqCst);
            self.stdin = None;
        }
    }

    pub fn is_dead(&self) -> bool { self.dead.load(Ordering::SeqCst) }

    pub fn dominant(&self, start_ms: u64, end_ms: u64) -> Option<String> {
        self.timeline.lock().ok()?.dominant_speaker(start_ms, end_ms)
    }

    pub fn shutdown(mut self) {
        drop(self.stdin.take()); // EOF → helper flush 后退出
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait(); // reap — kill() alone leaves a zombie
                    return;
                }
            }
        }
    }
}

/// Locate a bundled helper binary by name, trying the same six candidate
/// locations used across dev, `cargo test`, and the packaged .app bundle.
/// Shared by [`find_diarize_binary`] and
/// `system_capture::find_audio_capture_binary`.
pub(crate) fn find_helper_binary(name: &str) -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("Resources").join(name));
                candidates.push(parent.join("Resources").join("resources").join(name));
            }
        }
    }
    candidates.push(PathBuf::from(format!("src-tauri/resources/{name}")));
    candidates.push(PathBuf::from(format!("fonos-desktop/src-tauri/resources/{name}")));
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("resources").join(name));
    for c in &candidates {
        if c.exists() {
            eprintln!("fonos: found {} at {}", name, c.display());
            return Some(c.to_string_lossy().to_string());
        }
    }
    eprintln!("fonos: {} not found; searched: {:?}", name,
        candidates.iter().map(|c| c.display().to_string()).collect::<Vec<_>>());
    None
}

pub(crate) fn find_diarize_binary() -> Option<String> {
    find_helper_binary("fonos-diarize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_upsert_extends_same_start() {
        let mut t = SpeakerTimeline::default();
        t.upsert("s1", 0, 1000);
        t.upsert("s1", 0, 3000); // 同 speaker+start → 延长
        assert_eq!(t.dominant_speaker(0, 3000), Some("s1".to_string()));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn dominant_picks_max_overlap() {
        let mut t = SpeakerTimeline::default();
        t.upsert("s1", 0, 4000);      // 与查询 [3000,10000) 重叠 1000
        t.upsert("s2", 4000, 10000);  // 重叠 6000
        assert_eq!(t.dominant_speaker(3000, 10000), Some("s2".to_string()));
    }

    #[test]
    fn dominant_none_when_no_overlap() {
        let mut t = SpeakerTimeline::default();
        t.upsert("s1", 0, 1000);
        assert_eq!(t.dominant_speaker(5000, 6000), None); // 相切/无交 → None
        assert_eq!(t.dominant_speaker(1000, 2000), None); // 边界相切不算重叠
    }

    #[test]
    fn dominant_tie_breaks_deterministically() {
        let mut t = SpeakerTimeline::default();
        t.upsert("s2", 0, 1000);
        t.upsert("s1", 1000, 2000); // 各重叠 1000 → 取 speaker 字典序小者
        assert_eq!(t.dominant_speaker(0, 2000), Some("s1".to_string()));
    }

    #[test]
    fn parse_event_variants() {
        assert!(matches!(parse_event(r#"{"type":"ready"}"#), Some(DiarizeEvent::Ready)));
        match parse_event(r#"{"type":"segment","speaker":"s2","start_ms":100,"end_ms":900}"#) {
            Some(DiarizeEvent::Segment { speaker, start_ms, end_ms }) => {
                assert_eq!((speaker.as_str(), start_ms, end_ms), ("s2", 100, 900));
            }
            other => panic!("bad parse: {other:?}"),
        }
        assert!(matches!(parse_event(r#"{"type":"error","message":"x"}"#),
            Some(DiarizeEvent::Error { .. })));
        assert_eq!(parse_event("not json"), None);
        assert_eq!(parse_event(r#"{"type":"progress","pct":50}"#), None); // stream 不产生，忽略
    }

    #[test]
    fn models_dir_under_app_data() {
        let d = models_dir();
        assert!(d.ends_with("models/diarization"));
        assert!(d.to_string_lossy().contains("com.fonos.app"));
    }
}
