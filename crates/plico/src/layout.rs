//! Renderer-independent layout values. Percentages use 0–100 units.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
}
pub const AUTO: Length = Length::Auto;
pub const fn px(value: f32) -> Length {
    Length::Px(value)
}
pub const fn percent(value: f32) -> Length {
    Length::Percent(value)
}
macro_rules! numeric {($($t:ty),*)=>{$(impl From<$t> for Length {fn from(value:$t)->Self{Self::Px(value as f32)}})*}}
numeric!(f32, f64, u16, u32, usize, i32);
impl Length {
    pub fn valid(self, auto: bool, negative: bool) -> bool {
        match self {
            Self::Auto => auto,
            Self::Px(v) | Self::Percent(v) => v.is_finite() && (negative || v >= 0.),
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Edges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}
impl<T: Copy> Edges<T> {
    pub const fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
    pub fn values(self) -> [T; 4] {
        [self.top, self.right, self.bottom, self.left]
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Display {
    #[default]
    Flex,
    Grid,
    Block,
    None,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
    Balance,
    BalanceReverse,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoxSizing {
    #[default]
    BorderBox,
    ContentBox,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridFlow {
    #[default]
    Row,
    Column,
    RowDense,
    ColumnDense,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridLine {
    #[default]
    Auto,
    Line(i16),
    Span(u16),
}
impl GridLine {
    pub fn valid(self) -> bool {
        match self {
            Self::Auto => true,
            Self::Line(n) => n != 0,
            Self::Span(n) => n > 0,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutStyle {
    pub display: Display,
    pub min_size: [Option<Length>; 2],
    pub max_size: [Option<Length>; 2],
    pub margin: Option<Edges<Length>>,
    pub padding: Option<Edges<Length>>,
    pub inset: Option<Edges<Length>>,
    pub gap: [Option<Length>; 2],
    pub flex_direction: Option<FlexDirection>,
    pub flex_wrap: Option<FlexWrap>,
    pub flex_basis: Length,
    pub flex_line_count: u16,
    pub align_self: Option<crate::Align>,
    pub justify_self: Option<crate::Align>,
    pub justify_items: Option<crate::Align>,
    pub align_content: Option<crate::Justify>,
    pub aspect_ratio: Option<f32>,
    pub box_sizing: BoxSizing,
    pub direction: Direction,
    pub position: Position,
    pub grid_flow: GridFlow,
    pub grid_row: [GridLine; 2],
    pub grid_column: [GridLine; 2],
    pub template_revision: u64,
}
impl LayoutStyle {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn valid(self) -> bool {
        self.min_size
            .into_iter()
            .chain(self.max_size)
            .flatten()
            .all(|v| v.valid(true, false))
            && self
                .margin
                .is_none_or(|e| e.values().into_iter().all(|v| v.valid(true, true)))
            && self
                .padding
                .is_none_or(|e| e.values().into_iter().all(|v| v.valid(false, false)))
            && self
                .inset
                .is_none_or(|e| e.values().into_iter().all(|v| v.valid(true, true)))
            && self
                .gap
                .into_iter()
                .flatten()
                .all(|v| v.valid(false, false))
            && self.flex_basis.valid(true, false)
            && self.aspect_ratio.is_none_or(|v| v.is_finite() && v > 0.)
            && self
                .grid_row
                .into_iter()
                .chain(self.grid_column)
                .all(GridLine::valid)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TrackSize {
    Auto,
    MinContent,
    MaxContent,
    Px(f32),
    Percent(f32),
    Fr(f32),
}
impl TrackSize {
    fn valid(self, min: bool) -> bool {
        match self {
            Self::Auto | Self::MinContent | Self::MaxContent => true,
            Self::Px(v) | Self::Percent(v) => v.is_finite() && v >= 0.,
            Self::Fr(v) => !min && v.is_finite() && v >= 0.,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Track {
    Auto,
    MinContent,
    MaxContent,
    Px(f32),
    Percent(f32),
    Fr(f32),
    MinMax(TrackSize, TrackSize),
    FitContent(Length),
}
pub const fn fr(value: f32) -> Track {
    Track::Fr(value)
}
pub const fn minmax(min: TrackSize, max: TrackSize) -> Track {
    Track::MinMax(min, max)
}
impl From<Length> for Track {
    fn from(v: Length) -> Self {
        match v {
            Length::Auto => Self::Auto,
            Length::Px(v) => Self::Px(v),
            Length::Percent(v) => Self::Percent(v),
        }
    }
}
impl From<&Track> for Track {
    fn from(v: &Track) -> Self {
        *v
    }
}
impl Track {
    pub fn valid(self) -> bool {
        match self {
            Self::Auto | Self::MinContent | Self::MaxContent => true,
            Self::Px(v) | Self::Percent(v) | Self::Fr(v) => v.is_finite() && v >= 0.,
            Self::MinMax(a, b) => a.valid(true) && b.valid(false),
            Self::FitContent(v) => v.valid(false, false),
        }
    }
}
/// Variable-size grid data is retained separately from the Copy Style value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GridTemplate {
    pub columns: Vec<Track>,
    pub rows: Vec<Track>,
    pub auto_columns: Vec<Track>,
    pub auto_rows: Vec<Track>,
    /// Space-separated cell names, `.` means an unnamed cell. Areas must be rectangular.
    pub areas: Vec<String>,
    /// Named area occupied by this item, resolved by the parent grid.
    pub area: Option<String>,
}
impl GridTemplate {
    pub fn valid(&self) -> bool {
        if self.columns.len() + self.rows.len() + self.auto_columns.len() + self.auto_rows.len()
            > 1024
            || !self
                .columns
                .iter()
                .chain(&self.rows)
                .chain(&self.auto_columns)
                .chain(&self.auto_rows)
                .all(|t| t.valid())
        {
            return false;
        }
        if self.areas.len() > 256 {
            return false;
        }
        let rows: Vec<Vec<_>> = self
            .areas
            .iter()
            .map(|r| r.split_whitespace().collect())
            .collect();
        let width = rows.first().map_or(0, Vec::len);
        if width > 256 || rows.iter().any(|r| r.len() != width || r.is_empty()) {
            return false;
        }
        let mut bounds = std::collections::HashMap::new();
        for (y, row) in rows.iter().enumerate() {
            for (x, name) in row.iter().enumerate() {
                if *name != "." {
                    if !valid_name(name) {
                        return false;
                    }
                    let b = bounds.entry(*name).or_insert((x, y, x, y, 0));
                    b.0 = b.0.min(x);
                    b.1 = b.1.min(y);
                    b.2 = b.2.max(x);
                    b.3 = b.3.max(y);
                    b.4 += 1;
                }
            }
        }
        bounds
            .values()
            .all(|&(x0, y0, x1, y1, n)| n == (x1 - x0 + 1) * (y1 - y0 + 1))
            && self.area.as_ref().is_none_or(|a| valid_name(a))
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}
impl crate::Style {
    pub(crate) fn set_dimension(&mut self, name: &str, value: Length) {
        let slot = match name {
            "width" => &mut self.width,
            "height" => &mut self.height,
            "min_width" => &mut self.min_width,
            "min_height" => &mut self.min_height,
            "max_width" => &mut self.max_width,
            "max_height" => &mut self.max_height,
            _ => unreachable!(),
        };
        *slot = if let Length::Px(v) = value {
            Some(v)
        } else {
            None
        };
        match name {
            "width" => {
                self.width_percent = if let Length::Percent(v) = value {
                    Some(v)
                } else {
                    None
                }
            }
            "height" => {
                self.height_percent = if let Length::Percent(v) = value {
                    Some(v)
                } else {
                    None
                }
            }
            _ => {
                let axis = usize::from(name.ends_with("height"));
                let slots = if name.starts_with("min") {
                    &mut self.layout.min_size
                } else {
                    &mut self.layout.max_size
                };
                slots[axis] = if matches!(value, Length::Px(_)) {
                    None
                } else {
                    Some(value)
                };
            }
        }
    }
    fn resolved_edges(self, name: &str) -> Edges<Length> {
        match name {
            "margin" => self.layout.margin.unwrap_or(Edges::all(px(0.))),
            "padding" => self.layout.padding.unwrap_or(Edges::new(
                px(self.vertical_padding()),
                px(self.horizontal_padding()),
                px(self.vertical_padding()),
                px(self.horizontal_padding()),
            )),
            "inset" => self.layout.inset.unwrap_or_else(|| {
                self.absolute.map_or(Edges::all(AUTO), |[x, y]| {
                    Edges::new(px(y), AUTO, AUTO, px(x))
                })
            }),
            _ => unreachable!(),
        }
    }
    pub(crate) fn set_edges(&mut self, name: &str, value: Edges<Length>) {
        match name {
            "margin" => self.layout.margin = Some(value),
            "padding" => self.layout.padding = Some(value),
            "inset" => self.layout.inset = Some(value),
            _ => unreachable!(),
        }
    }
    pub(crate) fn set_edge(&mut self, name: &str, side: &str, value: Length) {
        let mut edges = self.resolved_edges(name);
        match side {
            "top" => edges.top = value,
            "right" => edges.right = value,
            "bottom" => edges.bottom = value,
            "left" => edges.left = value,
            _ => unreachable!(),
        }
        self.set_edges(name, edges);
    }
    pub(crate) fn set_spacing(&mut self, name: &str, value: Length) {
        if name == "gap" {
            self.gap = if let Length::Px(v) = value { v } else { 0. };
            self.layout.gap = if matches!(value, Length::Px(_)) {
                [None, None]
            } else {
                [Some(value); 2]
            };
            return;
        }
        if name == "padding" {
            self.padding = if let Length::Px(v) = value { v } else { 0. };
            self.padding_x = None;
            self.padding_y = None;
            self.layout.padding = if matches!(value, Length::Px(_)) {
                None
            } else {
                Some(Edges::all(value))
            };
            return;
        }
        if self.layout.padding.is_none()
            && let Length::Px(v) = value
        {
            if name == "padding_x" {
                self.padding_x = Some(v);
            } else {
                self.padding_y = Some(v);
            }
            return;
        }
        for side in if name == "padding_x" {
            ["left", "right"]
        } else {
            ["top", "bottom"]
        } {
            self.set_edge("padding", side, value);
        }
    }
}
