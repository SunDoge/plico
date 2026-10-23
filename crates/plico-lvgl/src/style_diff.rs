//! Compare resolved native values, including explicit defaults. Local setters
//! override the initial owned style without detaching it or invalidating every
//! property. Unchanged pixel-quantized values produce no native calls.
use crate::ffi::{self, Obj, color, pct};
use lightvgl_sys as sys;
use plico_core::{Align, Justify, Kind, KindFamily, Style, StyleProperties, Theme};

#[derive(Clone, Copy)]
struct Values {
    width: i32,
    height: i32,
    pad_x: i32,
    pad_y: i32,
    gap: i32,
    radius: i32,
    border_width: i32,
    border_color: u32,
    text_color: u32,
    font24: bool,
    bg: u32,
    bg_opa: u8,
    opa: u8,
    layout: u16,
    flow: u32,
    main: u32,
    cross: u32,
    disabled: bool,
}
impl Values {
    fn new(kind: Kind, style: Style, theme: Theme) -> Self {
        let dimension = |pixels: Option<f32>, percent: Option<f32>| {
            pixels
                .map(|v| v as i32)
                .or_else(|| percent.map(|v| pct(v as i32)))
                .unwrap_or(if kind == Kind::Root {
                    pct(100)
                } else {
                    ffi::SIZE_CONTENT
                })
        };
        Self {
            width: dimension(style.width, style.width_percent),
            height: dimension(style.height, style.height_percent),
            pad_x: style.horizontal_padding() as i32,
            pad_y: style.vertical_padding() as i32,
            gap: style.gap as i32,
            radius: style.radius as i32,
            border_width: style.border_width as i32,
            border_color: theme.resolve(style.border_color),
            text_color: theme.resolve(style.foreground),
            font24: style.font_size >= 24,
            bg: style.background.map(|c| theme.resolve(c)).unwrap_or(0),
            bg_opa: if style.background.is_some() {
                opacity(style.background_alpha)
            } else {
                0
            },
            opa: opacity(style.opacity),
            layout: if matches!(kind.family(), KindFamily::Container | KindFamily::Control) {
                sys::lv_layout_t_LV_LAYOUT_FLEX as u16
            } else {
                0
            },
            flow: if matches!(kind, Kind::Row | Kind::ButtonBase) {
                sys::lv_flex_flow_t_LV_FLEX_FLOW_ROW
            } else {
                sys::lv_flex_flow_t_LV_FLEX_FLOW_COLUMN
            },
            main: match style.justify.unwrap_or(Justify::Start) {
                Justify::Center => sys::lv_flex_align_t_LV_FLEX_ALIGN_CENTER,
                Justify::End => sys::lv_flex_align_t_LV_FLEX_ALIGN_END,
                Justify::SpaceBetween => sys::lv_flex_align_t_LV_FLEX_ALIGN_SPACE_BETWEEN,
                Justify::SpaceAround => sys::lv_flex_align_t_LV_FLEX_ALIGN_SPACE_AROUND,
                Justify::SpaceEvenly => sys::lv_flex_align_t_LV_FLEX_ALIGN_SPACE_EVENLY,
                _ => sys::lv_flex_align_t_LV_FLEX_ALIGN_START,
            },
            cross: match style.align.unwrap_or(Align::Start) {
                Align::Center => sys::lv_flex_align_t_LV_FLEX_ALIGN_CENTER,
                Align::End => sys::lv_flex_align_t_LV_FLEX_ALIGN_END,
                _ => sys::lv_flex_align_t_LV_FLEX_ALIGN_START,
            },
            disabled: style.disabled,
        }
    }
}
pub(super) fn opacity(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

pub(super) fn apply(
    obj: *mut Obj,
    before: (Kind, Style, Theme),
    after: (Kind, Style, Theme),
    fields: StyleProperties,
) -> usize {
    let old = Values::new(before.0, before.1, before.2);
    let new = Values::new(after.0, after.1, after.2);
    let mut writes = 0;
    unsafe {
        macro_rules! mask {
            (width) => {
                StyleProperties::WIDTH | StyleProperties::WIDTH_PERCENT
            };
            (height) => {
                StyleProperties::HEIGHT | StyleProperties::HEIGHT_PERCENT
            };
            (pad_x) => {
                StyleProperties::PADDING | StyleProperties::PADDING_X
            };
            (pad_y) => {
                StyleProperties::PADDING | StyleProperties::PADDING_Y
            };
            (gap) => {
                StyleProperties::GAP
            };
            (radius) => {
                StyleProperties::RADIUS
            };
            (border_width) => {
                StyleProperties::BORDER_WIDTH
            };
            (border_color) => {
                StyleProperties::BORDER_COLOR
            };
            (text_color) => {
                StyleProperties::FOREGROUND
            };
            (font24) => {
                StyleProperties::FONT_SIZE
            };
            (bg) => {
                StyleProperties::BACKGROUND
            };
            (bg_opa) => {
                StyleProperties::BACKGROUND | StyleProperties::BACKGROUND_ALPHA
            };
            (opa) => {
                StyleProperties::OPACITY
            };
            (layout) => {
                StyleProperties::LAYOUT
            };
            (flow) => {
                StyleProperties::LAYOUT
            };
            (main) => {
                StyleProperties::JUSTIFY
            };
            (cross) => {
                StyleProperties::ALIGN
            };
            (disabled) => {
                StyleProperties::DISABLED
            };
        }
        macro_rules! set {
            ($field:ident, $count:expr, $body:expr) => {
                if fields.intersects(mask!($field)) && old.$field != new.$field {
                    $body;
                    writes += $count;
                }
            };
        }
        set!(width, 1, sys::lv_obj_set_width(obj, new.width));
        set!(height, 1, sys::lv_obj_set_height(obj, new.height));
        set!(pad_x, 2, {
            sys::lv_obj_set_style_pad_left(obj, new.pad_x, 0);
            sys::lv_obj_set_style_pad_right(obj, new.pad_x, 0);
        });
        set!(pad_y, 2, {
            sys::lv_obj_set_style_pad_top(obj, new.pad_y, 0);
            sys::lv_obj_set_style_pad_bottom(obj, new.pad_y, 0);
        });
        set!(gap, 2, {
            sys::lv_obj_set_style_pad_row(obj, new.gap, 0);
            sys::lv_obj_set_style_pad_column(obj, new.gap, 0);
        });
        set!(radius, 1, sys::lv_obj_set_style_radius(obj, new.radius, 0));
        set!(
            border_width,
            1,
            sys::lv_obj_set_style_border_width(obj, new.border_width, 0)
        );
        set!(
            border_color,
            1,
            sys::lv_obj_set_style_border_color(obj, color(new.border_color), 0)
        );
        set!(
            text_color,
            1,
            sys::lv_obj_set_style_text_color(obj, color(new.text_color), 0)
        );
        set!(
            font24,
            1,
            sys::lv_obj_set_style_text_font(
                obj,
                if new.font24 {
                    &sys::lv_font_montserrat_24
                } else {
                    &sys::lv_font_montserrat_16
                },
                0
            )
        );
        set!(bg, 1, sys::lv_obj_set_style_bg_color(obj, color(new.bg), 0));
        set!(bg_opa, 1, sys::lv_obj_set_style_bg_opa(obj, new.bg_opa, 0));
        set!(opa, 1, sys::lv_obj_set_style_opa(obj, new.opa, 0));
        set!(layout, 1, sys::lv_obj_set_style_layout(obj, new.layout, 0));
        set!(
            flow,
            1,
            sys::lv_obj_set_style_flex_flow(obj, new.flow as _, 0)
        );
        set!(
            main,
            1,
            sys::lv_obj_set_style_flex_main_place(obj, new.main as _, 0)
        );
        set!(cross, 2, {
            sys::lv_obj_set_style_flex_cross_place(obj, new.cross as _, 0);
            sys::lv_obj_set_style_flex_track_place(obj, new.cross as _, 0);
        });
        set!(disabled, 1, {
            if new.disabled {
                sys::lv_obj_add_state(obj, sys::lv_state_t_LV_STATE_DISABLED);
            } else {
                sys::lv_obj_remove_state(obj, sys::lv_state_t_LV_STATE_DISABLED);
            }
        });
    }
    writes
}
