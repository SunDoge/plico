//! The portable layout vocabulary maps here; Taffy stays out of application crates.
use crate::{Kind, Length, Style as NodeStyle, Track, TrackSize};
use taffy::{
    geometry::{Line, Rect, Size},
    style::*,
    style_helpers::*,
};
fn lp(v: Length) -> LengthPercentage {
    match v {
        Length::Px(v) => length(v),
        Length::Percent(v) => percent(v / 100.),
        Length::Auto => length(0.),
    }
}
fn lpa(v: Length) -> LengthPercentageAuto {
    match v {
        Length::Px(v) => length(v),
        Length::Percent(v) => percent(v / 100.),
        Length::Auto => auto(),
    }
}
fn dim(v: Length) -> Dimension {
    match v {
        Length::Px(v) => length(v),
        Length::Percent(v) => percent(v / 100.),
        Length::Auto => auto(),
    }
}
fn edges<T>(e: crate::Edges<Length>, f: fn(Length) -> T) -> Rect<T> {
    Rect {
        top: f(e.top),
        right: f(e.right),
        bottom: f(e.bottom),
        left: f(e.left),
    }
}
fn align(a: crate::Align) -> AlignItems {
    match a {
        crate::Align::Start => AlignItems::START,
        crate::Align::Center => AlignItems::CENTER,
        crate::Align::End => AlignItems::END,
        crate::Align::Stretch => AlignItems::STRETCH,
        crate::Align::Baseline => AlignItems::BASELINE,
    }
}
fn justify(a: crate::Justify) -> AlignContent {
    match a {
        crate::Justify::Start => AlignContent::START,
        crate::Justify::Center => AlignContent::CENTER,
        crate::Justify::End => AlignContent::END,
        crate::Justify::SpaceBetween => AlignContent::SPACE_BETWEEN,
        crate::Justify::SpaceAround => AlignContent::SPACE_AROUND,
        crate::Justify::SpaceEvenly => AlignContent::SPACE_EVENLY,
        crate::Justify::Stretch => AlignContent::STRETCH,
    }
}
fn track_min(t: TrackSize) -> MinTrackSizingFunction {
    match t {
        TrackSize::Auto => auto(),
        TrackSize::MinContent => min_content(),
        TrackSize::MaxContent => max_content(),
        TrackSize::Px(v) => length(v),
        TrackSize::Percent(v) => percent(v / 100.),
        TrackSize::Fr(_) => unreachable!("validated track minimum"),
    }
}
fn track_max(t: TrackSize) -> MaxTrackSizingFunction {
    match t {
        TrackSize::Auto => auto(),
        TrackSize::MinContent => min_content(),
        TrackSize::MaxContent => max_content(),
        TrackSize::Px(v) => length(v),
        TrackSize::Percent(v) => percent(v / 100.),
        TrackSize::Fr(v) => fr(v),
    }
}
fn track(t: Track) -> TrackSizingFunction {
    match t {
        Track::Auto => auto(),
        Track::MinContent => min_content(),
        Track::MaxContent => max_content(),
        Track::Px(v) => length(v),
        Track::Percent(v) => percent(v / 100.),
        Track::Fr(v) => fr(v),
        Track::MinMax(a, b) => minmax(track_min(a), track_max(b)),
        Track::FitContent(v) => fit_content(lp(v)),
    }
}
fn placement(v: crate::GridLine) -> GridPlacement {
    match v {
        crate::GridLine::Auto => auto(),
        crate::GridLine::Line(v) => line(v),
        crate::GridLine::Span(v) => span(v),
    }
}
pub(crate) fn apply(style: &mut Style, authored: crate::Style, grid: Option<&crate::GridTemplate>) {
    let l = authored.layout;
    style.display = match l.display {
        crate::Display::Flex => Display::Flex,
        crate::Display::Grid => Display::Grid,
        crate::Display::Block => Display::Block,
        crate::Display::None => Display::None,
    };
    if let Some(v) = l.min_size[0] {
        style.min_size.width = lpa(v);
    }
    if let Some(v) = l.min_size[1] {
        style.min_size.height = lpa(v);
    }
    if let Some(v) = l.max_size[0] {
        style.max_size.width = lpa(v);
    }
    if let Some(v) = l.max_size[1] {
        style.max_size.height = lpa(v);
    }
    if let Some(v) = l.padding {
        style.padding = edges(v, lp);
    }
    if let Some(v) = l.margin {
        style.margin = edges(v, lpa);
    }
    if let Some(v) = l.inset {
        style.inset = edges(v, lpa);
    }
    if let Some(v) = l.gap[0] {
        style.gap.width = lp(v);
    }
    if let Some(v) = l.gap[1] {
        style.gap.height = lp(v);
    }
    if let Some(v) = l.flex_direction {
        style.flex_direction = match v {
            crate::FlexDirection::Row => FlexDirection::Row,
            crate::FlexDirection::Column => FlexDirection::Column,
            crate::FlexDirection::RowReverse => FlexDirection::RowReverse,
            crate::FlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
        };
    }
    if let Some(v) = l.flex_wrap {
        style.flex_wrap = match v {
            crate::FlexWrap::NoWrap => FlexWrap::NoWrap,
            crate::FlexWrap::Wrap => FlexWrap::Wrap,
            crate::FlexWrap::WrapReverse => FlexWrap::WrapReverse,
            crate::FlexWrap::Balance => FlexWrap::Balance,
            crate::FlexWrap::BalanceReverse => FlexWrap::BalanceReverse,
        };
    }
    style.flex_line_count = l.flex_line_count.max(1);
    style.flex_basis = dim(l.flex_basis);
    style.align_self = l.align_self.map(align);
    style.justify_self = l.justify_self.map(align);
    if let Some(v) = l.justify_items {
        style.justify_items = align(v);
    }
    if let Some(v) = l.align_content {
        style.align_content = justify(v);
    }
    style.aspect_ratio = l.aspect_ratio;
    style.box_sizing = match l.box_sizing {
        crate::BoxSizing::BorderBox => BoxSizing::BorderBox,
        crate::BoxSizing::ContentBox => BoxSizing::ContentBox,
    };
    style.direction = match l.direction {
        crate::Direction::Ltr => Direction::Ltr,
        crate::Direction::Rtl => Direction::Rtl,
    };
    if l.position != crate::Position::Static {
        style.position = match l.position {
            crate::Position::Static => Position::Static,
            crate::Position::Relative => Position::Relative,
            crate::Position::Absolute => Position::Absolute,
        };
    }
    style.grid_auto_flow = match l.grid_flow {
        crate::GridFlow::Row => GridAutoFlow::Row,
        crate::GridFlow::Column => GridAutoFlow::Column,
        crate::GridFlow::RowDense => GridAutoFlow::RowDense,
        crate::GridFlow::ColumnDense => GridAutoFlow::ColumnDense,
    };
    style.grid_row = Line {
        start: placement(l.grid_row[0]),
        end: placement(l.grid_row[1]),
    };
    style.grid_column = Line {
        start: placement(l.grid_column[0]),
        end: placement(l.grid_column[1]),
    };
    if let Some(g) = grid {
        style.grid_template_columns = g
            .columns
            .iter()
            .map(|t| GridTemplateComponent::Single(track(*t)))
            .collect();
        style.grid_template_rows = g
            .rows
            .iter()
            .map(|t| GridTemplateComponent::Single(track(*t)))
            .collect();
        style.grid_auto_columns = g.auto_columns.iter().map(|t| track(*t)).collect();
        style.grid_auto_rows = g.auto_rows.iter().map(|t| track(*t)).collect();
        if !g.areas.is_empty() {
            let mut areas = std::collections::BTreeMap::new();
            let mut width = 0;
            for (y, row) in g.areas.iter().enumerate() {
                for (x, name) in row.split_whitespace().enumerate() {
                    width = width.max(x + 1);
                    if name != "." {
                        let area = areas.entry(name.to_string()).or_insert(GridTemplateArea {
                            name: name.into(),
                            row_start: y as u16 + 1,
                            row_end: y as u16 + 2,
                            column_start: x as u16 + 1,
                            column_end: x as u16 + 2,
                        });
                        area.row_end = y as u16 + 2;
                        area.column_end = area.column_end.max(x as u16 + 2);
                    }
                }
            }
            style.grid_template_areas = Some(GridTemplateAreas {
                areas: areas.into_values().collect(),
                row_count: g.areas.len() as u16,
                column_count: width as u16,
            });
        }
        if let Some(area) = &g.area {
            style.grid_row = Line {
                start: GridPlacement::NamedLine(format!("{area}-start"), 1),
                end: GridPlacement::NamedLine(format!("{area}-end"), 1),
            };
            style.grid_column = style.grid_row.clone();
        }
    }
}

/// Convert a Plico node style to Taffy geometry, without choosing text or controls.
pub fn style(kind: Kind, s: NodeStyle, grid: Option<&crate::GridTemplate>) -> taffy::Style {
    let mut style = taffy::Style {
        flex_direction: if matches!(kind, Kind::Row | Kind::ButtonBase) {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        },
        align_items: if matches!(kind, Kind::Root | Kind::Column) {
            AlignItems::STRETCH
        } else {
            AlignItems::CENTER
        },
        size: Size {
            width: s
                .width_percent
                .map_or_else(|| s.width.map_or(auto(), length), |v| percent(v / 100.)),
            height: s
                .height_percent
                .map_or_else(|| s.height.map_or(auto(), length), |v| percent(v / 100.)),
        },
        min_size: Size {
            width: s.min_width.map_or(length(0.), length),
            height: s.min_height.map_or(auto(), length),
        },
        max_size: Size {
            width: s.max_width.map_or(auto(), length),
            height: s.max_height.map_or(auto(), length),
        },
        padding: Rect {
            left: length(s.horizontal_padding()),
            right: length(s.horizontal_padding()),
            top: length(s.vertical_padding()),
            bottom: length(s.vertical_padding()),
        },
        gap: Size {
            width: length(s.gap),
            height: length(s.gap),
        },
        overflow: taffy::Point {
            x: if s.clip {
                taffy::Overflow::Hidden
            } else {
                taffy::Overflow::Visible
            },
            y: if s.scroll_y {
                taffy::Overflow::Scroll
            } else if s.clip {
                taffy::Overflow::Hidden
            } else {
                taffy::Overflow::Visible
            },
        },
        scrollbar_width: if s.scroll_y { 16. } else { 0. },
        border: Rect {
            left: length(s.border_width),
            right: length(s.border_width),
            top: length(s.border_width),
            bottom: length(s.border_width),
        },
        flex_grow: s.grow,
        flex_shrink: s.shrink,
        flex_wrap: if s.wrap {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        },
        ..Default::default()
    };
    if let Some(align) = s.align {
        style.align_items = match align {
            crate::Align::Start => AlignItems::START,
            crate::Align::End => AlignItems::END,
            crate::Align::Center => AlignItems::CENTER,
            crate::Align::Stretch => AlignItems::STRETCH,
            crate::Align::Baseline => AlignItems::BASELINE,
        };
    }
    if let Some(justify) = s.justify {
        style.justify_content = match justify {
            crate::Justify::Start => JustifyContent::START,
            crate::Justify::End => JustifyContent::END,
            crate::Justify::Center => JustifyContent::CENTER,
            crate::Justify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
            crate::Justify::SpaceAround => JustifyContent::SPACE_AROUND,
            crate::Justify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
            crate::Justify::Stretch => JustifyContent::STRETCH,
        };
    }
    if let Some([left, top]) = s.absolute {
        style.position = Position::Absolute;
        style.inset.left = length(left);
        style.inset.top = length(top);
    }
    apply(&mut style, s, grid);
    style
}
