//! Rust use of the shared component kit; no managed runtime or copied controls.
use plico::prelude::*;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ComponentKit {
    pub page: usize,
    pub pinned: bool,
    pub notice: bool,
    pub tag: bool,
    pub animated: bool,
    pub cleared: bool,
    pub section: usize,
    pub smoothing: f64,
}
impl Default for ComponentKit {
    fn default() -> Self {
        Self {
            page: 1,
            pinned: false,
            notice: true,
            tag: true,
            animated: false,
            cleared: false,
            section: 0,
            smoothing: 1.,
        }
    }
}
#[component]
#[rustfmt::skip]
pub fn page(cx: &mut UiContext, state: &mut ComponentKit) {
    crate::heading(cx, "Component kit", "Feedback, navigation and loading states, built from reusable components.");
    tabs(cx, &mut state.section, &["Surfaces & feedback", "Corners"]);
    if state.section == 1 {
        corners(cx, &mut state.smoothing);
        return;
    }
    column(cx).width_percent(100.).height(0.).grow(1.).scroll_y(true).padding_right(24).gap(16) + |cx| {
        row(cx).gap(12).align(Align::Center) + |cx| {
            if button(cx).text("Reset examples").ghost().clicked() {
                *state = ComponentKit::default();
            }
            switch(cx, "Animate placeholders", &mut state.animated);
        };
        row(cx).width_percent(100.).gap(16).wrap(true).align(Align::Start) + |cx| {
            column(cx).width(0.).grow(1.).min_width(320.).gap(16) + |cx| {
                panel(cx, Some("Feedback")).description(
                    "State and recovery actions stay with the application.") + |cx|
                {
                    alert(cx, "Ready to work", "Vector icons and semantic tones follow the active theme.",
                        Tone::Info);
                    notification(cx, "Your changes were saved.", &mut state.notice, Tone::Success);
                    row(cx).gap(8).align(Align::Center) + |cx| {
                        badge_tone(cx, "Synced", Tone::Success);
                        chip(cx, "Pinned", &mut state.pinned);
                        if state.tag && tag(cx, "Rust") {
                            state.tag = false;
                        }
                    };
                    skeleton(cx, 180., 16., state.animated);
                };
                panel(cx, Some("Pagination")).description(
                    "At most five numeric pages, even with a billion pages.") + |cx|
                {
                    pagination(cx, &mut state.page, 1_000_000_000);
                    text!(cx, "Page {} of 1,000,000,000", state.page).color(Color::Muted);
                };
            };
            column(cx).width(0.).grow(1.).min_width(320.).gap(16) + |cx| {
                panel(cx, Some("Empty state")) + |cx| {
                    if state.cleared {
                        text(cx, "Filters cleared").color(Color::Success);
                    } else if empty_state(cx, "No matching items",
                        "Change the search or clear the current filters.", Some("Clear filters")) {
                        state.cleared = true;
                    }
                };
                column(cx).position(Position::Relative).width_percent(100.).height(260.).clip(true)
                    .radius(18.).corner_smoothing(1.).background(Color::Panel) + |cx|
                {
                    column(cx).width_percent(100.).height_percent(100.).scroll_y(true).padding(12)
                        .padding_top(64).gap(8) + |cx|
                    {
                        for item in 0..12 {
                            cx.keyed(item as u64, |cx| {
                                column(cx).width_percent(100.).height(48.).padding(12).radius(10.)
                                    .background(Color::Literal(
                                        if item % 2 == 0 { 0xc4def0 } else { 0xf3d9bd })) + |cx|
                                {
                                    text!(cx, "Scroll item {}", item + 1)
                                        .color(Color::Literal(0x243443));
                                };
                            });
                        }
                    };
                    scroll_edge(
                        cx,
                        ScrollEdgeSide::Top,
                        BackdropBlur {
                            radius: 8.,
                            tint: Color::Panel,
                            tint_opacity: 0.8,
                        },
                    ).absolute(0., 0.).width_percent(100.).height(80.);
                    text(cx, "Progressive scroll edge").font_size(18).weight(650).absolute(16., 16.);
                };
            };
        };
    };
}

#[component]
#[rustfmt::skip]
fn corners(cx: &mut UiContext, smoothing: &mut f64) {
    column(cx).width_percent(100.).height(0.).grow(1.).scroll_y(true).padding_right(24) + |cx| {
        panel(cx, Some("Continuous corners"))
            .description("The same radius, with a gentler transition into straight edges.") + |cx|
        {
            slider(cx, "Corner smoothing", smoothing, 0.0..=1.0).width(260.);
            text!(cx, "Smoothing: {:.2}", *smoothing).color(Color::Muted);
            for example in ["Filled surface", "Border outline", "Clipped artwork"] {
                cx.keyed(example, |cx| {
                    text(cx, example).weight(650);
                    row(cx).width_percent(100.).gap(24).wrap(true) + |cx| {
                        for (name, amount) in
                            [("Circular", 0.), ("Continuous", *smoothing as f32)]
                        {
                            cx.keyed(name, |cx| {
                                column(cx).gap(10).shrink(0.) + |cx| {
                                    let mut surface = column(cx).position(Position::Relative)
                                        .width(300.).height(160.).radius(28.).corner_smoothing(amount)
                                        .align(Align::Center).justify(Justify::Center);
                                    if example == "Border outline" {
                                        surface = surface.border(2., Color::Accent);
                                    } else if example == "Filled surface" {
                                        surface = surface.background(Color::Accent);
                                    } else {
                                        surface = surface.clip(true)
                                            .background(Color::Literal(0xc4def0));
                                    }
                                    surface + |cx| {
                                        if example == "Clipped artwork" {
                                            column(cx).absolute(-30., -60.).width(200.).height(200.)
                                                .radius(100.).background(Color::Literal(0xf3b693)).end();
                                            column(cx).absolute(130., 40.).width(220.).height(220.)
                                                .radius(110.).background(Color::Literal(0x74b9d9)).end();
                                        }
                                    };
                                    text!(cx, "{} · radius 28", name).color(Color::Muted);
                                };
                            });
                        }
                    };
                });
            }
        };
    };
}
