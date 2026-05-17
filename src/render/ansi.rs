//! ANSI/SGR output for [`StyledLine`]s.
//!
//! Two color modes: truecolor (`\x1b[38;2;r;g;bm`) and 256-color
//! (`\x1b[38;5;Nm`). The 256-color fallback quantises an `(r, g, b)` to
//! the xterm 6×6×6 color cube + 24-step grayscale ramp.

use std::io::{self, Write};

use crate::error::RenderError;
use crate::style::{Color, Segment, StyledLine};

use super::terminal::ColorMode;

const RESET: &str = "\x1b[0m";

/// Write a frame (slice of styled lines) plus trailing newline per line.
pub fn write_frame<W: Write>(
    writer: &mut W,
    frame: &[StyledLine],
    mode: ColorMode,
) -> Result<(), RenderError> {
    for line in frame {
        write_line(writer, line, mode)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

fn write_line<W: Write>(
    writer: &mut W,
    line: &StyledLine,
    mode: ColorMode,
) -> Result<(), io::Error> {
    for segment in &line.segments {
        write_segment(writer, segment, mode)?;
    }
    Ok(())
}

fn write_segment<W: Write>(
    writer: &mut W,
    segment: &Segment,
    mode: ColorMode,
) -> Result<(), io::Error> {
    if mode == ColorMode::None || segment.style.is_plain() {
        return writer.write_all(segment.text.as_bytes());
    }

    let mut codes: Vec<String> = Vec::new();
    if segment.style.bold {
        codes.push("1".into());
    }
    if segment.style.dim {
        codes.push("2".into());
    }
    if segment.style.italic {
        codes.push("3".into());
    }
    if segment.style.underline {
        codes.push("4".into());
    }
    if let Some(fg) = segment.style.fg {
        codes.push(color_code(fg, true, mode));
    }
    if let Some(bg) = segment.style.bg {
        codes.push(color_code(bg, false, mode));
    }

    if codes.is_empty() {
        return writer.write_all(segment.text.as_bytes());
    }

    write!(writer, "\x1b[{}m", codes.join(";"))?;
    writer.write_all(segment.text.as_bytes())?;
    writer.write_all(RESET.as_bytes())?;
    Ok(())
}

fn color_code(color: Color, foreground: bool, mode: ColorMode) -> String {
    let (r, g, b) = color.to_rgb();
    let prefix = if foreground { 38 } else { 48 };
    match mode {
        ColorMode::Truecolor => format!("{prefix};2;{r};{g};{b}"),
        ColorMode::Palette256 => format!("{prefix};5;{}", quantize_256(r, g, b)),
        ColorMode::None => unreachable!("write_segment short-circuits None"),
    }
}

// ---------------------------------------------------------------------------
// 256-color quantizer (xterm cube + grayscale ramp)
// ---------------------------------------------------------------------------

/// Convert an `(r, g, b)` triple to the nearest xterm 256-color index.
pub fn quantize_256(r: u8, g: u8, b: u8) -> u8 {
    let cube = quantize_cube(r, g, b);
    let gray = quantize_gray(r, g, b);

    let cube_rgb = cube_to_rgb(cube);
    let gray_rgb = gray_to_rgb(gray);

    let cube_dist = squared_dist((r, g, b), cube_rgb);
    let gray_dist = squared_dist((r, g, b), gray_rgb);

    if gray_dist < cube_dist { gray } else { cube }
}

/// Pick the 6×6×6 cube index (16-231) closest to the input.
fn quantize_cube(r: u8, g: u8, b: u8) -> u8 {
    let rc = nearest_cube_step(r);
    let gc = nearest_cube_step(g);
    let bc = nearest_cube_step(b);
    16 + 36 * rc + 6 * gc + bc
}

/// xterm cube uses steps: 0, 95, 135, 175, 215, 255.
fn nearest_cube_step(v: u8) -> u8 {
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let mut best = 0u8;
    let mut best_diff = i32::MAX;
    for (idx, step) in STEPS.iter().enumerate() {
        let diff = (i32::from(v) - i32::from(*step)).abs();
        if diff < best_diff {
            best_diff = diff;
            best = idx as u8;
        }
    }
    best
}

fn cube_to_rgb(index: u8) -> (u8, u8, u8) {
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let i = index - 16;
    let r = STEPS[(i / 36) as usize];
    let g = STEPS[((i % 36) / 6) as usize];
    let b = STEPS[(i % 6) as usize];
    (r, g, b)
}

/// Grayscale ramp 232-255 (24 levels, evenly spaced from 8 to 238).
fn quantize_gray(r: u8, g: u8, b: u8) -> u8 {
    let avg = (i32::from(r) + i32::from(g) + i32::from(b)) / 3;
    let mut best = 232u8;
    let mut best_diff = i32::MAX;
    for level in 0..24u8 {
        let value = 8 + 10 * i32::from(level);
        let diff = (avg - value).abs();
        if diff < best_diff {
            best_diff = diff;
            best = 232 + level;
        }
    }
    best
}

fn gray_to_rgb(index: u8) -> (u8, u8, u8) {
    let level = index - 232;
    let value = 8 + 10 * level;
    (value, value, value)
}

fn squared_dist(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
    let dr = i32::from(a.0) - i32::from(b.0);
    let dg = i32::from(a.1) - i32::from(b.1);
    let db = i32::from(a.2) - i32::from(b.2);
    dr * dr + dg * dg + db * db
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, Style};

    #[test]
    fn quantize_pure_red_lands_in_cube() {
        // Pure red (255, 0, 0) is exactly cube step (5, 0, 0) → 16 + 5*36 = 196.
        assert_eq!(quantize_256(255, 0, 0), 196);
    }

    #[test]
    fn quantize_pure_white_picks_brightest() {
        // (255, 255, 255) → cube (5,5,5) = 231, dist 0. Grayscale max is 238 → distance > 0.
        assert_eq!(quantize_256(255, 255, 255), 231);
    }

    #[test]
    fn quantize_neutral_gray_prefers_ramp() {
        // (128, 128, 128): grayscale ramp has closer match than any cube cell.
        let idx = quantize_256(128, 128, 128);
        assert!((232..=255).contains(&idx));
    }

    #[test]
    fn truecolor_emits_38_2_codes() {
        let mut buf = Vec::new();
        let line = StyledLine::from_segments(vec![Segment::styled(
            "x",
            Style::fg(Color::Rgb(10, 20, 30)),
        )]);
        write_frame(&mut buf, &[line], ColorMode::Truecolor).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\x1b[38;2;10;20;30m"));
        assert!(s.contains(RESET));
    }

    #[test]
    fn pipe_mode_strips_codes() {
        let mut buf = Vec::new();
        let line =
            StyledLine::from_segments(vec![Segment::styled("x", Style::fg(Color::Rgb(255, 0, 0)))]);
        write_frame(&mut buf, &[line], ColorMode::None).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, "x\n");
    }
}
