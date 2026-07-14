//! Engine-setup pure logic — hardware tiering, disk/meminfo parsing, and
//! Ollama `/api/pull` progress-line parsing. Platform reads (sysctl, df,
//! which) live in the desktop command layer; everything here is unit-testable
//! without a shell.

use serde::{Deserialize, Serialize};

/// Hardware capability tier. Never shown to users as a concept — it only
/// drives model recommendations and downgrade suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HardwareTier {
    /// 16 GB-class or CPU-only machines.
    Light,
    /// 32 GB-class unified memory or an NVIDIA GPU present.
    Balanced,
    /// 64 GB+ unified memory (Apple Silicon) or 64 GB+ with an NVIDIA GPU.
    Max,
}

/// Classify hardware into a tier.
///
/// Rules (mockup v3): Apple Silicon goes by unified memory (>=64 GB → Max,
/// >=32 GB → Balanced, else Light); a machine with an NVIDIA GPU is Balanced
/// (Max with >=64 GB RAM); a pure-CPU machine is always Light regardless of
/// RAM — CPU-only LLM inference can't carry the bigger models.
pub fn classify_tier(mem_bytes: u64, chip_brand: &str, has_nvidia_gpu: bool) -> HardwareTier {
    const GB: u64 = 1024 * 1024 * 1024;
    let apple = chip_brand.contains("Apple");
    if apple {
        if mem_bytes >= 64 * GB {
            return HardwareTier::Max;
        }
        if mem_bytes >= 32 * GB {
            return HardwareTier::Balanced;
        }
        return HardwareTier::Light;
    }
    if has_nvidia_gpu {
        if mem_bytes >= 64 * GB {
            return HardwareTier::Max;
        }
        return HardwareTier::Balanced;
    }
    HardwareTier::Light
}

/// One tier's recommended Ollama pull target.
#[derive(Debug, Clone, Serialize)]
pub struct TierPull {
    /// Ollama model tag to pull.
    pub model: &'static str,
    /// Approximate download size in GB — display + disk-precheck estimate
    /// only; the pull itself reports authoritative sizes.
    pub size_gb: f64,
}

/// Recommended Ollama LLM per tier (Qwen3 family, Q4-class quants).
pub fn tier_pull(tier: HardwareTier) -> TierPull {
    match tier {
        HardwareTier::Max => TierPull { model: "qwen3:30b-a3b", size_gb: 18.6 },
        HardwareTier::Balanced => TierPull { model: "qwen3:14b", size_gb: 9.3 },
        HardwareTier::Light => TierPull { model: "qwen3:4b", size_gb: 2.6 },
    }
}

/// The next tier down, for failure/disk downgrade suggestions.
pub fn downgrade(tier: HardwareTier) -> Option<HardwareTier> {
    match tier {
        HardwareTier::Max => Some(HardwareTier::Balanced),
        HardwareTier::Balanced => Some(HardwareTier::Light),
        HardwareTier::Light => None,
    }
}

/// Parse `df -k <path>` output: locate the Use%/Capacity column — the first
/// whitespace-separated field on the data line that ends with `%` — and take
/// the field immediately before it as the Available KB count.
///
/// A fixed column index is not safe here. GNU `df` wraps the data line onto
/// a continuation line when the device name is long (common on Linux LVM,
/// e.g. `/dev/mapper/ubuntu--vg-ubuntu--lv`); the continuation line drops
/// the device-name column, shifting every following column one to the left.
/// Scanning for the `%` column instead of assuming a fixed index handles
/// both the normal (device-present) line and the wrapped continuation line.
/// It also handles macOS/BSD `df -k`, which can append extra `iused`/
/// `ifree`/`%iused` columns after `Available`/`Capacity` — the *first* `%`
/// column on the line is always `Capacity`/`Use%`, never the trailing
/// `%iused`. Returns the value parsed from the first line that yields one.
pub fn parse_df_available_kb(output: &str) -> Option<u64> {
    for line in output.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if let Some(pct_idx) = cols.iter().position(|c| c.ends_with('%')) {
            if pct_idx > 0 {
                if let Ok(kb) = cols[pct_idx - 1].parse::<u64>() {
                    return Some(kb);
                }
            }
        }
    }
    None
}

/// Parse `/proc/meminfo` content: `MemTotal:  32768000 kB` → bytes.
pub fn parse_meminfo_total_bytes(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// One parsed progress line from Ollama's streaming `/api/pull`.
#[derive(Debug, Clone, Serialize)]
pub struct PullProgress {
    /// Raw status string ("pulling manifest", "downloading …", "success").
    pub status: String,
    /// Percent complete when the line carries total+completed byte counts.
    pub pct: Option<u8>,
    /// Error message when the line is an error line.
    pub error: Option<String>,
}

/// Parse one NDJSON line from `/api/pull`. Non-JSON lines yield `None`.
pub fn parse_pull_line(line: &str) -> Option<PullProgress> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
        return Some(PullProgress { status: String::new(), pct: None, error: Some(err.to_string()) });
    }
    let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string();
    let pct = match (
        v.get("total").and_then(|t| t.as_u64()),
        v.get("completed").and_then(|c| c.as_u64()),
    ) {
        (Some(total), Some(done)) if total > 0 => Some(((done * 100) / total).min(100) as u8),
        _ => None,
    };
    Some(PullProgress { status, pct, error: None })
}

/// Incremental byte→line splitter for NDJSON streams (chunks may cut lines).
#[derive(Default)]
pub struct LineBuffer {
    buf: Vec<u8>,
}

impl LineBuffer {
    /// Feed a chunk; returns the complete lines it closed (without `\n`).
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            let line = &line[..line.len() - 1];
            out.push(String::from_utf8_lossy(line).into_owned());
        }
        out
    }

    /// Drain any trailing bytes as a final line (streams may omit the last `\n`).
    pub fn finish(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            return None;
        }
        let rest = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        Some(rest)
    }
}

/// Pull a model via Ollama's streaming `POST /api/pull`, invoking
/// `on_progress` per parsed NDJSON line. Errors on HTTP failure, an error
/// line, or a stream that ends without a `success` status.
pub async fn ollama_pull(
    base_url: &str,
    model: &str,
    mut on_progress: impl FnMut(PullProgress),
) -> Result<(), String> {
    use futures_util::StreamExt;
    let url = format!("{}/api/pull", base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        // Both spellings for old/new Ollama versions.
        .json(&serde_json::json!({ "name": model, "model": model, "stream": true }))
        .send()
        .await
        .map_err(|e| format!("ollama pull request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("ollama pull error {status}: {body}"));
    }
    let mut stream = resp.bytes_stream();
    let mut lines = LineBuffer::default();
    let mut succeeded = false;
    let mut handle = |line: String| -> Result<(), String> {
        if let Some(p) = parse_pull_line(&line) {
            if let Some(err) = &p.error {
                return Err(err.clone());
            }
            if p.status == "success" {
                succeeded = true;
            }
            on_progress(p);
        }
        Ok(())
    };
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("ollama pull stream failed: {e}"))?;
        for line in lines.push(&chunk) {
            handle(line)?;
        }
    }
    if let Some(rest) = lines.finish() {
        handle(rest)?;
    }
    if succeeded {
        Ok(())
    } else {
        Err("ollama pull ended without success".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn tiers_follow_mockup_rules() {
        assert_eq!(classify_tier(64 * GB, "Apple M4 Pro", false), HardwareTier::Max);
        assert_eq!(classify_tier(36 * GB, "Apple M3", false), HardwareTier::Balanced);
        assert_eq!(classify_tier(16 * GB, "Apple M2", false), HardwareTier::Light);
        // NVIDIA box: Balanced, Max only with 64 GB+.
        assert_eq!(classify_tier(32 * GB, "AMD Ryzen 9", true), HardwareTier::Balanced);
        assert_eq!(classify_tier(128 * GB, "Intel Xeon", true), HardwareTier::Max);
        // Pure CPU is always Light, even with lots of RAM.
        assert_eq!(classify_tier(128 * GB, "Intel Core i9", false), HardwareTier::Light);
        // Exact boundaries: >= semantics, not >.
        assert_eq!(classify_tier(32 * GB, "Apple M3", false), HardwareTier::Balanced);
        assert_eq!(classify_tier(64 * GB, "NVIDIA Jetson AGX", true), HardwareTier::Max);
    }

    #[test]
    fn downgrade_chain_terminates() {
        assert_eq!(downgrade(HardwareTier::Max), Some(HardwareTier::Balanced));
        assert_eq!(downgrade(HardwareTier::Balanced), Some(HardwareTier::Light));
        assert_eq!(downgrade(HardwareTier::Light), None);
    }

    #[test]
    fn df_output_parses_available_kb() {
        let bsd = "Filesystem   1024-blocks      Used Available Capacity  Mounted on\n\
                   /dev/disk3s5  971350180 250000000 536870912    32%    /System/Volumes/Data\n";
        assert_eq!(parse_df_available_kb(bsd), Some(536870912));
        assert_eq!(parse_df_available_kb("garbage"), None);
    }

    #[test]
    fn df_output_survives_wrapped_gnu_device_line() {
        // GNU `df -k` wraps onto a continuation line when the device name is
        // long (common on Linux LVM). The continuation line has no device
        // column, so a fixed column index would misread "21%" as Available.
        let gnu_wrapped = "Filesystem                     1K-blocks   Used Available Use% Mounted on\n\
                            /dev/mapper/ubuntu--vg-ubuntu--lv\n\
                             52428800 10485760 39942840  21% /\n";
        assert_eq!(parse_df_available_kb(gnu_wrapped), Some(39942840));
    }

    #[test]
    fn df_output_parses_real_macos_df_k_shape() {
        // Captured via `df -k /` on macOS (Darwin 25.2.0, arm64):
        //   Filesystem     1024-blocks      Used Available Capacity iused     ifree %iused  Mounted on
        //   /dev/disk3s1s1   971350180  17381012  15737864    53%  453019 157378640    0%   /
        // Note the trailing `%iused` column: this is a second field ending
        // in `%`, so the parser must pick the FIRST such column (Capacity),
        // not the last, to land on the correct Available value.
        let macos = "Filesystem     1024-blocks      Used Available Capacity iused     ifree %iused  Mounted on\n\
                     /dev/disk3s1s1   971350180  17381012  15737864    53%  453019 157378640    0%   /\n";
        assert_eq!(parse_df_available_kb(macos), Some(15737864));
    }

    #[test]
    fn meminfo_parses_total() {
        let text = "MemTotal:       32768000 kB\nMemFree:         1000000 kB\n";
        assert_eq!(parse_meminfo_total_bytes(text), Some(32768000 * 1024));
        assert_eq!(parse_meminfo_total_bytes("nope"), None);
    }

    #[test]
    fn pull_lines_parse_progress_and_errors() {
        let p = parse_pull_line(r#"{"status":"downloading sha","total":100,"completed":37}"#).unwrap();
        assert_eq!(p.pct, Some(37));
        assert!(p.error.is_none());
        let done = parse_pull_line(r#"{"status":"success"}"#).unwrap();
        assert_eq!(done.status, "success");
        assert_eq!(done.pct, None);
        let err = parse_pull_line(r#"{"error":"pull model manifest: file does not exist"}"#).unwrap();
        assert!(err.error.as_deref().unwrap().contains("manifest"));
        assert!(parse_pull_line("not json").is_none());
    }

    #[test]
    fn line_buffer_splits_ndjson_across_chunks() {
        let mut buf = LineBuffer::default();
        let mut lines: Vec<String> = Vec::new();
        lines.extend(buf.push(b"{\"status\":\"a\"}\n{\"sta"));
        lines.extend(buf.push(b"tus\":\"b\"}\n"));
        assert_eq!(lines, vec![r#"{"status":"a"}"#.to_string(), r#"{"status":"b"}"#.to_string()]);
        assert_eq!(buf.push(b"tail-no-newline"), Vec::<String>::new());
        assert_eq!(buf.finish().as_deref(), Some("tail-no-newline"));
    }
}
