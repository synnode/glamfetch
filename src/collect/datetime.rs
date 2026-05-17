//! `datetime` collector (spec §7). Local-time snapshot via `chrono`.

use chrono::{Datelike, Local};
use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct DateTime;

#[derive(Debug, Serialize)]
struct DateTimeData {
    time: String,
    date: String,
    iso: String,
    weekday: String,
    timestamp: i64,
}

impl Collector for DateTime {
    fn name(&self) -> &'static str {
        "datetime"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let now = Local::now();
        let data = DateTimeData {
            time: now.format("%H:%M:%S").to_string(),
            date: now.format("%Y-%m-%d").to_string(),
            iso: now.to_rfc3339(),
            weekday: now.weekday().to_string(),
            timestamp: now.timestamp(),
        };
        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "datetime".into(),
            message: err.to_string(),
        })
    }
}
