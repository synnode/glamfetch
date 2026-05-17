//! Widget layer (spec §4.4).
//!
//! Widgets produce a [`Cell`] of [`StyledLine`]s. Style propagation happens
//! at the segment level: a widget either emits plain segments and lets the
//! caller style them later, or applies its own style at construction time
//! (e.g. a text widget with a configured `color`).

pub mod ascii;
pub mod bar;
pub mod boxw;
pub mod figlet;
pub mod gauge;
pub mod row;
pub mod separator;
pub mod show_if;
pub mod spacer;
pub mod stack;
pub mod text;

use serde::{Deserialize, Serialize};

use crate::collect::CollectorRegistry;
use crate::config::expr::StaticContext;
use crate::error::{ConfigError, RenderError};
use crate::style::StyledLine;

/// Output of a widget render: one or more styled lines plus measured bounds.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub lines: Vec<StyledLine>,
    pub width: usize,
    pub height: usize,
}

impl Cell {
    pub fn from_lines(lines: Vec<StyledLine>) -> Self {
        let width = lines.iter().map(|l| l.width).max().unwrap_or(0);
        let height = lines.len();
        Self {
            lines,
            width,
            height,
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }
}

pub trait Widget: Send + Sync {
    fn render(
        &self,
        registry: &CollectorRegistry,
        max_width: Option<usize>,
    ) -> Result<Cell, RenderError>;
}

/// Typed widget configuration parsed from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "widget", rename_all = "lowercase")]
pub enum WidgetConfig {
    Text(text::TextConfig),
    Stack(stack::StackConfig),
    #[serde(rename = "box")]
    Box(boxw::BoxConfig),
    Gauge(gauge::GaugeConfig),
    Bar(bar::BarConfig),
    Row(row::RowConfig),
    Spacer(spacer::SpacerConfig),
    Separator(separator::SeparatorConfig),
    Figlet(figlet::FigletConfig),
    Ascii(ascii::AsciiConfig),
}

impl WidgetConfig {
    pub fn build(self, ctx: &StaticContext) -> Result<Box<dyn Widget>, ConfigError> {
        let show_if = self.show_if();
        let inner: Box<dyn Widget> = match self {
            Self::Text(cfg) => Box::new(text::TextWidget::build(cfg, ctx)?),
            Self::Stack(cfg) => Box::new(stack::StackWidget::build(cfg, ctx)?),
            Self::Box(cfg) => Box::new(boxw::BoxWidget::build(cfg, ctx)?),
            Self::Gauge(cfg) => Box::new(gauge::GaugeWidget::build(cfg, ctx)?),
            Self::Bar(cfg) => Box::new(bar::BarWidget::build(cfg, ctx)?),
            Self::Row(cfg) => Box::new(row::RowWidget::build(cfg, ctx)?),
            Self::Spacer(cfg) => Box::new(spacer::SpacerWidget::build(cfg, ctx)?),
            Self::Separator(cfg) => Box::new(separator::SeparatorWidget::build(cfg, ctx)?),
            Self::Figlet(cfg) => Box::new(figlet::FigletWidget::build(cfg, ctx)?),
            Self::Ascii(cfg) => Box::new(ascii::AsciiWidget::build(cfg, ctx)?),
        };
        match show_if {
            Some(expr) => Ok(Box::new(show_if::ShowIfWidget::wrap(
                expr,
                ctx.clone(),
                inner,
            ))),
            None => Ok(inner),
        }
    }

    fn show_if(&self) -> Option<String> {
        match self {
            Self::Text(cfg) => cfg.show_if.clone(),
            Self::Stack(cfg) => cfg.show_if.clone(),
            Self::Box(cfg) => cfg.show_if.clone(),
            Self::Gauge(cfg) => cfg.show_if.clone(),
            Self::Bar(cfg) => cfg.show_if.clone(),
            Self::Row(cfg) => cfg.show_if.clone(),
            Self::Spacer(cfg) => cfg.show_if.clone(),
            Self::Separator(cfg) => cfg.show_if.clone(),
            Self::Figlet(cfg) => cfg.show_if.clone(),
            Self::Ascii(cfg) => cfg.show_if.clone(),
        }
    }
}
