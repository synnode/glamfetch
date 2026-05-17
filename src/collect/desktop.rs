//! `desktop` collector (spec §7). Env-var heuristics only.

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct Desktop;

#[derive(Debug, Serialize)]
struct DesktopData {
    de: Option<String>,
    wm: Option<String>,
    session_type: Option<String>,
}

impl Collector for Desktop {
    fn name(&self) -> &'static str {
        "desktop"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let de = std::env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .or_else(|| std::env::var("DESKTOP_SESSION").ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let session_type = std::env::var("XDG_SESSION_TYPE").ok().or_else(|| {
            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                Some("wayland".into())
            } else if std::env::var_os("DISPLAY").is_some() {
                Some("x11".into())
            } else {
                None
            }
        });

        // WM detection beyond env vars (e.g. wmctrl, X atom queries) is out
        // of scope for v0.1; leave as None when no env hint exists.
        let wm = std::env::var("XDG_SESSION_DESKTOP")
            .ok()
            .filter(|s| !s.is_empty());

        let data = DesktopData {
            de,
            wm,
            session_type,
        };
        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "desktop".into(),
            message: err.to_string(),
        })
    }
}
