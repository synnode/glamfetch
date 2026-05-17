//! Collector layer (spec §4.3).
//!
//! Each collector implements [`Collector`] and returns a `serde_json::Value`.
//! The [`CollectorRegistry`] runs every registered collector once, caches
//! the result, and serves lookups by dotted path (e.g. `"system.hostname"`).
//!
//! Phase 1 is sequential. Parallel collection via `rayon` lands in Phase 4
//! when the collector set grows enough to justify it.

pub mod cpu;
pub mod kernel;
pub mod memory;
pub mod os;
pub mod system;

use std::collections::HashMap;

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

    let mut out = Map::new();
    let mut errors = Map::new();

    for collector in all() {
        let name = collector.name();
        match collector.collect() {
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
        Box::new(cpu::Cpu),
        Box::new(memory::Memory),
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

    /// Run every collector in `collectors` and stash the result. If `filter`
    /// is `Some`, only collectors whose `name()` appears in the set actually
    /// run — this is the pre-pass optimisation from spec §4.3.
    ///
    /// Phase 1 calls this with `filter = None` for simplicity; the layout
    /// pre-pass that builds the referenced set lands in Phase 4.
    pub fn prime(
        &mut self,
        collectors: &[Box<dyn Collector>],
        filter: Option<&std::collections::HashSet<&str>>,
    ) {
        for collector in collectors {
            let name = collector.name();
            if let Some(filter) = filter
                && !filter.contains(name)
            {
                continue;
            }
            let result = collector.collect();
            self.cache.insert(name, result);
        }
    }

    /// Test/mock helper: inject a pre-built value for a collector name.
    #[allow(dead_code)] // Used by tests and (in Phase 4) by the pre-pass.
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
