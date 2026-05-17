//! `os` collector — parses `/etc/os-release` (spec §7).
//!
//! The file is a tiny key=value format defined by freedesktop.org.
//! Values may be quoted with double quotes; surrounding quotes are stripped.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct Os;

#[derive(Debug, Serialize)]
struct OsData {
    name: String,
    version: String,
    id: String,
    codename: Option<String>,
}

impl Collector for Os {
    fn name(&self) -> &'static str {
        "os"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let map = read_os_release(Path::new("/etc/os-release"))?;

        let data = OsData {
            name: map.get("NAME").cloned().unwrap_or_else(|| "Linux".into()),
            version: map.get("VERSION").cloned().unwrap_or_default(),
            id: map.get("ID").cloned().unwrap_or_else(|| "linux".into()),
            codename: map.get("VERSION_CODENAME").cloned(),
        };

        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "os".into(),
            message: err.to_string(),
        })
    }
}

fn read_os_release(path: &Path) -> Result<HashMap<String, String>, CollectorError> {
    let text = std::fs::read_to_string(path).map_err(|err| CollectorError::Parse {
        origin: path.display().to_string(),
        message: err.to_string(),
    })?;
    Ok(parse_os_release(&text))
}

fn parse_os_release(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value);
        out.insert(key.to_string(), value.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_and_unquoted() {
        let text = r#"
NAME="EndeavourOS"
ID=endeavouros
VERSION="2026"
# comment
VERSION_CODENAME=mercury
"#;
        let map = parse_os_release(text);
        assert_eq!(map.get("NAME").unwrap(), "EndeavourOS");
        assert_eq!(map.get("ID").unwrap(), "endeavouros");
        assert_eq!(map.get("VERSION").unwrap(), "2026");
        assert_eq!(map.get("VERSION_CODENAME").unwrap(), "mercury");
    }

    #[test]
    fn skips_malformed_lines() {
        let map = parse_os_release("no-equals-sign\nKEY=value\n");
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("KEY").unwrap(), "value");
    }
}
