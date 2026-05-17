//! Filter pipeline applied after expression lookup (spec §6.4).
//!
//! Each filter takes the incoming [`Value`] plus zero or more parsed
//! arguments and returns a new `Value`. Filters chain left-to-right:
//! `${data.uptime|round(0)|pad(6)}`.

use serde_json::Value;

use crate::error::ConfigError;

#[derive(Debug, Clone)]
pub struct Filter {
    pub name: String,
    pub args: Vec<String>,
}

impl Filter {
    pub fn apply(&self, value: Value) -> Result<Value, ConfigError> {
        match self.name.as_str() {
            "humanize" => Ok(filter_humanize(&value, &self.args)),
            "round" => filter_round(value, &self.args),
            "truncate" => filter_truncate(value, &self.args),
            "upper" => Ok(map_string(value, |s| s.to_uppercase())),
            "lower" => Ok(map_string(value, |s| s.to_lowercase())),
            "title" => Ok(map_string(value, title_case)),
            "pad" => filter_pad(value, &self.args),
            "default" => Ok(filter_default(value, &self.args)),
            other => Err(ConfigError::Invalid(format!("unknown filter `{other}`"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Individual filters
// ---------------------------------------------------------------------------

fn filter_humanize(value: &Value, args: &[String]) -> Value {
    let n = match value {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        _ => return value.clone(),
    };

    let unit = args.first().map(String::as_str).unwrap_or("seconds");
    let formatted = match unit {
        "bytes" => humanize_bytes(n),
        _ => humanize_seconds(n),
    };
    Value::String(formatted)
}

pub fn humanize_seconds(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let days = s / 86_400;
    let hours = (s % 86_400) / 3600;
    let mins = (s % 3600) / 60;
    let secs = s % 60;

    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 || !parts.is_empty() {
        parts.push(format!("{hours}h"));
    }
    if mins > 0 || !parts.is_empty() {
        parts.push(format!("{mins}m"));
    }
    if parts.is_empty() {
        parts.push(format!("{secs}s"));
    }
    parts.join(" ")
}

pub fn humanize_bytes(bytes: f64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value as u64, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn filter_round(value: Value, args: &[String]) -> Result<Value, ConfigError> {
    let n = parse_arg::<usize>(args, 0).unwrap_or(0);
    let f = match &value {
        Value::Number(num) => num.as_f64().unwrap_or(0.0),
        Value::String(s) => s
            .parse::<f64>()
            .map_err(|_| ConfigError::Invalid(format!("round: cannot parse `{s}` as number")))?,
        _ => {
            return Err(ConfigError::Invalid(
                "round: expected number, got non-numeric".into(),
            ));
        }
    };
    let mult = 10f64.powi(n as i32);
    let rounded = (f * mult).round() / mult;
    Ok(Value::String(if n == 0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.*}", n)
    }))
}

fn filter_truncate(value: Value, args: &[String]) -> Result<Value, ConfigError> {
    let n = parse_arg::<usize>(args, 0)
        .ok_or_else(|| ConfigError::Invalid("truncate: missing length argument".into()))?;
    let s = value_to_display(&value);
    if s.chars().count() <= n {
        return Ok(Value::String(s));
    }
    let truncated: String = s.chars().take(n.saturating_sub(1)).collect();
    Ok(Value::String(format!("{truncated}…")))
}

fn filter_pad(value: Value, args: &[String]) -> Result<Value, ConfigError> {
    let n = parse_arg::<usize>(args, 0)
        .ok_or_else(|| ConfigError::Invalid("pad: missing width argument".into()))?;
    let ch = args.get(1).and_then(|s| s.chars().next()).unwrap_or(' ');
    let s = value_to_display(&value);
    let current = s.chars().count();
    if current >= n {
        return Ok(Value::String(s));
    }
    let pad: String = std::iter::repeat_n(ch, n - current).collect();
    Ok(Value::String(format!("{s}{pad}")))
}

fn filter_default(value: Value, args: &[String]) -> Value {
    let is_missing =
        matches!(&value, Value::Null) || matches!(&value, Value::String(s) if s.is_empty());
    if is_missing {
        Value::String(args.first().cloned().unwrap_or_default())
    } else {
        value
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_arg<T: std::str::FromStr>(args: &[String], idx: usize) -> Option<T> {
    args.get(idx).and_then(|s| s.parse().ok())
}

fn map_string(value: Value, f: impl FnOnce(&str) -> String) -> Value {
    let s = value_to_display(&value);
    Value::String(f(&s))
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cap_next = true;
    for ch in s.chars() {
        if ch.is_whitespace() {
            cap_next = true;
            out.push(ch);
        } else if cap_next {
            out.extend(ch.to_uppercase());
            cap_next = false;
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

/// Render a JSON value the way a user would expect it in a template:
/// strings raw, numbers/bools via Display, null/array/object → empty/JSON.
pub fn value_to_display(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn apply(name: &str, args: &[&str], value: Value) -> Value {
        Filter {
            name: name.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
        .apply(value)
        .unwrap()
    }

    #[test]
    fn humanize_seconds_basic() {
        assert_eq!(humanize_seconds(0.0), "0s");
        assert_eq!(humanize_seconds(45.0), "45s");
        assert_eq!(humanize_seconds(3661.0), "1h 1m");
        assert_eq!(humanize_seconds(90061.0), "1d 1h 1m");
    }

    #[test]
    fn humanize_bytes_basic() {
        assert_eq!(humanize_bytes(512.0), "512 B");
        assert_eq!(humanize_bytes(2048.0), "2.0 KB");
        assert_eq!(humanize_bytes(1_572_864.0), "1.5 MB");
    }

    #[test]
    fn round_int_and_decimal() {
        assert_eq!(apply("round", &[], json!(3.7)), json!("4"));
        assert_eq!(apply("round", &["2"], json!(1.23456)), json!("1.23"));
    }

    #[test]
    fn truncate_appends_ellipsis() {
        assert_eq!(
            apply("truncate", &["5"], json!("hello world")),
            json!("hell…")
        );
        assert_eq!(apply("truncate", &["20"], json!("short")), json!("short"));
    }

    #[test]
    fn pad_extends() {
        assert_eq!(apply("pad", &["6"], json!("hi")), json!("hi    "));
    }

    #[test]
    fn default_replaces_missing() {
        assert_eq!(apply("default", &["n/a"], json!(null)), json!("n/a"));
        assert_eq!(apply("default", &["n/a"], json!("")), json!("n/a"));
        assert_eq!(apply("default", &["n/a"], json!("real")), json!("real"));
    }

    #[test]
    fn title_case_capitalizes_words() {
        assert_eq!(
            apply("title", &[], json!("hello world")),
            json!("Hello World")
        );
    }
}
