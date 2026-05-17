//! `gpu` collector (spec §7). Reads `/sys/class/drm/card*/device/{vendor,device}`.
//!
//! v0.1 strategy: identify vendor by PCI ID against a minimal embedded
//! lookup table. Model name detection beyond that requires either a PCI
//! database or `lspci`, both out of scope for now — the field stays empty.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct Gpu;

#[derive(Debug, Serialize, Clone)]
struct Adapter {
    vendor: String,
    model: String,
    driver: Option<String>,
}

#[derive(Debug, Serialize)]
struct GpuData {
    present: bool,
    primary: Option<Adapter>,
    all: Vec<Adapter>,
}

const VENDORS: &[(&str, &str)] = &[("0x10de", "NVIDIA"), ("0x1002", "AMD"), ("0x8086", "Intel")];

impl Collector for Gpu {
    fn name(&self) -> &'static str {
        "gpu"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let adapters = scan_drm().unwrap_or_default();
        let data = GpuData {
            present: !adapters.is_empty(),
            primary: adapters.first().cloned(),
            all: adapters,
        };
        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "gpu".into(),
            message: err.to_string(),
        })
    }
}

fn scan_drm() -> Option<Vec<Adapter>> {
    let entries = std::fs::read_dir("/sys/class/drm").ok()?;
    let mut found: Vec<Adapter> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        // Real GPUs surface as `card0`, `card1`, ...; renderD* + control* are
        // duplicates of the same hardware via different ioctl interfaces.
        if !name_s.starts_with("card") || name_s.contains('-') || name_s.starts_with("controlD") {
            continue;
        }
        let device = entry.path().join("device");
        let vendor_id = read_first_line(&device.join("vendor"));
        if vendor_id.is_none() {
            continue;
        }
        let vendor = vendor_name(vendor_id.as_deref().unwrap_or(""));
        let driver = read_driver(&device);
        found.push(Adapter {
            vendor,
            model: String::new(),
            driver,
        });
    }
    if found.is_empty() { None } else { Some(found) }
}

fn vendor_name(id: &str) -> String {
    VENDORS
        .iter()
        .find(|(prefix, _)| id.eq_ignore_ascii_case(prefix))
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| id.to_string())
}

fn read_first_line(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
}

fn read_driver(device: &Path) -> Option<String> {
    let link = std::fs::read_link(device.join("driver")).ok()?;
    link.file_name().map(|s| s.to_string_lossy().into_owned())
}
