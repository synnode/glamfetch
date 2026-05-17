//! Pipe / colorless renderer.
//!
//! Thin wrapper over [`crate::render::ansi::write_frame`] with
//! [`ColorMode::None`]. Lives in its own module so callers can be explicit
//! about intent (`render::pipe::write_frame(...)`) rather than passing a
//! mode argument.

use std::io::Write;

use crate::error::RenderError;
use crate::style::StyledLine;

use super::{ansi, terminal::ColorMode};

pub fn write_frame<W: Write>(writer: &mut W, frame: &[StyledLine]) -> Result<(), RenderError> {
    ansi::write_frame(writer, frame, ColorMode::None)
}
