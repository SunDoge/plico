//! The portable layout vocabulary maps here; Taffy stays out of application crates.
use plico_core::{Length, Track, TrackSize};
use taffy::{
    geometry::{Line, Rect},
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
fn edges<T>(e: plico_core::Edges<Length>, f: fn(Length) -> T) -> Rect<T> {
    Rect {
        top: f(e.top),
        right: f(e.right),
        bottom: f(e.bottom),
        left: f(e.left),
    }
}
fn align(a: plico_core::Align) -> AlignItems {
    match a {
        plico_core::Align::Start => AlignItems::START,
        plico_core::Align::Center => AlignItems::CENTER,
        plico_core::Align::End => AlignItems::END,
        plico_core::Align::Stretch => AlignItems::STRETCH,
        plico_core::Align::Baseline => AlignItems::BASELINE,
    }
}
fn justify(a: plico_core::Justify) -> AlignContent {
    match a {
        plico_core::Justify::Start => AlignContent::START,
        plico_core::Justify::Center => AlignContent::CENTER,
        plico_core::Justify::End => AlignContent::END,
        plico_core::Justify::SpaceBetween => AlignContent::SPACE_BETWEEN,
        plico_core::Justify::SpaceAround => AlignContent::SPACE_AROUND,
        plico_core::Justify::SpaceEvenly => AlignContent::SPACE_EVENLY,
        plico_core::Justify::Stretch => AlignContent::STRETCH,
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
fn placement(v: plico_core::GridLine) -> GridPlacement {
    match v {
        plico_core::GridLine::Auto => auto(),
        plico_core::GridLine::Line(v) => line(v),
        plico_core::GridLine::Span(v) => span(v),
    }
}
pub(super) fn apply(
    style: &mut Style,
    authored: plico_core::Style,
    grid: Option<&plico_core::GridTemplate>,
) {
    let l = authored.layout;
    style.display = match l.display {
        plico_core::Display::Flex => Display::Flex,
        plico_core::Display::Grid => Display::Grid,
        plico_core::Display::Block => Display::Block,
        plico_core::Display::None => Display::None,
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
            plico_core::FlexDirection::Row => FlexDirection::Row,
            plico_core::FlexDirection::Column => FlexDirection::Column,
            plico_core::FlexDirection::RowReverse => FlexDirection::RowReverse,
            plico_core::FlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
        };
    }
    if let Some(v) = l.flex_wrap {
        style.flex_wrap = match v {
            plico_core::FlexWrap::NoWrap => FlexWrap::NoWrap,
            plico_core::FlexWrap::Wrap => FlexWrap::Wrap,
            plico_core::FlexWrap::WrapReverse => FlexWrap::WrapReverse,
            plico_core::FlexWrap::Balance => FlexWrap::Balance,
            plico_core::FlexWrap::BalanceReverse => FlexWrap::BalanceReverse,
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
        plico_core::BoxSizing::BorderBox => BoxSizing::BorderBox,
        plico_core::BoxSizing::ContentBox => BoxSizing::ContentBox,
    };
    style.direction = match l.direction {
        plico_core::Direction::Ltr => Direction::Ltr,
        plico_core::Direction::Rtl => Direction::Rtl,
    };
    if l.position != plico_core::Position::Static {
        style.position = match l.position {
            plico_core::Position::Static => Position::Static,
            plico_core::Position::Relative => Position::Relative,
            plico_core::Position::Absolute => Position::Absolute,
        };
    }
    style.grid_auto_flow = match l.grid_flow {
        plico_core::GridFlow::Row => GridAutoFlow::Row,
        plico_core::GridFlow::Column => GridAutoFlow::Column,
        plico_core::GridFlow::RowDense => GridAutoFlow::RowDense,
        plico_core::GridFlow::ColumnDense => GridAutoFlow::ColumnDense,
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
