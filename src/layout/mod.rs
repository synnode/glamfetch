//! Layout layer (spec §4.5).
//!
//! Phase 2: rows of cells with horizontal/vertical gap, top-align rows,
//! unstyled space padding between cells. `show_if`, alignment, padding,
//! and width propagation arrive in later phases.

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::StaticContext;
use crate::error::{ConfigError, RenderError};
use crate::style::{Segment, StyledLine};
use crate::widget::{Cell, Widget, WidgetConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowConfig {
    #[serde(default = "default_row_gap")]
    pub gap: usize,
    #[serde(default, rename = "cell")]
    pub cells: Vec<WidgetConfig>,
}

fn default_row_gap() -> usize {
    2
}

pub struct Row {
    gap: usize,
    cells: Vec<Box<dyn Widget>>,
}

impl Row {
    pub fn build(cfg: RowConfig, ctx: &StaticContext) -> Result<Self, ConfigError> {
        let cells = cfg
            .cells
            .into_iter()
            .map(|c| c.build(ctx))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            gap: cfg.gap,
            cells,
        })
    }

    pub fn render(&self, registry: &CollectorRegistry) -> Result<Vec<StyledLine>, RenderError> {
        if self.cells.is_empty() {
            return Ok(Vec::new());
        }

        let rendered_all: Vec<Cell> = self
            .cells
            .iter()
            .map(|c| c.render(registry, None))
            .collect::<Result<_, _>>()?;

        // Drop empty cells (e.g. `show_if = false`) before layout so the
        // remaining cells close up without an extra gap on either side
        // (spec §6.5 / §9.3).
        let rendered: Vec<Cell> = rendered_all
            .into_iter()
            .filter(|c| c.width > 0 && c.height > 0)
            .collect();

        let row_height = rendered.iter().map(|c| c.height).max().unwrap_or(0);
        if row_height == 0 {
            return Ok(Vec::new());
        }

        let gap_segment = if self.gap > 0 {
            Some(Segment::plain(" ".repeat(self.gap)))
        } else {
            None
        };

        let mut out: Vec<StyledLine> = Vec::with_capacity(row_height);

        for line_idx in 0..row_height {
            let mut segments: Vec<Segment> = Vec::new();
            for (cell_idx, cell) in rendered.iter().enumerate() {
                if cell_idx > 0
                    && let Some(ref gap) = gap_segment
                {
                    segments.push(gap.clone());
                }

                let cell_line = cell.lines.get(line_idx).cloned().unwrap_or_default();
                let printed = cell_line.width;
                for seg in cell_line.segments {
                    segments.push(seg);
                }
                if printed < cell.width {
                    let pad = cell.width - printed;
                    segments.push(Segment::plain(" ".repeat(pad)));
                }
            }
            out.push(StyledLine::from_segments(segments));
        }

        Ok(out)
    }
}

pub struct Layout {
    rows: Vec<Row>,
    gap: usize,
}

impl Layout {
    pub fn new(rows: Vec<Row>, gap: usize) -> Self {
        Self { rows, gap }
    }

    /// Build the full styled frame (Vec<StyledLine>) for downstream
    /// renderers. Top-level horizontal alignment + truncation land in a
    /// later phase.
    pub fn render(&self, registry: &CollectorRegistry) -> Result<Vec<StyledLine>, RenderError> {
        let mut out: Vec<StyledLine> = Vec::new();
        for (idx, row) in self.rows.iter().enumerate() {
            if idx > 0 {
                for _ in 0..self.gap {
                    out.push(StyledLine::empty());
                }
            }
            out.extend(row.render(registry)?);
        }
        Ok(out)
    }
}
