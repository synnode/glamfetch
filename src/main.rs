//! Glamfetch entry point.
//!
//! Thin shell over the library at `src/lib.rs`. Responsible for: CLI
//! parsing, tracing init, dispatch to the right command handler, choosing
//! the appropriate renderer for the current terminal.

#![deny(unsafe_code)]

mod cli;

use std::io::{self, Write};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::Parser;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

use glamfetch::collect::{CollectorRegistry, all as all_collectors};
use glamfetch::config::expr::StaticContext;
use glamfetch::config::prepass;
use glamfetch::config::{self, LoadedConfig};
use glamfetch::layout::{Layout, Row};
use glamfetch::render::ansi;
use glamfetch::render::terminal::Capabilities;
use glamfetch::style::StyledLine;
use glamfetch::theme;

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("glamfetch: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn init_tracing(verbose: bool) {
    let default_filter = if verbose {
        "glamfetch=debug"
    } else {
        "glamfetch=warn"
    };
    let filter =
        EnvFilter::try_from_env("GLAMFETCH_LOG").unwrap_or_else(|_| EnvFilter::new(default_filter));
    fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .without_time()
        .init();
}

fn run(cli: &Cli) -> Result<()> {
    match cli.command() {
        Command::Init => cmd_init(cli),
        Command::PrintConfig => cmd_print_config(cli),
        Command::ListPresets => cmd_list_presets(),
        Command::Once => cmd_once(cli),
        Command::Json => cmd_json(),
        Command::PrintData => cmd_print_data(),
        Command::Watch => cmd_watch(cli),
        Command::Edit => cmd_edit(cli),
    }
}

fn cmd_once(cli: &Cli) -> Result<()> {
    let loaded = load_config(cli)?;
    let frame = build_frame(loaded)?;

    let caps = if cli.pipe {
        Capabilities::forced_pipe()
    } else {
        Capabilities::detect()
    };

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    ansi::write_frame(&mut handle, &frame, caps.color)?;
    handle.flush().ok();
    Ok(())
}

fn build_frame(loaded: LoadedConfig) -> Result<Vec<StyledLine>> {
    let LoadedConfig { text, config: cfg } = loaded;

    let resolved_theme = theme::resolve(&cfg.theme).context("resolving theme variables")?;
    let ctx = StaticContext {
        theme: resolved_theme,
        icons: cfg.icons.overrides.clone(),
        env_allowed: true,
    };

    let rows: Vec<Row> = cfg
        .rows
        .into_iter()
        .map(|r| Row::build(r, &ctx))
        .collect::<Result<_, _>>()
        .context("building widget tree")?;

    // Pre-pass: only run collectors that the layout actually references.
    let referenced = prepass::referenced_data_roots(&text);
    let collectors = all_collectors();
    let mut registry = CollectorRegistry::new();
    registry.prime(&collectors, Some(&referenced));

    let layout = Layout::new(rows, cfg.layout.gap);
    Ok(layout.render(&registry)?)
}

fn cmd_init(cli: &Cli) -> Result<()> {
    let target = cli
        .config
        .clone()
        .unwrap_or_else(config::default_init_target);

    if target.exists() {
        bail!(
            "refusing to overwrite existing config at {}",
            target.display()
        );
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }

    std::fs::write(&target, config::DEFAULT_PRESET)
        .with_context(|| format!("writing default preset to {}", target.display()))?;

    println!("wrote default preset to {}", target.display());
    info!(path = %target.display(), "config initialized");
    Ok(())
}

fn cmd_print_config(cli: &Cli) -> Result<()> {
    let loaded = load_config(cli)?;
    let toml = toml::to_string_pretty(&loaded.config).context("re-serialising resolved config")?;
    print!("{toml}");
    Ok(())
}

fn cmd_list_presets() -> Result<()> {
    for (name, _) in glamfetch::config::extends::BUILTIN_PRESETS {
        println!("{name}");
    }
    Ok(())
}

fn cmd_json() -> Result<()> {
    let data = glamfetch::collect::collect_all_as_json();
    let text = serde_json::to_string_pretty(&data).context("serialising collector data")?;
    println!("{text}");
    Ok(())
}

fn cmd_watch(cli: &Cli) -> Result<()> {
    let interval = cli.watch.unwrap_or(1.0);
    glamfetch::modes::watch::run(cli.config.as_deref(), interval, cli.pipe)
}

fn cmd_edit(cli: &Cli) -> Result<()> {
    glamfetch::modes::edit::run(cli.config.clone(), cli.pipe)
}

fn cmd_print_data() -> Result<()> {
    let data = glamfetch::collect::collect_all_as_json();
    print_value("", &data);
    Ok(())
}

fn print_value(prefix: &str, value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let next = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                print_value(&next, v);
            }
        }
        serde_json::Value::Array(arr) => {
            for (idx, v) in arr.iter().enumerate() {
                print_value(&format!("{prefix}[{idx}]"), v);
            }
        }
        _ => println!("{prefix:32} = {}", display_scalar(value)),
    }
}

fn display_scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "(null)".into(),
        other => other.to_string(),
    }
}

fn load_config(cli: &Cli) -> Result<LoadedConfig> {
    match config::resolve_path(cli.config.as_deref()) {
        Some(path) => {
            info!(path = %path.display(), "loading config");
            Ok(config::load_from_path(&path)?)
        }
        None => {
            info!("no config found, using embedded default");
            Ok(config::load_embedded_default()?)
        }
    }
}
