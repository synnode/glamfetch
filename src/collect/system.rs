//! `system` collector — hostname, user, shell, terminal, locale (spec §7).

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct System;

#[derive(Debug, Serialize)]
struct SystemData {
    hostname: String,
    user: String,
    shell: String,
    terminal: String,
    locale: String,
}

impl Collector for System {
    fn name(&self) -> &'static str {
        "system"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let uname = rustix::system::uname();
        let hostname = uname.nodename().to_string_lossy().into_owned();

        let data = SystemData {
            hostname,
            user: env_or_unknown("USER"),
            shell: env_or_unknown("SHELL"),
            terminal: env_or_unknown("TERM"),
            locale: env_or_unknown("LANG"),
        };

        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "system".into(),
            message: err.to_string(),
        })
    }
}

fn env_or_unknown(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| "unknown".to_string())
}
