//! `--watch [INTERVAL]` loop.
//!
//! Clears the screen and re-renders every `interval` seconds. Reads key
//! events with a short timeout so Ctrl+C / 'q' exits within ~interval
//! seconds. Uses the terminal alt-screen so the watch session leaves the
//! user's prior scrollback intact on exit.

use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{Event, KeyCode, KeyModifiers, poll, read};
use crossterm::execute;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::collect::{CollectorRegistry, all as all_collectors};
use crate::config;
use crate::config::expr::StaticContext;
use crate::config::prepass;
use crate::layout::{Layout, Row};
use crate::render::ansi;
use crate::render::terminal::Capabilities;
use crate::theme;

pub fn run(config_path: Option<&Path>, interval_secs: f32, force_pipe: bool) -> Result<()> {
    let interval = Duration::from_secs_f32(interval_secs.max(0.1));

    let caps = if force_pipe {
        Capabilities::forced_pipe()
    } else {
        Capabilities::detect()
    };

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let result = run_loop(&mut stdout, config_path, interval, caps);

    // Always restore terminal, even on error.
    let _ = execute!(stdout, Show, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    result
}

fn run_loop<W: Write>(
    stdout: &mut W,
    config_path: Option<&Path>,
    interval: Duration,
    caps: Capabilities,
) -> Result<()> {
    loop {
        let frame_start = Instant::now();
        render_once(stdout, config_path, caps)?;

        // Poll for input until interval elapses, so Ctrl+C / q reacts
        // quickly without busy-spinning.
        loop {
            let elapsed = frame_start.elapsed();
            if elapsed >= interval {
                break;
            }
            let remaining = interval - elapsed;
            let tick = remaining.min(Duration::from_millis(100));
            if poll(tick)? {
                if let Event::Key(key) = read()? {
                    let ctrl_c =
                        key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL;
                    if ctrl_c || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn render_once<W: Write>(
    stdout: &mut W,
    config_path: Option<&Path>,
    caps: Capabilities,
) -> Result<()> {
    execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;

    let loaded = match config_path {
        Some(path) => config::load_from_path(path)?,
        None => config::load_embedded_default()?,
    };

    let resolved_theme = theme::resolve(&loaded.config.theme)?;
    let ctx = StaticContext {
        theme: resolved_theme,
        icons: loaded.config.icons.overrides.clone(),
        env_allowed: true,
    };

    let rows: Vec<Row> = loaded
        .config
        .rows
        .into_iter()
        .map(|r| Row::build(r, &ctx))
        .collect::<Result<_, _>>()?;

    let referenced = prepass::referenced_data_roots(&loaded.text);
    let collectors = all_collectors();
    let mut registry = CollectorRegistry::new();
    registry.prime(&collectors, Some(&referenced));

    let layout = Layout::new(rows, loaded.config.layout.gap);
    let frame = layout.render(&registry)?;

    ansi::write_frame(stdout, &frame, caps.color)?;
    stdout.flush()?;
    Ok(())
}
