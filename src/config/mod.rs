//! Config loading + path resolution (spec §6.1).
//!
//! Phase 0: load + parse only. `extends` chains, deep-merging, and the
//! expression evaluator land in Phase 2.

pub mod color_spec;
pub mod expr;
pub mod extends;
pub mod filters;
pub mod prepass;
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

/// Parsed config plus the original TOML text. The text is kept so the
/// collector pre-pass can scan for `${data.<root>}` references without
/// re-serialising.
pub struct LoadedConfig {
    pub text: String,
    pub config: ConfigFile,
}

/// Load + parse a config file from disk, expanding any `extends` chain.
pub fn load_from_path(path: &Path) -> Result<LoadedConfig, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let origin_dir = path.parent();
    let config = parse_with_extends(&text, path, origin_dir)?;
    Ok(LoadedConfig { text, config })
}

/// Parse the embedded default preset.
pub fn load_embedded_default() -> Result<LoadedConfig, ConfigError> {
    let path = Path::new("<embedded:default>");
    let config = parse_with_extends(DEFAULT_PRESET, path, None)?;
    Ok(LoadedConfig {
        text: DEFAULT_PRESET.to_string(),
        config,
    })
}

/// Parse + expand `extends` + materialise into the typed [`ConfigFile`].
fn parse_with_extends(
    text: &str,
    path: &Path,
    origin_dir: Option<&Path>,
) -> Result<ConfigFile, ConfigError> {
    let raw: toml::Value = toml::from_str(text).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    let expanded = extends::expand(raw, origin_dir)?;
    expanded.try_into::<ConfigFile>().map_err(|err| {
        ConfigError::Invalid(format!(
            "{} (after extends expansion): {err}",
            path.display()
        ))
    })
}
