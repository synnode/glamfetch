//! Serde structs mirroring the TOML schema (spec §6).
//!
//! Phase 0 covers the top-level shape only. The widget tree under
//! `[[row]]` / `[[row.cell]]` is parsed as raw `toml::Value` for now;
//! a typed `WidgetConfig` enum lands in Phase 2 alongside the expression
//! evaluator. Keeping it untyped here means the loader can already validate
//! presence/structure without blocking on widget work.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::layout::RowConfig;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    /// Single preset name/path, or a chain (CSS-cascade order — see spec §6.6).
    #[serde(default)]
    pub extends: Option<Extends>,

    #[serde(default)]
    pub meta: Meta,

    #[serde(default)]
    pub theme: BTreeMap<String, String>,

    #[serde(default)]
    pub icons: Icons,

    #[serde(default)]
    pub layout: Layout,

    #[serde(default, rename = "row")]
    pub rows: Vec<RowConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Extends {
    Single(String),
    Chain(Vec<String>),
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Icons {
    #[serde(default = "default_icon_set")]
    pub set: String,
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
}

impl Default for Icons {
    fn default() -> Self {
        Self {
            set: default_icon_set(),
            overrides: BTreeMap::new(),
        }
    }
}

fn default_icon_set() -> String {
    "nerd-font".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default = "default_gap")]
    pub gap: usize,
    #[serde(default = "default_align")]
    pub align: String,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            gap: default_gap(),
            align: default_align(),
        }
    }
}

fn default_gap() -> usize {
    1
}

fn default_align() -> String {
    "left".to_string()
}
