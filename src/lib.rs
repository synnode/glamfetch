//! Glamfetch library surface.
//!
//! The binary at `src/main.rs` is a thin shell over this crate; everything
//! interesting lives here so integration tests and (eventually) external
//! tooling can call the same code path.

#![deny(unsafe_code)]

pub mod collect;
pub mod config;
pub mod error;
pub mod layout;
pub mod render;
pub mod style;
pub mod theme;
pub mod widget;
