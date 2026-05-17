//! `kernel` collector — `uname()` via `rustix` (spec §7).

use serde::Serialize;
use serde_json::Value;

use crate::error::CollectorError;

use super::Collector;

pub struct Kernel;

#[derive(Debug, Serialize)]
struct KernelData {
    name: String,
    version: String,
    arch: String,
}

impl Collector for Kernel {
    fn name(&self) -> &'static str {
        "kernel"
    }

    fn collect(&self) -> Result<Value, CollectorError> {
        let uname = rustix::system::uname();
        let data = KernelData {
            name: uname.sysname().to_string_lossy().into_owned(),
            version: uname.release().to_string_lossy().into_owned(),
            arch: uname.machine().to_string_lossy().into_owned(),
        };

        serde_json::to_value(data).map_err(|err| CollectorError::Parse {
            origin: "kernel".into(),
            message: err.to_string(),
        })
    }
}
