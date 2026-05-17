//! `spacer` widget (spec §8) — fixed-size empty cell.
//!
//! Used inside `row` / `stack` to push siblings around without a visible
//! border or content. In horizontal contexts `width` matters; in vertical
//! contexts `height` matters; supply both if the surrounding context can
//! go either way.

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::StaticContext;
use crate::error::{ConfigError, RenderError};
use crate::style::StyledLine;

use super::{Cell, Widget};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpacerConfig {
    #[serde(default = "default_size")]
    pub width: usize,
    #[serde(default = "default_height")]
    pub height: usize,
    #[serde(default)]
    pub show_if: Option<String>,
}

fn default_size() -> usize {
    1
}
fn default_height() -> usize {
    1
}

pub struct SpacerWidget {
    width: usize,
    height: usize,
}

impl SpacerWidget {
    pub fn build(cfg: SpacerConfig, _ctx: &StaticContext) -> Result<Self, ConfigError> {
        Ok(Self {
            width: cfg.width,
            height: cfg.height,
        })
    }
}

impl Widget for SpacerWidget {
    fn render(
        &self,
        _registry: &CollectorRegistry,
        _max_width: Option<usize>,
    ) -> Result<Cell, RenderError> {
        let line = StyledLine::plain(" ".repeat(self.width));
        let lines: Vec<StyledLine> = (0..self.height).map(|_| line.clone()).collect();
        Ok(Cell {
            lines,
            width: self.width,
            height: self.height,
        })
    }
}
