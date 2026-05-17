//! Long-running modes that wrap the one-shot render path.
//!
//! `watch` polls on an interval; `edit` re-renders whenever the config
//! file changes on disk. Both share the same render pipeline as
//! `Command::Once` — they just drive it in a loop and add a few terminal
//! niceties (alt-screen, cursor restore, Ctrl+C cleanup).

pub mod edit;
pub mod watch;
