//! Core styling types (spec §4.2).
//!
//! `Color` keeps only the variants needed for Phase 2 — `Named` and `Rgb`.
//! `Hex` from the spec is collapsed into `Rgb` at parse time (hex strings
//! are validated once, then carried as plain `(u8, u8, u8)`). `Gradient`
//! lands in Phase 6 with the per-char interpolation renderer.

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
}

impl Style {
    pub const fn plain() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            dim: false,
        }
    }

    pub const fn fg(color: Color) -> Self {
        Self {
            fg: Some(color),
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            dim: false,
        }
    }

    pub const fn is_plain(&self) -> bool {
        self.fg.is_none()
            && self.bg.is_none()
            && !self.bold
            && !self.italic
            && !self.underline
            && !self.dim
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Color {
    Named(NamedColor),
    Rgb(u8, u8, u8),
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb(r, g, b)
    }

    /// Resolve to a concrete `(r, g, b)` triple. Named colors use the
    /// standard ANSI palette (matching xterm defaults).
    pub fn to_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Rgb(r, g, b) => (r, g, b),
            Self::Named(name) => name.to_rgb(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl NamedColor {
    pub fn to_rgb(self) -> (u8, u8, u8) {
        // xterm/VGA default palette.
        match self {
            Self::Black => (0, 0, 0),
            Self::Red => (205, 0, 0),
            Self::Green => (0, 205, 0),
            Self::Yellow => (205, 205, 0),
            Self::Blue => (0, 0, 238),
            Self::Magenta => (205, 0, 205),
            Self::Cyan => (0, 205, 205),
            Self::White => (229, 229, 229),
            Self::BrightBlack => (127, 127, 127),
            Self::BrightRed => (255, 0, 0),
            Self::BrightGreen => (0, 255, 0),
            Self::BrightYellow => (255, 255, 0),
            Self::BrightBlue => (92, 92, 255),
            Self::BrightMagenta => (255, 0, 255),
            Self::BrightCyan => (0, 255, 255),
            Self::BrightWhite => (255, 255, 255),
        }
    }
}

impl FromStr for NamedColor {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "black" => Self::Black,
            "red" => Self::Red,
            "green" => Self::Green,
            "yellow" => Self::Yellow,
            "blue" => Self::Blue,
            "magenta" => Self::Magenta,
            "cyan" => Self::Cyan,
            "white" => Self::White,
            "bright_black" | "gray" | "grey" => Self::BrightBlack,
            "bright_red" => Self::BrightRed,
            "bright_green" => Self::BrightGreen,
            "bright_yellow" => Self::BrightYellow,
            "bright_blue" => Self::BrightBlue,
            "bright_magenta" => Self::BrightMagenta,
            "bright_cyan" => Self::BrightCyan,
            "bright_white" => Self::BrightWhite,
            _ => return Err(()),
        })
    }
}

/// Parse a config color string. Accepts:
/// - `"#rrggbb"` or `"#rgb"` (case-insensitive)
/// - named colors (`"red"`, `"bright_blue"`, `"gray"`, ...)
/// - `"transparent"` / `"none"` → `None`
pub fn parse_color(input: &str) -> Result<Option<Color>, ColorParseError> {
    let s = input.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("transparent") || s.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    if let Some(rest) = s.strip_prefix('#') {
        return parse_hex(rest).map(Some);
    }
    if let Ok(name) = NamedColor::from_str(&s.to_ascii_lowercase()) {
        return Ok(Some(Color::Named(name)));
    }
    Err(ColorParseError::Unknown(s.to_string()))
}

fn parse_hex(hex: &str) -> Result<Color, ColorParseError> {
    let normalized = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => return Err(ColorParseError::BadHex(format!("#{hex}"))),
    };
    let parse = |i: usize| {
        u8::from_str_radix(&normalized[i..i + 2], 16)
            .map_err(|_| ColorParseError::BadHex(format!("#{hex}")))
    };
    Ok(Color::Rgb(parse(0)?, parse(2)?, parse(4)?))
}

#[derive(Debug, thiserror::Error)]
pub enum ColorParseError {
    #[error("unknown color name: `{0}`")]
    Unknown(String),

    #[error("invalid hex color: `{0}` (expected `#rgb` or `#rrggbb`)")]
    BadHex(String),
}

// ---------------------------------------------------------------------------
// PaintSpec — resolved foreground + per-character paint behavior.
//
// A single `Color` paints a whole `Segment` uniformly. A list of two or
// more colors triggers per-character gradient interpolation when the
// widget calls [`PaintSpec::paint_line`]. The gradient itself isn't part
// of `Color` so the rest of the styling stack stays `Copy`-friendly.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct PaintSpec {
    /// 0 entries → no foreground. 1 → solid. 2+ → gradient stops.
    pub colors: Vec<Color>,
    /// Bold/italic/underline/dim. `fg` is ignored here — `colors` is the
    /// source of truth for foreground.
    pub attrs: Style,
}

impl PaintSpec {
    pub fn solid(color: Option<Color>) -> Self {
        Self {
            colors: color.into_iter().collect(),
            attrs: Style::plain(),
        }
    }

    pub fn gradient(stops: Vec<Color>) -> Self {
        Self {
            colors: stops,
            attrs: Style::plain(),
        }
    }

    pub fn with_attrs(mut self, attrs: Style) -> Self {
        self.attrs = Style { fg: None, ..attrs };
        self
    }

    /// Convert a line of text into one or more segments using this paint.
    /// Single-color → one segment; gradient → one segment per character
    /// with linearly interpolated RGB across the stops.
    pub fn paint_line(&self, text: &str) -> Vec<Segment> {
        if text.is_empty() {
            return Vec::new();
        }
        match self.colors.len() {
            0 => vec![Segment::styled(text.to_string(), self.attrs)],
            1 => {
                let mut style = self.attrs;
                style.fg = Some(self.colors[0]);
                vec![Segment::styled(text.to_string(), style)]
            }
            _ => {
                let chars: Vec<char> = text.chars().collect();
                let n = chars.len();
                if n == 1 {
                    let mut style = self.attrs;
                    style.fg = Some(self.colors[0]);
                    return vec![Segment::styled(chars[0].to_string(), style)];
                }
                let mut out = Vec::with_capacity(n);
                for (i, ch) in chars.into_iter().enumerate() {
                    let t = i as f32 / (n - 1) as f32;
                    let color = sample_gradient(&self.colors, t);
                    let mut style = self.attrs;
                    style.fg = Some(color);
                    out.push(Segment::styled(ch.to_string(), style));
                }
                out
            }
        }
    }
}

fn sample_gradient(stops: &[Color], t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let last = stops.len() - 1;
    let scaled = t * last as f32;
    let lo = scaled.floor() as usize;
    let hi = (lo + 1).min(last);
    let frac = scaled - lo as f32;
    let (r0, g0, b0) = stops[lo].to_rgb();
    let (r1, g1, b1) = stops[hi].to_rgb();
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * frac).round() as u8;
    Color::Rgb(lerp(r0, r1), lerp(g0, g1), lerp(b0, b1))
}

// ---------------------------------------------------------------------------
// Styled segments / lines
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Segment {
    pub text: String,
    pub style: Style,
}

impl Segment {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::plain(),
        }
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    pub fn width(&self) -> usize {
        UnicodeWidthStr::width(self.text.as_str())
    }
}

#[derive(Debug, Clone, Default)]
pub struct StyledLine {
    pub segments: Vec<Segment>,
    pub width: usize,
}

impl StyledLine {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn plain(text: impl Into<String>) -> Self {
        let seg = Segment::plain(text);
        let width = seg.width();
        Self {
            segments: vec![seg],
            width,
        }
    }

    pub fn from_segments(segments: Vec<Segment>) -> Self {
        let width = segments.iter().map(Segment::width).sum();
        Self { segments, width }
    }

    /// Append unstyled trailing spaces so the line fills `target_width`.
    /// No-op if the line is already wide enough.
    pub fn pad_to(&mut self, target_width: usize) {
        if self.width >= target_width {
            return;
        }
        let pad = target_width - self.width;
        self.segments.push(Segment::plain(" ".repeat(pad)));
        self.width = target_width;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_long_and_short() {
        assert_eq!(
            parse_color("#ff8800").unwrap(),
            Some(Color::Rgb(0xff, 0x88, 0x00))
        );
        assert_eq!(
            parse_color("#f80").unwrap(),
            Some(Color::Rgb(0xff, 0x88, 0x00))
        );
    }

    #[test]
    fn parses_named_with_aliases() {
        assert_eq!(
            parse_color("red").unwrap(),
            Some(Color::Named(NamedColor::Red))
        );
        assert_eq!(
            parse_color("GRAY").unwrap(),
            Some(Color::Named(NamedColor::BrightBlack))
        );
        assert_eq!(
            parse_color("bright_blue").unwrap(),
            Some(Color::Named(NamedColor::BrightBlue))
        );
    }

    #[test]
    fn parses_transparent() {
        assert_eq!(parse_color("transparent").unwrap(), None);
        assert_eq!(parse_color("none").unwrap(), None);
        assert_eq!(parse_color("").unwrap(), None);
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(matches!(
            parse_color("#xyz"),
            Err(ColorParseError::BadHex(_))
        ));
        assert!(matches!(
            parse_color("#1234"),
            Err(ColorParseError::BadHex(_))
        ));
    }

    #[test]
    fn rejects_unknown_name() {
        assert!(matches!(
            parse_color("plaidpink"),
            Err(ColorParseError::Unknown(_))
        ));
    }

    #[test]
    fn pad_to_extends() {
        let mut line = StyledLine::plain("hi");
        line.pad_to(5);
        assert_eq!(line.width, 5);
        assert_eq!(line.segments.last().unwrap().text, "   ");
    }
}
