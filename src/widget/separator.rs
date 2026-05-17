//! `separator` widget (spec §8) — horizontal divider line.
//!
//! `length = N` produces exactly N characters. `length = "auto"` asks the
//! caller for a width (spec §9.1): when the parent passes `Some(w)` we use
//! `w`; when the parent passes `None` we fall back to `default_length`.

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::{EvalContext, StaticContext, eval_single, eval_template};
use crate::error::{ConfigError, RenderError};
use crate::style::{Segment, Style, StyledLine, parse_color};

use super::{Cell, Widget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeparatorConfig {
    #[serde(default = "default_char")]
    pub char: String,
    /// `usize` or the literal string `"auto"`.
    #[serde(default = "default_length")]
    pub length: toml::Value,
    #[serde(default = "default_default_length")]
    pub default_length: usize,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub show_if: Option<String>,
}

fn default_char() -> String {
    "─".into()
}
fn default_length() -> toml::Value {
    toml::Value::Integer(20)
}
fn default_default_length() -> usize {
    20
}

#[derive(Debug, Clone, Copy)]
enum Length {
    Fixed(usize),
    Auto,
}

pub struct SeparatorWidget {
    glyph: String,
    length: Length,
    default_length: usize,
    style: Style,
}

impl SeparatorWidget {
    pub fn build(cfg: SeparatorConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let length = parse_length(&cfg.length)?;
        let glyph = eval_template(&cfg.char, &EvalContext::build_only(ctx))?;
        let fg = match cfg.color.as_deref() {
            Some(raw) => {
                let resolved = eval_single(raw, &EvalContext::build_only(ctx))?;
                parse_color(&resolved).map_err(|err| ConfigError::Invalid(err.to_string()))?
            }
            None => None,
        };
        Ok(Self {
            glyph,
            length,
            default_length: cfg.default_length.max(1),
            style: Style {
                fg,
                ..Style::plain()
            },
        })
    }
}

fn parse_length(value: &toml::Value) -> Result<Length, ConfigError> {
    match value {
        toml::Value::Integer(n) if *n >= 0 => Ok(Length::Fixed(*n as usize)),
        toml::Value::String(s) if s == "auto" => Ok(Length::Auto),
        other => Err(ConfigError::Invalid(format!(
            "separator length must be a non-negative integer or \"auto\", got {other:?}"
        ))),
    }
}

impl Widget for SeparatorWidget {
    fn render(
        &self,
        _registry: &CollectorRegistry,
        max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let count = match self.length {
            Length::Fixed(n) => n,
            Length::Auto => max_width.unwrap_or(self.default_length),
        };
        if count == 0 {
            return Ok(Cell::empty());
        }
        let text = self.glyph.repeat(count);
        let line = StyledLine::from_segments(vec![Segment::styled(text, self.style)]);
        let width = line.width;
        Ok(Cell {
            lines: vec![line],
            width,
            height: 1,
        })
    }
}
