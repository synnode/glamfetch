//! `packages` collector (spec §7). Filesystem-based counts per manager.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::CollectorError;

use super::Collector;

pub struct Packages;

#[derive(Debug, Serialize)]
struct PackagesData {
    total: u32,
    by_manager: Map<String, Value>,
}

impl Collector for Packages {
    fn name(&self) -> &'static str {
        "packages"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let mut by_manager = Map::new();
        let mut total = 0u32;

        for (name, count) in counts() {
            total += count;
            by_manager.insert(name.into(), Value::from(count));
        }

        let data = PackagesData { total, by_manager };
        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "packages".into(),
            message: err.to_string(),
        })
    }
}

fn counts() -> Vec<(&'static str, u32)> {
    let mut out = Vec::new();

    if let Some(n) = count_pacman() {
        out.push(("pacman", n));
    }
    if let Some(n) = count_flatpak() {
        out.push(("flatpak", n));
    }
    if let Some(n) = count_snap() {
        out.push(("snap", n));
    }
    if let Some(n) = count_apt() {
        out.push(("apt", n));
    }

    out
}

fn count_pacman() -> Option<u32> {
    let dir = Path::new("/var/lib/pacman/local");
    if !dir.exists() {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let n = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .count();
    Some(n as u32)
}

fn count_flatpak() -> Option<u32> {
    let mut paths: Vec<PathBuf> = vec![PathBuf::from("/var/lib/flatpak/app")];
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".local/share/flatpak/app"));
    }

    let mut total = 0u32;
    let mut any = false;
    for path in paths {
        if !path.exists() {
            continue;
        }
        any = true;
        if let Ok(entries) = std::fs::read_dir(path) {
            total += entries
                .flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .count() as u32;
        }
    }

    if any { Some(total) } else { None }
}

fn count_snap() -> Option<u32> {
    let dir = Path::new("/snap");
    if !dir.exists() {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let n = entries
        .flatten()
        .filter(|e| {
            // Skip `bin` symlink dir and `README`.
            let name = e.file_name();
            let s = name.to_string_lossy();
            s != "bin" && s != "README"
        })
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .count();
    Some(n as u32)
}

fn count_apt() -> Option<u32> {
    let path = Path::new("/var/lib/dpkg/status");
    if !path.exists() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let n = count_lines_starting_with(&bytes, b"Package:");
    Some(n as u32)
}

fn count_lines_starting_with(data: &[u8], needle: &[u8]) -> usize {
    let mut count = 0;
    let mut at_start = true;
    let mut i = 0;
    while i < data.len() {
        if at_start && data[i..].starts_with(needle) {
            count += 1;
        }
        at_start = data[i] == b'\n';
        i += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_package_lines() {
        let data = b"Package: foo\nVersion: 1\n\nPackage: bar\nVersion: 2\n";
        assert_eq!(count_lines_starting_with(data, b"Package:"), 2);
    }
}
