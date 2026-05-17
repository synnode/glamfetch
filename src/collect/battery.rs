//! `battery` collector (spec §7). `/sys/class/power_supply/BAT*`.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct Battery;

#[derive(Debug, Serialize)]
struct BatteryData {
    present: bool,
    percent: Option<u8>,
    status: Option<String>,
    time_remaining_min: Option<u32>,
}

impl Collector for Battery {
    fn name(&self) -> &'static str {
        "battery"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let data = read_battery().unwrap_or(BatteryData {
            present: false,
            percent: None,
            status: None,
            time_remaining_min: None,
        });
        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "battery".into(),
            message: err.to_string(),
        })
    }
}

fn read_battery() -> Option<BatteryData> {
    let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if !name_s.starts_with("BAT") {
            continue;
        }
        let path = entry.path();
        let percent = read_u32(&path.join("capacity")).map(|v| v.min(100) as u8);
        let status = read_string(&path.join("status"));
        let time_remaining_min = compute_time_remaining(&path);
        return Some(BatteryData {
            present: true,
            percent,
            status,
            time_remaining_min,
        });
    }
    None
}

fn compute_time_remaining(path: &Path) -> Option<u32> {
    // Prefer energy_now / power_now (Wh, W). Fall back to charge_now / current_now (Ah, A).
    let now = read_u64(&path.join("energy_now")).or_else(|| read_u64(&path.join("charge_now")))?;
    let rate = read_u64(&path.join("power_now")).or_else(|| read_u64(&path.join("current_now")))?;
    if rate == 0 {
        return None;
    }
    let status = read_string(&path.join("status")).unwrap_or_default();
    let basis = if status == "Charging" {
        read_u64(&path.join("energy_full"))
            .or_else(|| read_u64(&path.join("charge_full")))
            .map(|full| full.saturating_sub(now))?
    } else {
        now
    };
    Some(((basis as f64 / rate as f64) * 60.0) as u32)
}

fn read_string(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

fn read_u32(path: &Path) -> Option<u32> {
    read_string(path)?.parse().ok()
}

fn read_u64(path: &Path) -> Option<u64> {
    read_string(path)?.parse().ok()
}
