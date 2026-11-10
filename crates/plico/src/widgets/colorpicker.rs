//! A color picker ported from MyGO's `ui/colorpicker.go`.
//!
//! Plico's [`Color`] is a theme token or an RGB literal without alpha, so the
//! opacity channel is the only MyGO control that cannot be represented.
use crate::WidgetRole;
use crate::prelude::*;
use crate::{ShaderAsset, fr, text};

static SATURATION_VALUE: ShaderAsset = ShaderAsset::new(
    r#"fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
        let hue = plico.values[0].xyz;
        let saturated = mix(vec3<f32>(1.0), hue, uv.x);
        return vec4<f32>(mix(saturated, vec3<f32>(0.0), uv.y), 1.0);
    }"#,
);
static HUE_TRACK: ShaderAsset = ShaderAsset::new(
    r#"fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
        let h = uv.x * 6.0;
        let x = 1.0 - abs((h % 2.0) - 1.0);
        var rgb = vec3<f32>(1.0, x, 0.0);
        if h >= 1.0 && h < 2.0 { rgb = vec3<f32>(x, 1.0, 0.0); }
        if h >= 2.0 && h < 3.0 { rgb = vec3<f32>(0.0, 1.0, x); }
        if h >= 3.0 && h < 4.0 { rgb = vec3<f32>(0.0, x, 1.0); }
        if h >= 4.0 && h < 5.0 { rgb = vec3<f32>(x, 0.0, 1.0); }
        if h >= 5.0 { rgb = vec3<f32>(1.0, 0.0, x); }
        return vec4<f32>(rgb, 1.0);
    }"#,
);

/// The colors a picker offers, named for assistive technology (MyGO's
/// swatch list).
const SWATCHES: [(&str, u32); 12] = [
    ("Red", 0xef4444),
    ("Orange", 0xf97316),
    ("Yellow", 0xeab308),
    ("Green", 0x22c55e),
    ("Teal", 0x14b8a6),
    ("Blue", 0x3b82f6),
    ("Indigo", 0x6366f1),
    ("Purple", 0xa855f7),
    ("Pink", 0xec4899),
    ("Brown", 0x92400e),
    ("Gray", 0x71717a),
    ("Black", 0x000000),
];

/// A color as hue (0..360), saturation and brightness (0..1). MyGO keeps the
/// hue of grays so the picker does not forget it; this type does the same.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Hsva {
    h: f64,
    s: f64,
    v: f64,
}

impl Hsva {
    fn from_rgb(rgb: u32) -> Self {
        let r = f64::from((rgb >> 16) & 0xff) / 255.;
        let g = f64::from((rgb >> 8) & 0xff) / 255.;
        let b = f64::from(rgb & 0xff) / 255.;
        let high = r.max(g).max(b);
        let low = r.min(g).min(b);
        let delta = high - low;
        let mut hue = if delta == 0. {
            0.
        } else if high == r {
            60. * ((g - b) / delta).rem_euclid(6.)
        } else if high == g {
            60. * ((b - r) / delta + 2.)
        } else {
            60. * ((r - g) / delta + 4.)
        };
        if hue < 0. {
            hue += 360.;
        }
        let saturation = if high > 0. { delta / high } else { 0. };
        Self {
            h: hue,
            s: saturation,
            v: high,
        }
    }

    fn rgb(self) -> u32 {
        let chroma = self.v * self.s;
        let h = self.h.rem_euclid(360.) / 60.;
        let x = chroma * (1. - (h.rem_euclid(2.) - 1.).abs());
        let (r, g, b) = match h {
            h if h < 1. => (chroma, x, 0.),
            h if h < 2. => (x, chroma, 0.),
            h if h < 3. => (0., chroma, x),
            h if h < 4. => (0., x, chroma),
            h if h < 5. => (x, 0., chroma),
            _ => (chroma, 0., x),
        };
        let m = self.v - chroma;
        channel(r + m) << 16 | channel(g + m) << 8 | channel(b + m)
    }
}

fn channel(value: f64) -> u32 {
    (value.clamp(0., 1.) * 255.).round() as u32
}

/// The picker's transient memory per declaration: its hue while the color is
/// a gray, and the color it last set or saw. It never enters checkpoints.
#[derive(Clone, Debug, Default, PartialEq)]
struct PickerEntry {
    hsv: Hsva,
    last: u32,
    hex: String,
}

fn hex_of(rgb: u32) -> String {
    format!("#{rgb:06x}")
}

fn parse_hex(value: &str) -> Option<u32> {
    let value = value.trim().strip_prefix('#').unwrap_or(value.trim());
    match value.len() {
        3 => {
            let value = u16::from_str_radix(value, 16).ok()? as u32;
            let r = (value >> 8) & 0xf;
            let g = (value >> 4) & 0xf;
            let b = value & 0xf;
            Some((r * 17) << 16 | (g * 17) << 8 | (b * 17))
        }
        6 => u32::from_str_radix(value, 16).ok(),
        _ => None,
    }
}

fn hue_slider(cx: &mut UiContext<'_>, hue: &mut f64) -> crate::SliderResponse {
    let m = cx.theme().metrics;
    let width = m.space(60) as f32;
    let knob = m.space(3) as f32;
    row(cx)
        .width_percent(100.)
        .height(m.space(4))
        .position(crate::Position::Relative)
        .children(|cx| {
            shader(cx, &HUE_TRACK)
                .absolute(0., m.spacing as f32 * 0.5)
                .width_percent(100.)
                .height(m.space(3))
                .radius(m.spacing as f32 * 1.5)
                .corner_smoothing(0.);
            let x = knob / 2. + (width - knob) * (*hue as f32 / 360.) - knob / 2.;
            column(cx)
                .absolute(x, 0.)
                .width(knob)
                .height(m.space(4))
                .radius(knob / 2.)
                .corner_smoothing(0.)
                .background(Color::Literal(0xffffff))
                .shadow(crate::BoxShadow {
                    offset_x: 0.,
                    offset_y: 1.,
                    blur: 3.,
                    spread: 0.,
                    color: Color::Literal(0),
                    opacity: 0.3,
                })
                .children(|_cx| {});
            slider(cx, "Hue", hue, 0.0..=360.0)
                .unstyled()
                .absolute(0., 0.)
                .width_percent(100.)
                .height(m.space(4))
                .response()
        })
        .inner
}

/// A picker for an RGB [`Color`] (MyGO `ColorPicker`): a preview swatch and
/// its editable hex value, MyGO's saturation/brightness square, hue control
/// and twelve named swatch buttons. Returns whether the color changed this
/// frame.
///
/// `*color` becomes [`Color::Literal`] on the first edit; theme tokens can be
/// edited too, from the color they currently resolve to. MyGO's
/// The opacity slider is omitted because [`Color`] has no alpha channel.
///
/// ```text
/// if color_picker(cx, &mut tint) {
///     save(tint);
/// }
/// ```
#[track_caller]
pub fn color_picker(cx: &mut UiContext, color: &mut Color) -> bool {
    let m = cx.theme().metrics;
    let current = cx.theme().resolve(*color);
    let state = cx.state(|| PickerEntry {
        hsv: Hsva::from_rgb(current),
        last: current,
        hex: hex_of(current),
    });
    let mut entry = state.get();
    if entry.last != current {
        entry = PickerEntry {
            hsv: Hsva::from_rgb(current),
            last: current,
            hex: hex_of(current),
        };
    }

    let mut changed = false;
    column(cx)
        .width(m.space(60))
        .shrink(0.)
        .gap(m.spacing as f32 * 2.5)
        .children(|cx| {
            let hue = Hsva {
                h: entry.hsv.h,
                s: 1.,
                v: 1.,
            }
            .rgb();
            let hue_rgb = [
                ((hue >> 16) & 0xff) as f32 / 255.,
                ((hue >> 8) & 0xff) as f32 / 255.,
                (hue & 0xff) as f32 / 255.,
            ];
            let mut picked = None;
            let square = button_base(cx, "Saturation and brightness")
                .width_percent(100.)
                .height(m.space(40))
                .min_height(0.)
                .padding(0.)
                .position(crate::Position::Relative)
                .radius(m.radius as f32)
                .clip(true)
                .semantic_role(WidgetRole::Slider)
                .children(|cx| {
                    let pointer = cx.pointer_fraction();
                    if cx.pressed() {
                        picked = pointer;
                    }
                    shader(cx, &SATURATION_VALUE)
                        .absolute(0., 0.)
                        .width_percent(100.)
                        .height_percent(100.)
                        .radius(m.radius as f32)
                        .values(&hue_rgb);
                    let diameter = m.spacing as f32 * 3.5;
                    let x = entry.hsv.s as f32 * m.space(60) as f32 - diameter / 2.;
                    let y = (1. - entry.hsv.v as f32) * m.space(40) as f32 - diameter / 2.;
                    column(cx)
                        .absolute(x, y)
                        .width(diameter)
                        .height(diameter)
                        .radius(diameter / 2.)
                        .corner_smoothing(0.)
                        .border(2., Color::Literal(0xffffff))
                        .shadow(crate::BoxShadow {
                            offset_x: 0.,
                            offset_y: 0.,
                            blur: 0.,
                            spread: 1.,
                            color: Color::Literal(0),
                            opacity: 0.3,
                        })
                        .children(|_cx| {});
                });
            if let Some([x, y]) = picked {
                entry.hsv.s = f64::from(x.clamp(0., 1.));
                entry.hsv.v = 1. - f64::from(y.clamp(0., 1.));
                let next = entry.hsv.rgb();
                if next != current {
                    *color = Color::Literal(next);
                    entry.last = next;
                    entry.hex = hex_of(next);
                    changed = true;
                }
            }
            let _ = square.response;

            let mut hue = entry.hsv.h;
            if hue_slider(cx, &mut hue).changed {
                entry.hsv.h = hue;
                let next = entry.hsv.rgb();
                if next != cx.theme().resolve(*color) {
                    *color = Color::Literal(next);
                    entry.last = next;
                    entry.hex = hex_of(next);
                    changed = true;
                }
            }

            row(cx).gap(m.space(2)).align(Align::Center).children(|cx| {
                column(cx)
                    .width(m.space(7))
                    .height(m.space(7))
                    .radius(m.radius as f32)
                    .shrink(0.)
                    .clip(true)
                    .border(1., Color::Border)
                    .background(Color::Literal(entry.hsv.rgb()))
                    .children(|_cx| {});
                let response = cx
                    .text_input_string("Hex", &mut entry.hex)
                    .grow(1.)
                    .response();
                if response.changed
                    && let Some(next) = parse_hex(&entry.hex)
                {
                    entry.hsv = Hsva::from_rgb(next);
                    entry.last = next;
                    if next != cx.theme().resolve(*color) {
                        *color = Color::Literal(next);
                        changed = true;
                    }
                }
                if !response.focused && parse_hex(&entry.hex).is_none() {
                    entry.hex = hex_of(entry.hsv.rgb());
                }
            });

            grid(cx)
                .width_percent(100.)
                .grid_columns([fr(1.); 6])
                .gap(m.spacing as f32 * 1.5)
                .children(|cx| {
                    for (name, swatch) in SWATCHES {
                        let mut button = button_base(cx, name)
                            .width_percent(100.)
                            .height(m.space(5))
                            .min_width(0.)
                            .min_height(0.)
                            .padding(0.)
                            .radius(m.radius as f32)
                            .background(Color::Literal(swatch))
                            .hover_background(Color::Literal(swatch))
                            .pressed_background(Color::Literal(swatch));
                        if entry.hsv.rgb() == swatch {
                            button = button.border(2., Color::Foreground);
                        }
                        if button.children(|_cx| {}).response.clicked() {
                            entry.hsv = Hsva::from_rgb(swatch);
                            entry.last = swatch;
                            entry.hex = hex_of(swatch);
                            if swatch != cx.theme().resolve(*color) {
                                *color = Color::Literal(swatch);
                                changed = true;
                            }
                        }
                    }
                });
        });
    state.set(entry);
    changed
}

/// A color swatch that discloses a [`color_picker`] (MyGO `ColorWell`): the
/// button shows the color and its hex value, toggles `open` on click and
/// carries the color-well role and expanded state. While open the picker is
/// declared inline below the swatch; returns whether the picker changed the
/// color.
///
/// MyGO floats the picker in a popover below the well and exposes the hex as
/// the well's value; Plico has no custom popover trigger yet, so the picker
/// unfolds in place, and the well's accessible name is its label while the hex
/// stays visible text.
///
/// ```text
/// if color_well(cx, "Tint", &mut tint, &mut tint_open) {
///     save(tint);
/// }
/// ```
pub fn color_well(cx: &mut UiContext, label: &str, color: &mut Color, open: &mut bool) -> bool {
    let m = cx.theme().metrics;
    let current = cx.theme().resolve(*color);
    let swatch = button_base(cx, label)
        .padding(m.space(1))
        .gap(m.space(2))
        .radius(m.radius as f32)
        .background(Color::Panel)
        .hover_background(Color::ControlHover)
        .pressed_background(Color::ControlPressed)
        .expanded(*open)
        .semantic_role(WidgetRole::ColorWell)
        .children(|cx| {
            let _ = column(cx)
                .width(44.)
                .height(24.)
                .radius(4.)
                .shrink(0.)
                .background(Color::Literal(current))
                .border(1., Color::Border)
                .children(|_cx| {});
            let _ = text!(cx, "#{current:06x}")
                .font_size(m.font_size)
                .color(Color::Muted);
        })
        .response;
    if swatch.clicked() {
        *open = !*open;
    }
    if *open {
        color_picker(cx, color)
    } else {
        false
    }
}
