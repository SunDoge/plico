//! Rust counterparts of the MyGO-inspired managed component kit.
//! Application state, sorting, IO and service policy remain with the caller.
use crate::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Neutral,
    Info,
    Success,
    Warning,
    Danger,
}
impl Tone {
    pub const fn color(self) -> Color {
        match self {
            Self::Neutral => Color::Muted,
            Self::Info => Color::Info,
            Self::Success => Color::Success,
            Self::Warning => Color::Warning,
            Self::Danger => Color::Danger,
        }
    }
}
/// A status badge with an explicit semantic tone; `badge` remains available.
#[component]
pub fn badge_tone(cx: &mut UiContext, label: &str, tone: Tone) {
    let m = cx.theme().metrics;
    text(cx, label)
        .font_size(m.label_size)
        .weight(600)
        .padding_x(m.space(2))
        .padding_y(m.space(1))
        .radius(m.radius as f32)
        .background(Color::Control)
        .color(tone.color());
}
/// A native button chip; returns true when its caller-owned selection changes.
#[component]
pub fn chip(cx: &mut UiContext, label: &str, selected: &mut bool) -> bool {
    let control = button(cx).text(label).radius(18.).height(32.);
    let control = if *selected {
        control.primary()
    } else {
        control.ghost().border(1., Color::Border)
    };
    if control.clicked() {
        *selected = !*selected;
        true
    } else {
        false
    }
}
/// A removable tag; returns true for a removal request. Caller removes its data.
#[component]
pub fn tag(cx: &mut UiContext, label: &str) -> bool {
    row(cx)
        .gap(6)
        .align(Align::Center)
        .padding_x(8)
        .radius(6.)
        .background(Color::Control)
        .border(1., Color::Border)
        .children(|cx| {
            text(cx, label);
            (button_base(cx, format!("Remove {label}"))
                .width(28.)
                .height(28.)
                .padding(6.)
                .radius(5.)
                .hover_background(Color::ControlHover)
                + |cx| {
                    svg(cx, &crate::icons::X).size(14., 14.).color(Color::Muted);
                })
            .response
            .clicked()
        })
        .inner
}
/// Bounded one-based pagination; at most five numeric buttons even for usize::MAX.
#[component]
pub fn pagination(cx: &mut UiContext, page: &mut usize, page_count: usize) -> bool {
    assert!(page_count > 0, "pagination requires at least one page");
    *page = (*page).clamp(1, page_count);
    let before = *page;
    let mut pages = [
        1,
        before.saturating_sub(1).max(1),
        before,
        before.saturating_add(1).min(page_count),
        page_count,
    ];
    pages.sort_unstable();
    row(cx)
        .gap(4)
        .wrap(true)
        .align(Align::Center)
        .children(|cx| {
            if button(cx)
                .text("Previous")
                .aria_label("Previous page")
                .ghost()
                .disabled(before == 1)
                .clicked()
                && before > 1
            {
                *page = before - 1;
            }
            let mut previous = 0;
            for value in pages {
                if value == previous {
                    continue;
                }
                cx.keyed(value, |cx| {
                    if previous != 0 && value - previous > 1 {
                        text(cx, "…").color(Color::Muted);
                    }
                    let control = button(cx)
                        .text(value.to_string())
                        .aria_label(format!("Page {value}"));
                    let control = if before == value {
                        control.primary()
                    } else {
                        control.ghost()
                    };
                    if control.clicked() {
                        *page = value;
                    }
                });
                previous = value;
            }
            if button(cx)
                .text("Next")
                .aria_label("Next page")
                .ghost()
                .disabled(before == page_count)
                .clicked()
                && before < page_count
            {
                *page = before + 1;
            }
        });
    *page != before
}
static INFO: SvgAsset = SvgAsset::new(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><circle cx="12" cy="12" r="9"/><path d="M12 10v7M12 6v2"/></svg>"#,
);
/// An inline title/message with a vector indicator. No dismissal or service policy.
#[component]
pub fn alert(cx: &mut UiContext, title: &str, message: &str, tone: Tone) {
    row(cx)
        .width_percent(100.)
        .shrink(0.)
        .gap(12)
        .padding(14)
        .radius(8.)
        .background(Color::Control)
        .border(1., Color::Border)
        .align(Align::Start)
        .children(|cx| {
            svg(cx, &INFO).size(20., 20.).color(tone.color());
            column(cx)
                .width(0.)
                .grow(1.)
                .min_width(0.)
                .gap(5)
                .children(|cx| {
                    text(cx, title).weight(650).width_percent(100.);
                    text(cx, message).color(Color::Muted).width_percent(100.);
                });
        });
}
/// Caller-owned inline dismissible feedback. Returns true only when dismissed.
#[component]
pub fn notification(cx: &mut UiContext, message: &str, visible: &mut bool, tone: Tone) -> bool {
    if !*visible {
        return false;
    }
    let dismissed = row(cx)
        .width_percent(100.)
        .shrink(0.)
        .padding(12)
        .gap(12)
        .radius(8.)
        .background(Color::Control)
        .border(1., Color::Border)
        .align(Align::Center)
        .children(|cx| {
            text(cx, message)
                .width(0.)
                .grow(1.)
                .min_width(0.)
                .color(tone.color());
            (button_base(cx, "Dismiss notification")
                .width(28.)
                .height(28.)
                .padding(6.)
                .radius(5.)
                .hover_background(Color::ControlHover)
                + |cx| {
                    svg(cx, &crate::icons::X).size(14., 14.).color(Color::Muted);
                })
            .response
            .clicked()
        })
        .inner;
    if dismissed {
        *visible = false;
    }
    dismissed
}
/// Centered empty-state content. Returns true for the optional action button.
#[component]
pub fn empty_state(
    cx: &mut UiContext,
    title: &str,
    description: &str,
    action: Option<&str>,
) -> bool {
    let font_size = cx.theme().metrics.font_size;
    column(cx)
        .width_percent(100.)
        .padding(28)
        .gap(12)
        .align(Align::Center)
        .children(|cx| {
            text(cx, title).font_size(font_size + 4).weight(650);
            text(cx, description)
                .width_percent(100.)
                .color(Color::Muted);
            action.is_some_and(|label| button(cx).text(label).primary().clicked())
        })
        .inner
}
static SKELETON: crate::ShaderAsset = crate::shader_asset!("../../assets/materials/skeleton.wgsl");
/// Native-clock shimmer; `animated=false` is a static reduced-motion placeholder.
/// Requires a WGSL-capable backend and never reads the system clock in a component.
pub fn skeleton<'u, 'e>(
    cx: &'u mut UiContext<'e>,
    width: f32,
    height: f32,
    animated: bool,
) -> crate::Shader<'u, 'e, 'static> {
    assert!(
        width.is_finite()
            && height.is_finite()
            && width > 0.
            && height > 0.
            && width <= 16384.
            && height <= 16384.,
        "invalid skeleton size"
    );
    let color = cx.theme().resolve(Color::Border);
    shader(cx, &SKELETON)
        .width(width)
        .height(height)
        .radius(4.)
        .animated(animated)
        .values(&[
            ((color >> 16) & 255) as f32 / 255.,
            ((color >> 8) & 255) as f32 / 255.,
            (color & 255) as f32 / 255.,
            f32::from(animated),
        ])
}
