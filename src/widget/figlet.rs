//! `figlet` widget (spec §8) — render text as FIGlet ASCII art.
//!
//! Uses `figlet-rs`'s built-in font set. Unknown font names fall back to
//! `standard` with a debug-level warning. The text template resolves at
//! render time, so figlet output is regenerated per render — for
//! `${data.*}` references this means a fresh figure each `--watch` tick.

use figlet_rs::FIGlet;
use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::{EvalContext, StaticContext, eval_single, eval_template};
use crate::error::{ConfigError, RenderError};
use crate::style::{Segment, Style, StyledLine, parse_color};

use super::{Cell, Widget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FigletConfig {
    pub text: String,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub show_if: Option<String>,
}

fn default_font() -> String {
    "standard".into()
}

pub struct FigletWidget {
    text_template: String,
    font: FIGlet,
    style: Style,
}

impl FigletWidget {
    pub fn build(cfg: FigletConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let text_template = eval_template(&cfg.text, &EvalContext::build_only(ctx))?;
        let font = load_font(&cfg.font);
        let fg = match cfg.color.as_deref() {
            Some(raw) => {
                let resolved = eval_single(raw, &EvalContext::build_only(ctx))?;
                parse_color(&resolved).map_err(|err| ConfigError::Invalid(err.to_string()))?
            }
            None => None,
        };
        Ok(Self {
            text_template,
            font,
            style: Style {
                fg,
                ..Style::plain()
            },
        })
    }
}

fn load_font(name: &str) -> FIGlet {
    let result = match name {
        "standard" => FIGlet::standard(),
        "slant" => FIGlet::slant(),
        "small" => FIGlet::small(),
        "big" => FIGlet::big(),
        other => {
            tracing::warn!(
                "figlet: unknown font `{other}`, falling back to `standard` (valid: standard, slant, small, big)"
            );
            FIGlet::standard()
        }
    };
    result.expect("built-in figlet font failed to load")
}

impl Widget for FigletWidget {
    fn render(
        &self,
        registry: &CollectorRegistry,
        _max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let static_ctx = StaticContext::default();
        let ctx = EvalContext::full(&static_ctx, registry);
        let text = eval_template(&self.text_template, &ctx).map_err(|err| RenderError::Widget {
            widget: "figlet",
            message: err.to_string(),
        })?;

        let Some(figure) = self.font.convert(&text) else {
            return Ok(Cell::empty());
        };

        // FIGure::as_str() returns lines joined with `\n`. Trim trailing
        // blank lines that the font emits after each character row.
        let raw = figure.to_string();
        let trimmed = raw.trim_end_matches('\n');

        let lines: Vec<StyledLine> = trimmed
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    StyledLine::empty()
                } else {
                    StyledLine::from_segments(vec![Segment::styled(line.to_string(), self.style)])
                }
            })
            .collect();

        Ok(Cell::from_lines(lines))
    }
}
