//! `uptime` collector (spec §7). Reads `/proc/uptime`.

use serde::Serialize;
use serde_json::Value;

use crate::config::filters::humanize_seconds;
use crate::error::CollectorError;

use super::Collector;

pub struct Uptime;

#[derive(Debug, Serialize)]
struct UptimeData {
    seconds: u64,
    pretty: String,
}

impl Collector for Uptime {
    fn name(&self) -> &'static str {
        "uptime"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let text =
            std::fs::read_to_string("/proc/uptime").map_err(|err| CollectorError::Parse {
                origin: "/proc/uptime".into(),
                message: err.to_string(),
            })?;
        let secs = parse_uptime(&text).ok_or_else(|| CollectorError::Parse {
            origin: "/proc/uptime".into(),
            message: "could not parse first float".into(),
        })?;
        let data = UptimeData {
            seconds: secs as u64,
            pretty: humanize_seconds(secs),
        };
        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "uptime".into(),
            message: err.to_string(),
        })
    }
}

fn parse_uptime(text: &str) -> Option<f64> {
    text.split_whitespace().next().and_then(|s| s.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_first_float() {
        assert_eq!(parse_uptime("12345.67 89012.34\n"), Some(12345.67));
    }

    #[test]
    fn missing_returns_none() {
        assert_eq!(parse_uptime(""), None);
    }
}
