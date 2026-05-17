//! Output renderers (spec §10 / §13).
//!
//! Two strategies:
//! - [`ansi`] emits styled output for an interactive terminal
//! - [`pipe`] emits plain text suitable for piping / non-TTY consumers
//!
//! [`terminal`] decides which strategy to use based on TTY / NO_COLOR /
//! COLORTERM.

pub mod ansi;
pub mod pipe;
pub mod terminal;
