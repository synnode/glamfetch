//! Config loading + path resolution (spec §6.1).
//!
//! Phase 0: load + parse only. `extends` chains, deep-merging, and the
//! expression evaluator land in Phase 2.

pub mod expr;
pub mod filters;
pub mod schema;

use std::path::{Path, PathBuf};

use crate::error::ConfigError;

pub use schema::ConfigFile;

/// Embedded default preset, used when no config file is present.
pub const DEFAULT_PRESET: &str = include_str!("../../presets/default.toml");

/// Resolve the config path per spec §6.1.
///
/// Precedence:
/// 1. `--config <path>` (caller passes via `override_path`)
/// 2. `$GLAMFETCH_CONFIG` (clap already merges this into `override_path`)
/// 3. `$XDG_CONFIG_HOME/glamfetch/config.toml`
/// 4. `~/.config/glamfetch/config.toml`
pub fn resolve_path(override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = override_path {
        return Some(p.to_path_buf());
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let p = PathBuf::from(xdg).join("glamfetch/config.toml");
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(home).join(".config/glamfetch/config.toml");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Default target for `--init`. Mirrors [`resolve_path`] preference order
/// but does not require the file to exist.
pub fn default_init_target() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("glamfetch/config.toml");
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
    PathBuf::from(home).join(".config/glamfetch/config.toml")
}

/// Load + parse a config file from disk.
pub fn load_from_path(path: &Path) -> Result<ConfigFile, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&text, path)
}

/// Parse the embedded default preset.
pub fn load_embedded_default() -> Result<ConfigFile, ConfigError> {
    parse(DEFAULT_PRESET, Path::new("<embedded:default>"))
}

fn parse(text: &str, path: &Path) -> Result<ConfigFile, ConfigError> {
    toml::from_str(text).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })
}
