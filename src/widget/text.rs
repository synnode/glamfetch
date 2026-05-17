//! `text` widget (spec §8).
//!
//! Build-time: resolve theme/icons/env refs in `content` and the optional
//! `color`. Render-time: resolve `${data.*}` refs against the registry,
//! dedent, split into lines.

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::{EvalContext, StaticContext, eval_single, eval_template};
use crate::error::{ConfigError, RenderError};
use crate::style::{Segment, Style, StyledLine, parse_color};

use super::{Cell, Widget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextConfig {
    pub content: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub show_if: Option<String>,
}

pub struct TextWidget {
    content_template: String,
    style: Style,
}

impl TextWidget {
    pub fn build(cfg: TextConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let template = eval_template(&cfg.content, &EvalContext::build_only(ctx))?;

        let fg = match cfg.color.as_deref() {
            Some(raw) => {
                let resolved = eval_single(raw, &EvalContext::build_only(ctx))?;
                parse_color(&resolved).map_err(|err| ConfigError::Invalid(err.to_string()))?
            }
            None => None,
        };

        Ok(Self {
            content_template: template,
            style: Style {
                fg,
                bg: None,
                bold: cfg.bold,
                italic: cfg.italic,
                ..Style::plain()
            },
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
                    StyledLine::from_segments(vec![Segment::styled(line, self.style)])
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
