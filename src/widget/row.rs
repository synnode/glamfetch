//! Inner `row` widget (spec §8) — horizontal composition inside a cell.
//!
//! Composes children left-to-right with `gap` columns between them. Each
//! child is rendered to its own width, then padded vertically to the row's
//! tallest child according to `align`.

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::StaticContext;
use crate::error::{ConfigError, RenderError};
use crate::style::{Segment, StyledLine};

use super::{Cell, Widget, WidgetConfig};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowConfig {
    #[serde(default)]
    pub gap: usize,
    #[serde(default)]
    pub align: VAlign,
    pub children: Vec<WidgetConfig>,
    #[serde(default)]
    pub show_if: Option<String>,
}

pub struct RowWidget {
    gap: usize,
    align: VAlign,
    children: Vec<Box<dyn Widget>>,
}

impl RowWidget {
    pub fn build(cfg: RowConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let children = cfg
            .children
            .into_iter()
            .map(|c| c.build(ctx))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            gap: cfg.gap,
            align: cfg.align,
            children,
        })
    }
}

impl Widget for RowWidget {
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
        let rendered: Vec<Cell> = rendered
            .into_iter()
            .filter(|c| c.width > 0 && c.height > 0)
            .collect();

        if rendered.is_empty() {
            return Ok(Cell::empty());
        }

        let row_height = rendered.iter().map(|c| c.height).max().unwrap_or(0);
        let gap_seg = if self.gap > 0 {
            Some(Segment::plain(" ".repeat(self.gap)))
        } else {
            None
        };

        let mut lines: Vec<StyledLine> = Vec::with_capacity(row_height);
        for line_idx in 0..row_height {
            let mut segments = Vec::new();
            for (cell_idx, cell) in rendered.iter().enumerate() {
                if cell_idx > 0
                    && let Some(ref gap) = gap_seg
                {
                    segments.push(gap.clone());
                }

                let line_for_cell = vertical_lookup(cell, line_idx, row_height, self.align);
                let printed = line_for_cell.as_ref().map(|l| l.width).unwrap_or(0);
                if let Some(line) = line_for_cell {
                    for seg in &line.segments {
                        segments.push(seg.clone());
                    }
                }
                if printed < cell.width {
                    segments.push(Segment::plain(" ".repeat(cell.width - printed)));
                }
            }
            lines.push(StyledLine::from_segments(segments));
        }

        let width = lines.iter().map(|l| l.width).max().unwrap_or(0);
        Ok(Cell {
            lines,
            width,
            height: row_height,
        })
    }
}

/// Pick the source row for line `target` once the cell has been padded to
/// `row_height` according to `align`.
fn vertical_lookup(
    cell: &Cell,
    target: usize,
    row_height: usize,
    align: VAlign,
) -> Option<&StyledLine> {
    let pad = row_height.saturating_sub(cell.height);
    let (top, bottom) = match align {
        VAlign::Top => (0, pad),
        VAlign::Bottom => (pad, 0),
        VAlign::Middle => {
            let t = pad / 2;
            (t, pad - t)
        }
    };
    if target < top {
        return None;
    }
    let src_idx = target - top;
    if src_idx >= cell.height || src_idx + bottom + top < target {
        return None;
    }
    cell.lines.get(src_idx)
}
