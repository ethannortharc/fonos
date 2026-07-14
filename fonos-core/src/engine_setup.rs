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

/// Parse `df -k <path>` output: available KB is column 4 of the data line.
/// Handles both GNU and BSD df (header line + one data line; a data line may
/// wrap only when the device name is very long, in which case the numbers
/// land on the following line — handled by scanning for the first line whose
/// 4th column parses).
pub fn parse_df_available_kb(output: &str) -> Option<u64> {
    for line in output.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 4 {
            if let Ok(kb) = cols[3].parse::<u64>() {
                return Some(kb);
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
}
