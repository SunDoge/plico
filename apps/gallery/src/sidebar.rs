//! A grouped desktop source list: subdued icons, compact rows and one selected route.
use crate::{Gallery, LOGO, Page, icons};
use plico::{Route, prelude::*};

#[component]
#[rustfmt::skip]
pub fn show(cx: &mut UiContext, app: &mut Gallery) {
    let l = cx.localizer();
    let collapsed = app.sidebar_collapsed;
    let now = cx.now();
    let width = cx.transient_state("sidebar.width", || {
        std::cell::RefCell::new(plico_motion::Animated::new(if collapsed { 64. } else { 224. }))
    });
    let (width, active) = {
        let state = width.read();
        let mut motion = state.borrow_mut();
        motion.retarget(now, if collapsed { 64. } else { 224. }, std::time::Duration::from_millis(240),
            plico_motion::Easing::EaseOut);
        (motion.value(now) as f32, motion.active(now))
    };
    if active {
        cx.request_animation_frame();
    }
    column(cx).width(width).shrink(0.).padding(10).gap(12).clip(true).background(Color::Panel) + |cx| {
        row(cx).height(36.).align(Align::Center).gap(8) + |cx| {
            if !collapsed {
                svg(cx, &LOGO).size(24., 24.);
                text(cx, "plico").font_size(20).weight(650).grow(1.);
            }
            if button(cx)
                .icon(if collapsed { &plico::icons::CHEVRON_RIGHT } else { &plico::icons::CHEVRON_LEFT })
                .aria_label("Toggle navigation").ghost().clicked()
            {
                app.sidebar_collapsed = !collapsed;
            }
        };
        column(cx).height(0.).grow(1.).scroll_y(true).gap(16).min_width(0.) + |cx| {
            for (section, title, routes) in [
                ("sidebar-workspace", "Workspace", &[Page::Workspace][..]),
                (
                    "sidebar-components",
                    "Components",
                    &[
                        Page::Layout,
                        Page::Controls,
                        Page::Lists,
                        Page::DataTable,
                        Page::TreeView,
                        Page::Calendar,
                        Page::Localization,
                        Page::Patterns,
                    ][..],
                ),
                (
                    "sidebar-experiments",
                    "Experiments",
                    &[
                        Page::Motion,
                        Page::Glass,
                        Page::BlackHole,
                        Page::Images,
                        Page::Markdown,
                        Page::Game2048,
                    ][..],
                ),
            ] {
                cx.keyed(section, |cx| {
                    column(cx).gap(2) + |cx| {
                        if !collapsed {
                            text(cx, l.text(section, title)).font_size(12).weight(600).color(Color::Muted)
                                .padding_x(8).padding_y(4);
                        }
                        for &route in routes {
                            cx.keyed(route.id(), |cx| {
                                let label = cx.tr(route.id(), route.label());
                                if entry(cx, &label, app.router.show(route), route_icon(route), collapsed).clicked() {
                                    app.router.go(route);
                                }
                            });
                        }
                    };
                });
            }
        };
        separator(cx);
        if !collapsed {
            row(cx).gap(4) + |cx| {
                for (language, label) in [
                    (crate::i18n::Language::English, "English"),
                    (crate::i18n::Language::Chinese, "中文"),
                ] {
                    cx.keyed(label, |cx| {
                        choice(cx, label, &mut app.language, language);
                    });
                }
            };
            checkbox(cx, l.text("compact", "Compact"), &mut app.compact);
        }
    };
}

#[component]
#[rustfmt::skip]
fn entry(
    cx: &mut UiContext,
    label: &str,
    selected: bool,
    icon_asset: &SvgAsset,
    collapsed: bool,
) -> Response {
    let m = cx.theme().metrics;
    button_base(cx, label).width_percent(100.).height(m.control_height as f32).min_width(0.).padding_x(8).gap(8)
        .radius(6.).selected(selected).background(if selected { Color::Selection } else { Color::Panel })
        .hover_background(Color::ControlHover).pressed_background(Color::ControlPressed)
        .color(if selected { Color::Accent } else { Color::Foreground }).transition(0).children(|cx| {
        icon(cx, icon_asset).size(18., 18.);
        if !collapsed {
            text(cx, label).font_size(m.font_size).weight(if selected { 600 } else { 400 }).ellipsis(true)
                .min_width(0.);
        }
    }).response
}

fn route_icon(page: Page) -> &'static SvgAsset {
    match page {
        Page::Workspace => &icons::FOLDER,
        Page::Layout | Page::Game2048 => &icons::LAYOUT_GRID,
        Page::Controls | Page::Localization => &icons::SLIDERS_HORIZONTAL,
        Page::Lists | Page::DataTable | Page::TreeView => &icons::LIST,
        Page::Calendar => &icons::LAYOUT_GRID,
        Page::Patterns => &icons::LAYERS,
        Page::Effects => &icons::SPARKLES,
        Page::Motion | Page::Glass | Page::BlackHole => &icons::SPARKLES,
        Page::Images => &icons::LAYERS,
        Page::Markdown => &icons::MESSAGE_SQUARE,
    }
}
