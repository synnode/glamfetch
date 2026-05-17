//! Command-line interface definitions.
//!
//! See spec §10 for the full surface. Phase 0 implements only the flags that
//! the scaffold can honor end-to-end; the rest are declared so the help text
//! is final but exit with an "unimplemented in v0.1.0" message when used.

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "glamfetch",
    version,
    about = "A glamorous system-info fetch tool for the terminal.",
    long_about = None,
)]
pub struct Cli {
    /// Path to config file (overrides default resolution).
    #[arg(short, long, value_name = "PATH", env = "GLAMFETCH_CONFIG")]
    pub config: Option<PathBuf>,

    /// Plain output: strip ANSI, ASCII border fallback, no figlet.
    #[arg(long)]
    pub pipe: bool,

    /// Emit all collected data as JSON to stdout (no rendering).
    #[arg(long)]
    pub json: bool,

    /// Re-render on an interval (seconds; default 1).
    #[arg(long, value_name = "INTERVAL", num_args = 0..=1, default_missing_value = "1")]
    pub watch: Option<f32>,

    /// Open the live preview pane (pair with tmux/zellij for split).
    #[arg(long)]
    pub edit: bool,

    /// Write the default preset to the config path and exit.
    #[arg(long)]
    pub init: bool,

    /// List built-in presets and exit.
    #[arg(long)]
    pub list_presets: bool,

    /// Print the resolved config (after extends/merging) and exit.
    #[arg(long)]
    pub print_config: bool,

    /// Print collected data as a human-readable summary.
    #[arg(long)]
    pub print_data: bool,

    /// Enable debug logging to stderr.
    #[arg(short, long)]
    pub verbose: bool,
}

/// Returned by [`Cli::dispatch_kind`] to keep main.rs free of flag-priority
/// logic. The order in [`Cli::dispatch_kind`] determines precedence when
/// multiple flags are set (e.g. `--init --watch` → Init wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Init,
    ListPresets,
    PrintConfig,
    PrintData,
    Json,
    Edit,
    Watch,
    Once,
}

impl Cli {
    pub fn command(&self) -> Command {
        if self.init {
            Command::Init
        } else if self.list_presets {
            Command::ListPresets
        } else if self.print_config {
            Command::PrintConfig
        } else if self.print_data {
            Command::PrintData
        } else if self.json {
            Command::Json
        } else if self.edit {
            Command::Edit
        } else if self.watch.is_some() {
            Command::Watch
        } else {
            Command::Once
        }
    }
}
