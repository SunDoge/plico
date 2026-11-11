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
    // As MyGO's collapsible panels: the content keeps its expanded width and
    // the animated edge clips it, so nothing reflows or squeezes. Labels fade
    // over the last 50 px of travel, and the toggle rides the moving edge
    // instead of jumping between header slots.
    let labelled = width > 100.;
    let reveal = ((width - 100.) / 50.).clamp(0., 1.);
    // The title waits until it clears the toggle, then fades in after the logo.
    let titled = width > 150.;
    let title_reveal = ((width - 150.) / 30.).clamp(0., 1.);
    column(cx).position(Position::Relative).width(width).shrink(0.).clip(true).background(Color::Panel) + |cx| {
        column(cx).width(224.).grow(1.).shrink(0.).padding(10).gap(12) + |cx| {
            row(cx).height(36.).align(Align::Center).gap(8) + |cx| {
                if labelled {
                    svg(cx, &LOGO).size(24., 24.).opacity(reveal);
                    if titled {
                        text(cx, "plico").font_size(20).weight(650).grow(1.).ellipsis(true).min_width(0.)
                            .opacity(title_reveal);
                    }
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
                            Page::ComponentKit,
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
                            Page::Effects,
                            Page::ShaderPlayground,
                            Page::Images,
                            Page::Markdown,
                            Page::Game2048,
                            Page::Mines,
                        ][..],
                    ),
                ] {
                    cx.keyed(section, |cx| {
                        column(cx).gap(2) + |cx| {
                            if labelled {
                                text(cx, l.text(section, title)).font_size(12).weight(600)
                                    .color(Color::Muted).padding_x(8).padding_y(4).opacity(reveal);
                            }
                            for &route in routes {
                                cx.keyed(route.id(), |cx| {
                                    let label = cx.tr(route.id(), route.label());
                                    if entry(cx, &label, app.router.show(route), route_icon(route),
                                        labelled.then_some(reveal)).clicked()
                                    {
                                        app.router.go(route);
                                    }
                                });
                            }
                        };
                    });
                }
            };
            separator(cx);
            if labelled {
                row(cx).gap(4).opacity(reveal) + |cx| {
                    for (language, label) in [
                        (crate::i18n::Language::English, "English"),
                        (crate::i18n::Language::Chinese, "中文"),
                    ] {
                        cx.keyed(label, |cx| {
                            choice(cx, label, &mut app.language, language);
                        });
                    }
                };
                checkbox(cx, l.text("compact", "Compact"), &mut app.compact).opacity(reveal);
            }
        };
        // Pin the handle to the panel's animated edge so it slides with the
        // width instead of switching header slots at an arbitrary threshold.
        row(cx).position(Position::Absolute).inset_right(10.).inset_top(10.).height(36.).align(Align::Center) + |cx| {
            if button(cx)
                .icon(if collapsed { &plico::icons::CHEVRON_RIGHT } else { &plico::icons::CHEVRON_LEFT })
                .aria_label("Toggle navigation").ghost().clicked()
            {
                app.sidebar_collapsed = !collapsed;
            }
        };
    };
}

#[component]
#[rustfmt::skip]
fn entry(
    cx: &mut UiContext,
    label: &str,
    selected: bool,
    icon_asset: &SvgAsset,
    reveal: Option<f32>,
) -> Response {
    let m = cx.theme().metrics;
    button_base(cx, label).width_percent(100.).height(m.control_height as f32).min_width(0.).padding_x(8).gap(8)
        .radius(6.).selected(selected).background(if selected { Color::Selection } else { Color::Panel })
        .hover_background(Color::ControlHover).pressed_background(Color::ControlPressed)
        .color(if selected { Color::Accent } else { Color::Foreground }).transition(0).children(|cx| {
        icon(cx, icon_asset).size(18., 18.);
        if let Some(reveal) = reveal {
            text(cx, label).font_size(m.font_size).weight(if selected { 600 } else { 400 }).ellipsis(true)
                .min_width(0.).opacity(reveal);
        }
    }).response
}

fn route_icon(page: Page) -> &'static SvgAsset {
    match page {
        Page::Workspace => &icons::FOLDER,
        Page::Layout | Page::Game2048 | Page::Mines => &icons::LAYOUT_GRID,
        Page::Controls | Page::Localization => &icons::SLIDERS_HORIZONTAL,
        Page::Lists | Page::DataTable | Page::TreeView => &icons::LIST,
        Page::Calendar => &icons::LAYOUT_GRID,
        Page::Patterns | Page::ComponentKit => &icons::LAYERS,
        Page::Effects | Page::ShaderPlayground => &icons::SPARKLES,
        Page::Motion | Page::Glass | Page::BlackHole => &icons::SPARKLES,
        Page::Images => &icons::LAYERS,
        Page::Markdown => &icons::MESSAGE_SQUARE,
    }
}
