//! `cpu` collector (spec §7).
//!
//! Usage is the expensive field. We read `/proc/stat`, sleep 50ms, read again,
//! and compute the active/idle delta. Everything else (model, cores, freq,
//! temp) is cheap and runs in <1ms.

use std::collections::HashSet;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

const SAMPLE_WINDOW: Duration = Duration::from_millis(50);

pub struct Cpu;

#[derive(Debug, Serialize)]
struct CpuData {
    name: String,
    cores: u32,
    threads: u32,
    usage: f32,
    freq_mhz: f32,
    temp_c: Option<f32>,
}

impl Collector for Cpu {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let cpuinfo = read_file("/proc/cpuinfo")?;
        let (name, threads, freq) = parse_cpuinfo(&cpuinfo);
        let cores = count_physical_cores(&cpuinfo).max(1);

        let usage = sample_usage()?;
        let temp = read_temp();

        let data = CpuData {
            name,
            cores,
            threads,
            usage: round2(usage),
            freq_mhz: round2(freq),
            temp_c: temp.map(round2),
        };
        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "cpu".into(),
            message: err.to_string(),
        })
    }
}

fn read_file(path: &str) -> Result<String, CollectorError> {
    std::fs::read_to_string(path).map_err(|err| CollectorError::Parse {
        origin: path.into(),
        message: err.to_string(),
    })
}

/// Returns `(model_name, logical_count, avg_mhz)`.
fn parse_cpuinfo(text: &str) -> (String, u32, f32) {
    let mut model = String::new();
    let mut threads = 0u32;
    let mut total_mhz = 0f32;
    let mut mhz_samples = 0u32;

    for line in text.lines() {
        if let Some(value) = line.strip_prefix("model name") {
            if model.is_empty() {
                model = value
                    .trim_start_matches([':', ' ', '\t'])
                    .trim()
                    .to_string();
            }
        } else if line.starts_with("processor") {
            threads += 1;
        } else if let Some(value) = line.strip_prefix("cpu MHz") {
            if let Ok(mhz) = value
                .trim_start_matches([':', ' ', '\t'])
                .trim()
                .parse::<f32>()
            {
                total_mhz += mhz;
                mhz_samples += 1;
            }
        }
    }

    let avg_mhz = if mhz_samples > 0 {
        total_mhz / mhz_samples as f32
    } else {
        0.0
    };
    let model = if model.is_empty() {
        "unknown".into()
    } else {
        model
    };
    (model, threads.max(1), avg_mhz)
}

fn count_physical_cores(text: &str) -> u32 {
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut current_phys: Option<String> = None;
    let mut current_core: Option<String> = None;
    let mut fallback_threads = 0u32;

    for line in text.lines() {
        if line.starts_with("processor") {
            fallback_threads += 1;
            if let (Some(p), Some(c)) = (current_phys.take(), current_core.take()) {
                seen.insert((p, c));
            }
        } else if let Some(value) = line.strip_prefix("physical id") {
            current_phys = Some(
                value
                    .trim_start_matches([':', ' ', '\t'])
                    .trim()
                    .to_string(),
            );
        } else if let Some(value) = line.strip_prefix("core id") {
            current_core = Some(
                value
                    .trim_start_matches([':', ' ', '\t'])
                    .trim()
                    .to_string(),
            );
        }
    }
    if let (Some(p), Some(c)) = (current_phys, current_core) {
        seen.insert((p, c));
    }

    if seen.is_empty() {
        fallback_threads
    } else {
        seen.len() as u32
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct CpuTimes {
    active: u64,
    total: u64,
}

fn read_cpu_times() -> Result<CpuTimes, CollectorError> {
    let text = read_file("/proc/stat")?;
    let line = text.lines().next().ok_or_else(|| CollectorError::Parse {
        origin: "/proc/stat".into(),
        message: "empty file".into(),
    })?;
    parse_cpu_line(line).ok_or_else(|| CollectorError::Parse {
        origin: "/proc/stat".into(),
        message: format!("could not parse `{line}`"),
    })
}

fn parse_cpu_line(line: &str) -> Option<CpuTimes> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    let values: Vec<u64> = parts.filter_map(|s| s.parse().ok()).collect();
    // Fields: user, nice, system, idle, iowait, irq, softirq, steal, guest, guest_nice
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    let total: u64 = values.iter().sum();
    let active = total.saturating_sub(idle);
    Some(CpuTimes { active, total })
}

fn sample_usage() -> Result<f32, CollectorError> {
    let first = read_cpu_times()?;
    std::thread::sleep(SAMPLE_WINDOW);
    let second = read_cpu_times()?;
    let active = second.active.saturating_sub(first.active) as f64;
    let total = second.total.saturating_sub(first.total) as f64;
    if total <= 0.0 {
        return Ok(0.0);
    }
    Ok(((active / total) * 100.0).clamp(0.0, 100.0) as f32)
}

fn read_temp() -> Option<f32> {
    let entries = std::fs::read_dir("/sys/class/hwmon").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = std::fs::read_to_string(path.join("name")).ok()?;
        let trimmed = name.trim();
        if matches!(trimmed, "coretemp" | "k10temp" | "zenpower") {
            // temp1_input reports millidegrees.
            if let Ok(milli) = std::fs::read_to_string(path.join("temp1_input"))
                && let Ok(val) = milli.trim().parse::<i32>()
            {
                return Some(val as f32 / 1000.0);
            }
        }
    }
    None
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "processor	: 0\nmodel name	: AMD Ryzen 9 7950X 16-Core Processor\nphysical id	: 0\ncore id	: 0\ncpu MHz		: 3000.000\n\nprocessor	: 1\nphysical id	: 0\ncore id	: 0\ncpu MHz		: 3200.000\n\nprocessor	: 2\nphysical id	: 0\ncore id	: 1\ncpu MHz		: 4000.000\n\n";

    #[test]
    fn parses_model_and_thread_count() {
        let (name, threads, avg) = parse_cpuinfo(FIXTURE);
        assert!(name.contains("Ryzen"));
        assert_eq!(threads, 3);
        assert!((avg - 3400.0).abs() < 0.01);
    }

    #[test]
    fn dedupes_smt_siblings_for_physical_core_count() {
        // Threads 0 + 1 share core_id=0 → 2 physical cores total.
        assert_eq!(count_physical_cores(FIXTURE), 2);
    }

    #[test]
    fn parses_cpu_total_line() {
        let line = "cpu  100 200 300 400 50 0 0 0 0 0";
        let times = parse_cpu_line(line).unwrap();
        // idle = 400 + 50 = 450; total = sum = 1050; active = 600.
        assert_eq!(times.active, 600);
        assert_eq!(times.total, 1050);
    }
}
