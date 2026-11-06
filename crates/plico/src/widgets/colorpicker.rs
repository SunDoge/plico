//! A color picker ported from MyGO's `ui/colorpicker.go`.
//!
//! Plico's [`Color`] is a theme token or an RGB literal without alpha, so the
//! port edits an RGB literal through hue, saturation and brightness sliders
//! instead of MyGO's saturation/brightness square, and leaves out the opacity
//! slider and hex editing.
use crate::WidgetRole;
use crate::prelude::*;
use crate::text;

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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PickerEntry {
    hsv: Hsva,
    last: u32,
}

/// A picker for an RGB [`Color`] (MyGO `ColorPicker`): a preview swatch and
/// its hex value, Hue/Saturation/Brightness sliders and twelve named swatch
/// buttons. Returns whether the color changed this frame.
///
/// `*color` becomes [`Color::Literal`] on the first edit; theme tokens can be
/// edited too, from the color they currently resolve to. MyGO's
/// saturation/brightness square, opacity slider and editable hex field are not
/// ported: Plico has no local pointer coordinates for a custom-painted square
/// and no alpha channel on [`Color`], so the port uses three sliders and a
/// read-only hex readout.
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
    });
    let mut entry = state.get();
    if entry.last != current {
        entry = PickerEntry {
            hsv: Hsva::from_rgb(current),
            last: current,
        };
    }

    let before = entry.hsv;
    let mut hue = entry.hsv.h;
    let mut saturation = entry.hsv.s * 100.;
    let mut brightness = entry.hsv.v * 100.;
    let _ = slider(cx, "Hue", &mut hue, 0.0..=360.0).response();
    entry.hsv.h = hue;
    let _ = slider(cx, "Saturation", &mut saturation, 0.0..=100.0).response();
    entry.hsv.s = saturation / 100.;
    let _ = slider(cx, "Brightness", &mut brightness, 0.0..=100.0).response();
    entry.hsv.v = brightness / 100.;

    let shown = entry.hsv.rgb();
    row(cx).gap(m.space(2)).align(Align::Center).children(|cx| {
        let _ = column(cx)
            .width(28.)
            .height(28.)
            .radius(m.radius as f32)
            .shrink(0.)
            .border(1., Color::Border)
            .background(Color::Literal(shown))
            .children(|_cx| {});
        text!(cx, "#{shown:06x}")
            .font_size(m.font_size)
            .color(Color::Muted);
    });

    row(cx).gap(m.space(1)).wrap(true).children(|cx| {
        for (name, swatch) in SWATCHES {
            let mut button = button_base(cx, name)
                .width(24.)
                .height(24.)
                .min_width(0.)
                .min_height(0.)
                .padding(0.)
                .radius(4.)
                .background(Color::Literal(swatch))
                .hover_background(Color::Literal(swatch))
                .pressed_background(Color::Literal(swatch));
            button = if shown == swatch {
                button.border(2., Color::Foreground)
            } else {
                button.border(1., Color::Border)
            };
            if button.children(|_cx| {}).response.clicked() {
                entry.hsv = Hsva::from_rgb(swatch);
            }
        }
    });

    let mut changed = false;
    let next = entry.hsv.rgb();
    // Slider normalization and an external reset do not edit; only a user
    // move or swatch click (which changes the HSV state) does. Moving the
    // hue of a gray leaves the RGB color alone but keeps the hue in the
    // transient state for the next edit.
    if entry.hsv != before && next != current {
        *color = Color::Literal(next);
        entry.last = next;
        changed = true;
    }
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
