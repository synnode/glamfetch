//! `disk` collector (spec §7). `/proc/mounts` + `statvfs()`.

use std::collections::HashSet;
use std::ffi::CString;

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct Disk;

#[derive(Debug, Serialize)]
struct Mount {
    path: String,
    total: u64,
    used: u64,
    free: u64,
    percent: f32,
    fs: String,
}

#[derive(Debug, Serialize)]
struct DiskData {
    total_bytes: u64,
    used_bytes: u64,
    free_bytes: u64,
    percent: f32,
    mounts: Vec<Mount>,
}

const PSEUDO_FILESYSTEMS: &[&str] = &[
    "tmpfs",
    "devtmpfs",
    "proc",
    "sysfs",
    "cgroup",
    "cgroup2",
    "pstore",
    "bpf",
    "tracefs",
    "debugfs",
    "securityfs",
    "configfs",
    "fusectl",
    "mqueue",
    "hugetlbfs",
    "rpc_pipefs",
    "ramfs",
    "binfmt_misc",
    "autofs",
    "fuse.gvfsd-fuse",
    "fuse.portal",
    "nsfs",
    "overlay",
    "squashfs",
];

impl Collector for Disk {
    fn name(&self) -> &'static str {
        "disk"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let text =
            std::fs::read_to_string("/proc/mounts").map_err(|err| CollectorError::Parse {
                origin: "/proc/mounts".into(),
                message: err.to_string(),
            })?;

        let mut seen_devices = HashSet::new();
        let mut mounts: Vec<Mount> = Vec::new();
        let mut total_sum = 0u64;
        let mut used_sum = 0u64;
        let mut free_sum = 0u64;

        for entry in parse_mounts(&text) {
            if PSEUDO_FILESYSTEMS.contains(&entry.fs.as_str()) {
                continue;
            }
            // Skip bind/duplicate mounts of the same device.
            if !seen_devices.insert(entry.device.clone()) {
                continue;
            }

            let Some((total, free)) = statvfs(&entry.path) else {
                continue;
            };
            let used = total.saturating_sub(free);
            let percent = if total > 0 {
                ((used as f64 / total as f64) * 100.0) as f32
            } else {
                0.0
            };

            mounts.push(Mount {
                path: entry.path.clone(),
                total,
                used,
                free,
                percent: round2(percent),
                fs: entry.fs.clone(),
            });

            total_sum = total_sum.saturating_add(total);
            used_sum = used_sum.saturating_add(used);
            free_sum = free_sum.saturating_add(free);
        }

        let percent = if total_sum > 0 {
            ((used_sum as f64 / total_sum as f64) * 100.0) as f32
        } else {
            0.0
        };

        let data = DiskData {
            total_bytes: total_sum,
            used_bytes: used_sum,
            free_bytes: free_sum,
            percent: round2(percent),
            mounts,
        };

        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "disk".into(),
            message: err.to_string(),
        })
    }
}

struct MountEntry {
    device: String,
    path: String,
    fs: String,
}

fn parse_mounts(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let device = parts.next()?.to_string();
            let path = parts.next()?.to_string();
            let fs = parts.next()?.to_string();
            Some(MountEntry { device, path, fs })
        })
        .collect()
}

/// Returns `(total_bytes, free_bytes)` or `None` if the mount is unreachable.
fn statvfs(path: &str) -> Option<(u64, u64)> {
    let c_path = CString::new(path).ok()?;
    let stats = rustix::fs::statvfs(c_path.as_c_str()).ok()?;
    let block = stats.f_frsize as u64;
    let total = stats.f_blocks as u64 * block;
    let free = stats.f_bavail as u64 * block;
    Some((total, free))
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mount_lines() {
        let text = "/dev/nvme0n1p2 / ext4 rw,relatime 0 0\ntmpfs /tmp tmpfs rw,nosuid 0 0\n";
        let entries = parse_mounts(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].device, "/dev/nvme0n1p2");
        assert_eq!(entries[0].fs, "ext4");
    }
}
