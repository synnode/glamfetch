//! Collector layer (spec §4.3).
//!
//! Each collector implements [`Collector`] and returns a `serde_json::Value`.
//! The [`CollectorRegistry`] runs every registered collector once (in
//! parallel via `rayon`), caches the result, and serves lookups by dotted
//! path (e.g. `"system.hostname"`).
//!
//! [`CollectorRegistry::prime`] takes an optional filter — the layout
//! pre-pass passes the set of collector roots actually referenced via
//! `${data.<root>.*}` so we never run a collector nobody asked for.

pub mod battery;
pub mod cpu;
pub mod datetime;
pub mod desktop;
pub mod disk;
pub mod gpu;
pub mod kernel;
pub mod memory;
pub mod network;
pub mod os;
pub mod packages;
pub mod system;
pub mod uptime;

use std::collections::{HashMap, HashSet};

use rayon::prelude::*;
use serde_json::Value;

use crate::error::CollectorError;

pub trait Collector: Send + Sync {
    /// Stable name used in config references (`${data.<name>....}`).
    fn name(&self) -> &'static str;

    fn collect(&self) -> Result<Value, CollectorError>;
}

/// Run every shipped collector and serialise to a single JSON object.
///
/// Successful collectors land at their `name()` key; failures collapse to
/// `null` at that key and record an error message under `_errors`. The
/// result is what `--json` emits.
pub fn collect_all_as_json() -> serde_json::Value {
    use serde_json::{Map, Value, json};

    let collectors = all();
    let results: Vec<(&'static str, Result<Value, CollectorError>)> = collectors
        .par_iter()
        .map(|c| (c.name(), c.collect()))
        .collect();

    let mut out = Map::new();
    let mut errors = Map::new();
    for (name, result) in results {
        match result {
            Ok(value) => {
                out.insert(name.to_string(), value);
            }
            Err(err) => {
                out.insert(name.to_string(), Value::Null);
                errors.insert(name.to_string(), json!(err.to_string()));
            }
        }
    }

    if !errors.is_empty() {
        out.insert("_errors".into(), Value::Object(errors));
    }
    Value::Object(out)
}

/// All collectors shipped with the binary.
pub fn all() -> Vec<Box<dyn Collector>> {
    vec![
        Box::new(system::System),
        Box::new(os::Os),
        Box::new(kernel::Kernel),
        Box::new(uptime::Uptime),
        Box::new(cpu::Cpu),
        Box::new(memory::Memory),
        Box::new(disk::Disk),
        Box::new(gpu::Gpu),
        Box::new(battery::Battery),
        Box::new(network::Network),
        Box::new(packages::Packages),
        Box::new(desktop::Desktop),
        Box::new(datetime::DateTime),
    ]
}

/// Runs collectors once and serves their output by dotted path.
#[derive(Default)]
pub struct CollectorRegistry {
    cache: HashMap<&'static str, Result<Value, CollectorError>>,
}

impl CollectorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run every collector in `collectors` in parallel and stash results.
    ///
    /// If `filter` is `Some`, only collectors whose `name()` appears in the
    /// set actually run — this is the pre-pass optimisation that drops
    /// cold-start time for minimal configs.
    pub fn prime(&mut self, collectors: &[Box<dyn Collector>], filter: Option<&HashSet<String>>) {
        // Filter first, then run the remainder in parallel.
        let scheduled: Vec<&Box<dyn Collector>> = collectors
            .iter()
            .filter(|c| match filter {
                Some(set) => set.contains(c.name()),
                None => true,
            })
            .collect();

        let results: Vec<(&'static str, Result<Value, CollectorError>)> = scheduled
            .par_iter()
            .map(|c| (c.name(), c.collect()))
            .collect();

        for (name, result) in results {
            self.cache.insert(name, result);
        }
    }

    /// Test/mock helper: inject a pre-built value for a collector name.
    #[allow(dead_code)]
    pub fn insert(&mut self, name: &'static str, value: Value) {
        self.cache.insert(name, Ok(value));
    }

    /// Lookup by dotted path (`"system.hostname"` → `cache["system"]["hostname"]`).
    /// Returns `None` if the collector failed or the path doesn't exist.
    pub fn get(&self, path: &str) -> Option<&Value> {
        let (root, rest) = match path.split_once('.') {
            Some((r, rest)) => (r, Some(rest)),
            None => (path, None),
        };

        let value = self.cache.get(root)?.as_ref().ok()?;
        match rest {
            None => Some(value),
            Some(rest) => walk(value, rest),
        }
    }
}

fn walk<'a>(mut value: &'a Value, path: &str) -> Option<&'a Value> {
    for segment in path.split('.') {
        value = match value {
            Value::Object(map) => map.get(segment)?,
            _ => return None,
        };
    }
    Some(value)
}
