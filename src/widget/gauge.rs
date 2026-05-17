//! `gauge` widget (spec §8).
//!
//! Layout: `LABEL  NN%  [█████░░░░░]`. Bar fill ratio comes from
//! `value / max`. The bar uses two characters (filled + empty) and styles
//! each half independently; gradients across the bar arrive in Phase 6.

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::color_spec::{ColorSpec, resolve_optional};
use crate::config::expr::{EvalContext, StaticContext, eval_template};
use crate::error::{ConfigError, RenderError};
use crate::style::{PaintSpec, Segment, Style, StyledLine};

use super::{Cell, Widget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GaugeConfig {
    pub label: String,
    pub value: String,
    #[serde(default = "default_max")]
    pub max: f32,
    #[serde(default = "default_width")]
    pub width: usize,
    #[serde(default)]
    pub color: Option<ColorSpec>,
    #[serde(default)]
    pub empty_color: Option<ColorSpec>,
    #[serde(default = "default_filled_char")]
    pub filled_char: String,
    #[serde(default = "default_empty_char")]
    pub empty_char: String,
    #[serde(default = "default_show_percent")]
    pub show_percent: bool,
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
fn default_show_percent() -> bool {
    true
}

pub struct GaugeWidget {
    label: String,
    value_template: String,
    max: f32,
    width: usize,
    filled_char: String,
    empty_char: String,
    show_percent: bool,
    filled_paint: PaintSpec,
    empty_paint: PaintSpec,
}

impl GaugeWidget {
    pub fn build(cfg: GaugeConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let value_template = eval_template(&cfg.value, &EvalContext::build_only(ctx))?;
        Ok(Self {
            label: cfg.label,
            value_template,
            max: cfg.max.max(1.0),
            width: cfg.width.max(1),
            filled_char: cfg.filled_char,
            empty_char: cfg.empty_char,
            show_percent: cfg.show_percent,
            filled_paint: resolve_optional(cfg.color.as_ref(), Style::plain(), ctx)?,
            empty_paint: resolve_optional(cfg.empty_color.as_ref(), Style::plain(), ctx)?,
        })
    }
}

impl Widget for GaugeWidget {
    fn render(
        &self,
        registry: &CollectorRegistry,
        _max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let static_ctx = StaticContext::default();
        let ctx = EvalContext::full(&static_ctx, registry);

        let value_text =
            eval_template(&self.value_template, &ctx).map_err(|err| RenderError::Widget {
                widget: "gauge",
                message: err.to_string(),
            })?;
        let value: f32 = value_text.trim().parse().unwrap_or(0.0);
        let ratio = (value / self.max).clamp(0.0, 1.0);
        let filled_count = (ratio * self.width as f32).round() as usize;
        let empty_count = self.width.saturating_sub(filled_count);

        let mut segments: Vec<Segment> = Vec::new();
        // Label + padding gap (single space).
        segments.push(Segment::plain(format!("{} ", self.label)));

        if self.show_percent {
            let pct = (ratio * 100.0).round() as u32;
            segments.push(Segment::plain(format!("{pct:>3}% ")));
        }

        if filled_count > 0 {
            let bar = self.filled_char.repeat(filled_count);
            for seg in self.filled_paint.paint_line(&bar) {
                segments.push(seg);
            }
        }
        if empty_count > 0 {
            let bar = self.empty_char.repeat(empty_count);
            for seg in self.empty_paint.paint_line(&bar) {
                segments.push(seg);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn renders_fill_proportional_to_value() {
        let ctx = StaticContext::default();
        let cfg = GaugeConfig {
            label: "CPU".into(),
            value: "${data.cpu.usage}".into(),
            max: 100.0,
            width: 10,
            color: None,
            empty_color: None,
            filled_char: "#".into(),
            empty_char: "-".into(),
            show_percent: false,
            show_if: None,
        };
        let widget = GaugeWidget::build(cfg, &ctx).unwrap();

        let mut reg = CollectorRegistry::new();
        reg.insert("cpu", json!({ "usage": 30 }));

        let cell = widget.render(&reg, None).unwrap();
        let rendered: String = cell.lines[0]
            .segments
            .iter()
            .map(|s| s.text.clone())
            .collect();
        // 30% of 10 = 3 filled chars
        assert!(rendered.ends_with("###-------"));
        assert!(rendered.starts_with("CPU "));
    }

    #[test]
    fn label_width_is_measured_correctly() {
        let ctx = StaticContext::default();
        let cfg = GaugeConfig {
            label: "X".into(),
            value: "50".into(),
            max: 100.0,
            width: 4,
            color: None,
            empty_color: None,
            filled_char: "#".into(),
            empty_char: "-".into(),
            show_percent: true,
            show_if: None,
        };
        let widget = GaugeWidget::build(cfg, &ctx).unwrap();
        let cell = widget.render(&CollectorRegistry::new(), None).unwrap();
        // "X " (2) + " 50% " (5) + "##--" (4) = 11
        assert_eq!(cell.width, UnicodeWidthStr::width("X  50% ##--"));
    }
}
