//! `memory` collector (spec §7).
//!
//! Reads `/proc/meminfo`. `used = total - available` follows the standard
//! Linux convention (matches `free -m` and `htop`).

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct Memory;

#[derive(Debug, Serialize)]
struct MemoryData {
    total_bytes: u64,
    used_bytes: u64,
    free_bytes: u64,
    available_bytes: u64,
    percent: f32,
    swap_total: u64,
    swap_used: u64,
}

impl Collector for Memory {
    fn name(&self) -> &'static str {
        "mem"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let text = std::fs::read_to_string(Path::new("/proc/meminfo")).map_err(|err| {
            CollectorError::Parse {
                origin: "/proc/meminfo".into(),
                message: err.to_string(),
            }
        })?;

        let info = parse_meminfo(&text);
        let total = info.get("MemTotal").copied().unwrap_or(0);
        let available = info.get("MemAvailable").copied().unwrap_or(0);
        let free = info.get("MemFree").copied().unwrap_or(0);
        let swap_total = info.get("SwapTotal").copied().unwrap_or(0);
        let swap_free = info.get("SwapFree").copied().unwrap_or(0);

        let used = total.saturating_sub(available);
        let percent = if total > 0 {
            (used as f32 / total as f32) * 100.0
        } else {
            0.0
        };

        let data = MemoryData {
            total_bytes: total,
            used_bytes: used,
            free_bytes: free,
            available_bytes: available,
            percent: round2(percent),
            swap_total,
            swap_used: swap_total.saturating_sub(swap_free),
        };

        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "mem".into(),
            message: err.to_string(),
        })
    }
}

fn parse_meminfo(text: &str) -> std::collections::HashMap<String, u64> {
    let mut out = std::collections::HashMap::new();
    for line in text.lines() {
        // Format: "MemTotal:       16332596 kB"
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(value) = parts.next().and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        let bytes = match parts.next() {
            Some("kB") | Some("KB") => value * 1024,
            _ => value,
        };
        out.insert(key.to_string(), bytes);
    }
    out
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kb_lines() {
        let text = "MemTotal:       16332596 kB\nMemAvailable:    8000000 kB\n";
        let info = parse_meminfo(text);
        assert_eq!(info.get("MemTotal").copied(), Some(16_332_596 * 1024));
        assert_eq!(info.get("MemAvailable").copied(), Some(8_000_000 * 1024));
    }

    #[test]
    fn ignores_malformed() {
        let info = parse_meminfo("nope\nMemTotal: 100 kB\nempty:");
        assert_eq!(info.len(), 1);
    }
}
