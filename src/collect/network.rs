//! `network` collector (spec §7). `/sys/class/net/*` + `/proc/net/route`.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct Network;

#[derive(Debug, Serialize, Clone)]
struct Interface {
    name: String,
    ip4: Option<String>,
    ip6: Option<String>,
    mac: String,
    up: bool,
}

#[derive(Debug, Serialize)]
struct Primary {
    name: String,
    ip4: Option<String>,
    ssid: Option<String>,
}

#[derive(Debug, Serialize)]
struct NetworkData {
    interfaces: Vec<Interface>,
    primary: Option<Primary>,
    ssid: Option<String>,
}

impl Collector for Network {
    fn name(&self) -> &'static str {
        "network"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let interfaces = scan_interfaces().unwrap_or_default();
        let primary_name = default_route_iface();

        let primary = primary_name.as_ref().and_then(|name| {
            interfaces.iter().find(|i| &i.name == name).map(|i| {
                let ssid = if i.name.starts_with("wl") {
                    read_ssid()
                } else {
                    None
                };
                Primary {
                    name: i.name.clone(),
                    ip4: i.ip4.clone(),
                    ssid,
                }
            })
        });

        let ssid = primary.as_ref().and_then(|p| p.ssid.clone());

        let data = NetworkData {
            interfaces,
            primary,
            ssid,
        };
        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "network".into(),
            message: err.to_string(),
        })
    }
}

fn scan_interfaces() -> Option<Vec<Interface>> {
    let entries = std::fs::read_dir("/sys/class/net").ok()?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "lo" {
            continue;
        }
        let path = entry.path();
        let mac = read_first_line(&path.join("address")).unwrap_or_default();
        let up = read_first_line(&path.join("operstate"))
            .as_deref()
            .map(|s| s == "up")
            .unwrap_or(false);
        out.push(Interface {
            name,
            ip4: None,
            ip6: None,
            mac,
            up,
        });
    }
    Some(out)
}

/// Parse `/proc/net/route` for the default route's interface.
///
/// Format (after header): `iface dest gateway flags ...`. Destination
/// `00000000` (hex little-endian) is the default route entry.
fn default_route_iface() -> Option<String> {
    let text = std::fs::read_to_string("/proc/net/route").ok()?;
    for line in text.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let iface = parts.next()?;
        let dest = parts.next()?;
        if dest == "00000000" {
            return Some(iface.to_string());
        }
    }
    None
}

/// Subprocess: `iwgetid -r`. Typically <2ms. Spec acceptance per §7 network.
fn read_ssid() -> Option<String> {
    let output = std::process::Command::new("iwgetid")
        .arg("-r")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn read_first_line(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
}
