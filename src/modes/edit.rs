//! `--edit` live preview pane.
//!
//! Single-pane scope (spec §10). Pair with `tmux`/`zellij` for split.
//! Watches the resolved config path with `notify`; on every Write event,
//! re-renders. Parse errors render in place instead of crashing the
//! preview, so a half-typed save stays recoverable.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use anyhow::{Result, anyhow};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{Event, KeyCode, KeyModifiers, poll, read};
use crossterm::execute;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use notify::{
    Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher, event::EventKind,
};

use crate::collect::{CollectorRegistry, all as all_collectors};
use crate::config;
use crate::config::expr::StaticContext;
use crate::config::prepass;
use crate::layout::{Layout, Row};
use crate::render::ansi;
use crate::render::terminal::Capabilities;
use crate::style::{Segment, Style, StyledLine, parse_color};
use crate::theme;

pub fn run(config_path: Option<PathBuf>, force_pipe: bool) -> Result<()> {
    let path = config_path
        .or_else(|| config::resolve_path(None))
        .ok_or_else(|| {
            anyhow!(
                "--edit needs a real config file on disk (got the embedded preset); \
                 run `glamfetch --init` first or pass --config <path>"
            )
        })?;

    let caps = if force_pipe {
        Capabilities::forced_pipe()
    } else {
        Capabilities::detect()
    };

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let result = run_loop(&mut stdout, &path, caps);

    let _ = execute!(stdout, Show, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    result
}

fn run_loop<W: Write>(stdout: &mut W, path: &Path, caps: Capabilities) -> Result<()> {
    let (events_tx, events_rx) = channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res
                && matches!(
                    ev.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                )
            {
                let _ = events_tx.send(());
            }
        },
        NotifyConfig::default(),
    )?;

    // Watch the parent directory so editors that swap-on-save (vim, helix)
    // still trigger events — they delete + rename, not modify the file
    // directly.
    let watch_target = path.parent().unwrap_or(path);
    watcher.watch(watch_target, RecursiveMode::NonRecursive)?;

    render_or_error(stdout, path, caps)?;

    loop {
        if poll(Duration::from_millis(100))?
            && let Event::Key(key) = read()?
        {
            let ctrl_c = key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL;
            if ctrl_c || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                return Ok(());
            }
        }

        // Drain any events that piled up; rerender at most once per batch.
        if drain_events(&events_rx) {
            // Editors often write multiple syscalls per save; wait briefly
            // so we observe the final state.
            std::thread::sleep(Duration::from_millis(50));
            let _ = drain_events(&events_rx);
            render_or_error(stdout, path, caps)?;
        }
    }
}

fn drain_events(rx: &Receiver<()>) -> bool {
    let mut any = false;
    while rx.try_recv().is_ok() {
        any = true;
    }
    any
}

fn render_or_error<W: Write>(stdout: &mut W, path: &Path, caps: Capabilities) -> Result<()> {
    execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;
    match build_frame(path) {
        Ok(frame) => ansi::write_frame(stdout, &frame, caps.color)?,
        Err(err) => {
            let frame = error_frame(&err.to_string());
            ansi::write_frame(stdout, &frame, caps.color)?;
        }
    }
    stdout.flush()?;
    Ok(())
}

fn build_frame(path: &Path) -> Result<Vec<StyledLine>> {
    let loaded = config::load_from_path(path)?;
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
    Ok(layout.render(&registry)?)
}

fn error_frame(message: &str) -> Vec<StyledLine> {
    let red = parse_color("#f38ba8").ok().flatten();
    let muted = parse_color("#6c7086").ok().flatten();
    let mut lines = Vec::new();
    lines.push(StyledLine::from_segments(vec![Segment::styled(
        "── parse error ────────────────────────────────".to_string(),
        Style {
            fg: muted,
            ..Style::plain()
        },
    )]));
    lines.push(StyledLine::empty());
    for line in message.lines() {
        lines.push(StyledLine::from_segments(vec![Segment::styled(
            line.to_string(),
            Style {
                fg: red,
                bold: true,
                ..Style::plain()
            },
        )]));
    }
    lines.push(StyledLine::empty());
    lines.push(StyledLine::from_segments(vec![Segment::styled(
        "(waiting for next save...)".to_string(),
        Style {
            fg: muted,
            ..Style::plain()
        },
    )]));
    lines
}
