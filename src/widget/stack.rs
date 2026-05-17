//! `stack` widget (spec §8) — vertical composition of children.

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::StaticContext;
use crate::error::{ConfigError, RenderError};
use crate::style::StyledLine;

use super::{Cell, Widget, WidgetConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StackConfig {
    #[serde(default)]
    pub gap: usize,
    pub children: Vec<WidgetConfig>,
    #[serde(default)]
    pub show_if: Option<String>,
}

pub struct StackWidget {
    gap: usize,
    children: Vec<Box<dyn Widget>>,
}

impl StackWidget {
    pub fn build(cfg: StackConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let children = cfg
            .children
            .into_iter()
            .map(|c| c.build(ctx))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            gap: cfg.gap,
            children,
        })
    }
}

impl Widget for StackWidget {
    fn render(
        &self,
        registry: &CollectorRegistry,
        max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        if self.children.is_empty() {
            return Ok(Cell::empty());
        }

        let rendered: Vec<Cell> = self
            .children
            .iter()
            .map(|c| c.render(registry, max_width))
            .collect::<Result<_, _>>()?;

        let width = rendered.iter().map(|c| c.width).max().unwrap_or(0);

        let mut lines: Vec<StyledLine> = Vec::new();
        for (i, child) in rendered.into_iter().enumerate() {
            if i > 0 {
                for _ in 0..self.gap {
                    lines.push(StyledLine::empty());
                }
            }
            for mut line in child.lines {
                line.pad_to(width);
                lines.push(line);
            }
        }

        let height = lines.len();
        Ok(Cell {
            lines,
            width,
            height,
        })
    }
}
