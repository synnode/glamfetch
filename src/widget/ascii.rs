//! `ascii` widget (spec §8) — inline or file-sourced ASCII art block.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::color_spec::{ColorSpec, resolve_optional};
use crate::config::expr::StaticContext;
use crate::error::{ConfigError, RenderError};
use crate::style::{PaintSpec, Style, StyledLine};

use super::{Cell, Widget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsciiConfig {
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub color: Option<ColorSpec>,
    #[serde(default)]
    pub show_if: Option<String>,
}

fn default_source() -> String {
    "inline".into()
}

pub struct AsciiWidget {
    lines: Vec<String>,
    paint: PaintSpec,
}

impl AsciiWidget {
    pub fn build(cfg: AsciiConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let raw = match cfg.source.as_str() {
            "inline" => cfg.content.ok_or_else(|| {
                ConfigError::Invalid("ascii widget: source=inline requires `content`".into())
            })?,
            "file" => {
                let path_str = cfg.path.ok_or_else(|| {
                    ConfigError::Invalid("ascii widget: source=file requires `path`".into())
                })?;
                let path = expand_tilde(&path_str);
                std::fs::read_to_string(&path).map_err(|err| {
                    ConfigError::Invalid(format!(
                        "ascii widget: cannot read {}: {err}",
                        path.display()
                    ))
                })?
            }
            other => {
                return Err(ConfigError::Invalid(format!(
                    "ascii widget: unknown source `{other}` (expected `inline` or `file`)"
                )));
            }
        };

        let lines: Vec<String> = raw
            .trim_start_matches('\n')
            .trim_end_matches('\n')
            .lines()
            .map(|l| l.to_string())
            .collect();

        let paint = resolve_optional(cfg.color.as_ref(), Style::plain(), ctx)?;

        Ok(Self { lines, paint })
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(stripped);
    }
    PathBuf::from(path)
}

impl Widget for AsciiWidget {
    fn render(
        &self,
        _registry: &CollectorRegistry,
        _max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let lines: Vec<StyledLine> = self
            .lines
            .iter()
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
