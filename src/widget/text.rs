//! `text` widget (spec §8).
//!
//! Build-time: resolve theme/icons/env refs in `content` and the optional
//! `color`. Render-time: resolve `${data.*}` refs against the registry,
//! dedent, split into lines, and paint each line via the cached
//! [`PaintSpec`] (which handles gradients per-character).

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::color_spec::{ColorSpec, resolve_optional};
use crate::config::expr::{EvalContext, StaticContext, eval_template};
use crate::error::{ConfigError, RenderError};
use crate::style::{PaintSpec, Style, StyledLine};

use super::{Cell, Widget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextConfig {
    pub content: String,
    #[serde(default)]
    pub color: Option<ColorSpec>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub show_if: Option<String>,
}

pub struct TextWidget {
    content_template: String,
    paint: PaintSpec,
}

impl TextWidget {
    pub fn build(cfg: TextConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let template = eval_template(&cfg.content, &EvalContext::build_only(ctx))?;
        let attrs = Style {
            bold: cfg.bold,
            italic: cfg.italic,
            ..Style::plain()
        };
        let paint = resolve_optional(cfg.color.as_ref(), attrs, ctx)?;
        Ok(Self {
            content_template: template,
            paint,
        })
    }
}

impl Widget for TextWidget {
    fn render(
        &self,
        registry: &CollectorRegistry,
        _max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let static_ctx = StaticContext::default();
        let ctx = EvalContext::full(&static_ctx, registry);
        let resolved =
            eval_template(&self.content_template, &ctx).map_err(|err| RenderError::Widget {
                widget: "text",
                message: err.to_string(),
            })?;

        let dedented = dedent(&resolved);
        let lines: Vec<StyledLine> = dedented
            .lines()
            .map(|line| {
                if line.is_empty() {
                    StyledLine::empty()
                } else {
                    StyledLine::from_segments(self.paint.paint_line(line))
                }
            })
            .collect();

        Ok(Cell::from_lines(lines))
    }
}

/// Strip the smallest common leading-whitespace prefix from each non-blank
/// line. Blanks-only lines collapse to empty so they don't carry leftover
/// indentation, and trailing blanks are dropped.
fn dedent(input: &str) -> String {
    let input = input.trim_start_matches('\n').trim_end_matches('\n');

    let common = input
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(leading_ws_len)
        .min()
        .unwrap_or(0);

    let mut lines: Vec<&str> = input
        .lines()
        .map(|l| {
            if l.trim().is_empty() {
                ""
            } else if l.len() >= common {
                &l[common..]
            } else {
                l
            }
        })
        .collect();

    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

fn leading_ws_len(s: &str) -> usize {
    s.bytes().take_while(|b| *b == b' ' || *b == b'\t').count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dedent_keeps_relative_indent() {
        let s = "
            line one
              line two
            line three
        ";
        assert_eq!(dedent(s), "line one\n  line two\nline three");
    }

    #[test]
    fn renders_with_data_refs() {
        let ctx = StaticContext::default();
        let cfg = TextConfig {
            content: "host=${data.system.hostname}".into(),
            color: None,
            bold: false,
            italic: false,
            show_if: None,
        };
        let widget = TextWidget::build(cfg, &ctx).unwrap();

        let mut reg = CollectorRegistry::new();
        reg.insert("system", json!({ "hostname": "foo" }));

        let cell = widget.render(&reg, None).unwrap();
        assert_eq!(cell.lines.len(), 1);
        assert_eq!(cell.lines[0].segments[0].text, "host=foo");
    }
}
