//! Terminal capability detection (spec §13).
//!
//! Best-effort only. We never query the terminal directly; everything is
//! decided from env vars + TTY checks. This keeps the binary side-effect
//! free and predictable.

use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// 24-bit RGB output (`COLORTERM=truecolor`).
    Truecolor,
    /// 256-color palette fallback.
    Palette256,
    /// No colors at all (`NO_COLOR`, non-TTY, or explicit `--pipe`).
    None,
}

#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub color: ColorMode,
    pub is_tty: bool,
}

impl Capabilities {
    /// Detect from process env + stdout TTY status.
    pub fn detect() -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let color = detect_color_mode(is_tty);
        Self { color, is_tty }
    }

    /// Force colorless output (used by `--pipe`).
    pub fn forced_pipe() -> Self {
        Self {
            color: ColorMode::None,
            is_tty: false,
        }
    }
}

fn detect_color_mode(is_tty: bool) -> ColorMode {
    if std::env::var_os("NO_COLOR").is_some() {
        return ColorMode::None;
    }
    if !is_tty {
        return ColorMode::None;
    }
    match std::env::var("COLORTERM").ok().as_deref() {
        Some("truecolor") | Some("24bit") => ColorMode::Truecolor,
        _ => ColorMode::Palette256,
    }
}
