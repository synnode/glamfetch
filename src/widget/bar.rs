//! `bar` widget (spec §8) — standalone progress bar without label/percent.

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::{EvalContext, StaticContext, eval_single, eval_template};
use crate::error::{ConfigError, RenderError};
use crate::style::{Segment, Style, StyledLine, parse_color};

use super::{Cell, Widget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarConfig {
    pub value: String,
    #[serde(default = "default_max")]
    pub max: f32,
    #[serde(default = "default_width")]
    pub width: usize,
    #[serde(default = "default_filled_char")]
    pub filled_char: String,
    #[serde(default = "default_empty_char")]
    pub empty_char: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub empty_color: Option<String>,
    #[serde(default)]
    pub show_if: Option<String>,
}

fn default_max() -> f32 {
    100.0
}
fn default_width() -> usize {
    20
}
fn default_filled_char() -> String {
    "█".into()
}
fn default_empty_char() -> String {
    "░".into()
}

pub struct BarWidget {
    value_template: String,
    max: f32,
    width: usize,
    filled_char: String,
    empty_char: String,
    filled_style: Style,
    empty_style: Style,
}

impl BarWidget {
    pub fn build(cfg: BarConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let value_template = eval_template(&cfg.value, &EvalContext::build_only(ctx))?;
        Ok(Self {
            value_template,
            max: cfg.max.max(1.0),
            width: cfg.width.max(1),
            filled_char: cfg.filled_char,
            empty_char: cfg.empty_char,
            filled_style: make_style(cfg.color.as_deref(), ctx)?,
            empty_style: make_style(cfg.empty_color.as_deref(), ctx)?,
        })
    }
}

fn make_style(color: Option<&str>, ctx: &StaticContext) -> Result<Style, ConfigError> {
    let Some(raw) = color else {
        return Ok(Style::plain());
    };
    let resolved = eval_single(raw, &EvalContext::build_only(ctx))?;
    let fg = parse_color(&resolved).map_err(|err| ConfigError::Invalid(err.to_string()))?;
    Ok(Style {
        fg,
        ..Style::plain()
    })
}

impl Widget for BarWidget {
    fn render(
        &self,
        registry: &CollectorRegistry,
        _max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let static_ctx = StaticContext::default();
        let ctx = EvalContext::full(&static_ctx, registry);
        let value_text =
            eval_template(&self.value_template, &ctx).map_err(|err| RenderError::Widget {
                widget: "bar",
                message: err.to_string(),
            })?;
        let value: f32 = value_text.trim().parse().unwrap_or(0.0);
        let ratio = (value / self.max).clamp(0.0, 1.0);
        let filled_count = (ratio * self.width as f32).round() as usize;
        let empty_count = self.width.saturating_sub(filled_count);

        let mut segments = Vec::with_capacity(2);
        if filled_count > 0 {
            segments.push(Segment::styled(
                self.filled_char.repeat(filled_count),
                self.filled_style,
            ));
        }
        if empty_count > 0 {
            segments.push(Segment::styled(
                self.empty_char.repeat(empty_count),
                self.empty_style,
            ));
        }
        let line = StyledLine::from_segments(segments);
        let width = line.width;
        Ok(Cell {
            lines: vec![line],
            width,
            height: 1,
        })
    }
}
