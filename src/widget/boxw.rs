//! `box` widget (spec §8).
//!
//! Phase 3 ships rounded borders only. Other styles (`sharp`, `double`,
//! `thick`, `ascii`, `none`) land in Phase 5. The module is named `boxw`
//! because `box` is a Rust reserved word.

use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthStr;

use crate::collect::CollectorRegistry;
use crate::config::expr::{EvalContext, StaticContext, eval_single, eval_template};
use crate::error::{ConfigError, RenderError};
use crate::style::{Segment, Style, StyledLine, parse_color};

use super::{Cell, Widget, WidgetConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoxConfig {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub title_color: Option<String>,
    #[serde(default)]
    pub border_color: Option<String>,
    /// `rounded` is the only style supported in v0.1.0.
    #[serde(default = "default_border")]
    pub border: String,
    /// `[vertical, horizontal]` or `[top, right, bottom, left]`.
    #[serde(default)]
    pub padding: Vec<usize>,
    pub child: Box<WidgetConfig>,
    #[serde(default)]
    pub show_if: Option<String>,
}

fn default_border() -> String {
    "rounded".into()
}

struct Padding {
    top: usize,
    right: usize,
    bottom: usize,
    left: usize,
}

fn parse_padding(values: &[usize]) -> Padding {
    match values.len() {
        0 => Padding {
            top: 0,
            right: 0,
            bottom: 0,
            left: 0,
        },
        1 => Padding {
            top: values[0],
            right: values[0],
            bottom: values[0],
            left: values[0],
        },
        2 => Padding {
            top: values[0],
            right: values[1],
            bottom: values[0],
            left: values[1],
        },
        _ => Padding {
            top: values[0],
            right: values[1],
            bottom: values[2],
            left: values[3],
        },
    }
}

struct BorderChars {
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    horizontal: char,
    vertical: char,
}

const ROUNDED: BorderChars = BorderChars {
    top_left: '╭',
    top_right: '╮',
    bottom_left: '╰',
    bottom_right: '╯',
    horizontal: '─',
    vertical: '│',
};

const SHARP: BorderChars = BorderChars {
    top_left: '┌',
    top_right: '┐',
    bottom_left: '└',
    bottom_right: '┘',
    horizontal: '─',
    vertical: '│',
};

const DOUBLE: BorderChars = BorderChars {
    top_left: '╔',
    top_right: '╗',
    bottom_left: '╚',
    bottom_right: '╝',
    horizontal: '═',
    vertical: '║',
};

const THICK: BorderChars = BorderChars {
    top_left: '┏',
    top_right: '┓',
    bottom_left: '┗',
    bottom_right: '┛',
    horizontal: '━',
    vertical: '┃',
};

const ASCII_BORDER: BorderChars = BorderChars {
    top_left: '+',
    top_right: '+',
    bottom_left: '+',
    bottom_right: '+',
    horizontal: '-',
    vertical: '|',
};

fn pick_border(name: &str) -> Result<BorderChars, ConfigError> {
    Ok(match name {
        "rounded" => ROUNDED,
        "sharp" => SHARP,
        "double" => DOUBLE,
        "thick" => THICK,
        "ascii" => ASCII_BORDER,
        // `none` is handled separately — caller falls back to a zero-width
        // border path that just renders the child.
        other => {
            return Err(ConfigError::Invalid(format!(
                "unknown box border style `{other}` (valid: rounded, sharp, double, thick, ascii, none)"
            )));
        }
    })
}

pub struct BoxWidget {
    title: Option<String>,
    title_style: Style,
    border_style: Style,
    chars: BorderChars,
    padding: Padding,
    child: Box<dyn Widget>,
}

impl BoxWidget {
    pub fn build(cfg: BoxConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let chars = pick_border(&cfg.border)?;

        let title = match cfg.title.as_deref() {
            Some(raw) => Some(eval_template(raw, &EvalContext::build_only(ctx))?),
            None => None,
        };

        let title_style = make_style(cfg.title_color.as_deref(), ctx)?;
        let border_style = make_style(cfg.border_color.as_deref(), ctx)?;
        let padding = parse_padding(&cfg.padding);
        let child = cfg.child.build(ctx)?;

        Ok(Self {
            title,
            title_style,
            border_style,
            chars,
            padding,
            child,
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

impl Widget for BoxWidget {
    fn render(
        &self,
        registry: &CollectorRegistry,
        _max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let child_cell = self.child.render(registry, None)?;

        let inner_width = child_cell.width + self.padding.left + self.padding.right;
        let title_width = self
            .title
            .as_deref()
            .map(|t| UnicodeWidthStr::width(t) + 2) // padding spaces around title
            .unwrap_or(0);
        let body_width = inner_width.max(title_width);

        let outer_width = body_width + 2; // border verticals
        let mut lines: Vec<StyledLine> = Vec::new();

        lines.push(self.render_top(body_width));

        for _ in 0..self.padding.top {
            lines.push(self.render_padding_line(body_width));
        }

        for child_line in &child_cell.lines {
            lines.push(self.render_content_line(child_line, body_width));
        }

        for _ in 0..self.padding.bottom {
            lines.push(self.render_padding_line(body_width));
        }

        lines.push(self.render_bottom(body_width));

        Ok(Cell {
            height: lines.len(),
            width: outer_width,
            lines,
        })
    }
}

impl BoxWidget {
    fn render_top(&self, body_width: usize) -> StyledLine {
        let mut segments = Vec::with_capacity(4);
        segments.push(Segment::styled(
            self.chars.top_left.to_string(),
            self.border_style,
        ));

        if let Some(ref title) = self.title {
            let title_text = format!(" {title} ");
            let title_w = UnicodeWidthStr::width(title_text.as_str());
            segments.push(Segment::styled(title_text, self.title_style));

            if body_width > title_w {
                let fill = body_width - title_w;
                segments.push(Segment::styled(
                    self.chars.horizontal.to_string().repeat(fill),
                    self.border_style,
                ));
            }
        } else {
            segments.push(Segment::styled(
                self.chars.horizontal.to_string().repeat(body_width),
                self.border_style,
            ));
        }

        segments.push(Segment::styled(
            self.chars.top_right.to_string(),
            self.border_style,
        ));
        StyledLine::from_segments(segments)
    }

    fn render_bottom(&self, body_width: usize) -> StyledLine {
        StyledLine::from_segments(vec![
            Segment::styled(self.chars.bottom_left.to_string(), self.border_style),
            Segment::styled(
                self.chars.horizontal.to_string().repeat(body_width),
                self.border_style,
            ),
            Segment::styled(self.chars.bottom_right.to_string(), self.border_style),
        ])
    }

    fn render_padding_line(&self, body_width: usize) -> StyledLine {
        StyledLine::from_segments(vec![
            Segment::styled(self.chars.vertical.to_string(), self.border_style),
            Segment::plain(" ".repeat(body_width)),
            Segment::styled(self.chars.vertical.to_string(), self.border_style),
        ])
    }

    fn render_content_line(&self, child_line: &StyledLine, body_width: usize) -> StyledLine {
        let mut segments = Vec::with_capacity(child_line.segments.len() + 4);
        segments.push(Segment::styled(
            self.chars.vertical.to_string(),
            self.border_style,
        ));
        if self.padding.left > 0 {
            segments.push(Segment::plain(" ".repeat(self.padding.left)));
        }

        let used = child_line.width;
        for seg in &child_line.segments {
            segments.push(seg.clone());
        }

        let trailing = body_width
            .saturating_sub(used)
            .saturating_sub(self.padding.left);
        if trailing > 0 {
            segments.push(Segment::plain(" ".repeat(trailing)));
        }

        segments.push(Segment::styled(
            self.chars.vertical.to_string(),
            self.border_style,
        ));
        StyledLine::from_segments(segments)
    }
}
