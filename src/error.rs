//! Top-level error taxonomy for glamfetch.
//!
//! Binary entry points use `anyhow::Result` for convenience; library-style
//! modules return concrete variants of these `thiserror`-derived enums so
//! callers can branch on cause. See spec §12 for the policy mapping each
//! variant to a user-visible behavior.

#![allow(dead_code)] // Phase 0: variants wired in later phases.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(PathBuf),

    #[error("failed to read config file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse TOML in {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("`extends` target not found: {0}")]
    ExtendsNotFound(String),

    #[error("undefined theme variable: ${{{0}}}")]
    UndefinedThemeVar(String),

    #[error("invalid config: {0}")]
    Invalid(String),
}

#[derive(Debug, Error)]
pub enum CollectorError {
    #[error("collector `{name}` failed: {source}")]
    Failed {
        name: &'static str,
        #[source]
        source: anyhow::Error,
    },

    #[error("data source unavailable: {0}")]
    Unavailable(String),

    #[error("failed to parse data from {origin}: {message}")]
    Parse { origin: String, message: String },
}

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("widget `{widget}` failed to render: {message}")]
    Widget {
        widget: &'static str,
        message: String,
    },

    #[error("terminal too narrow: need at least {needed} columns, have {have}")]
    TerminalTooNarrow { needed: usize, have: usize },

    #[error("write to output failed: {0}")]
    Io(#[from] std::io::Error),
}
