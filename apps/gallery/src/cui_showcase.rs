//! A dense CI workspace based on CUI's README component showcase.
//!
//! The composition and visual proportions follow CUI revision
//! 6e6b35326f655405822fcf863fac0d3bb35bb787. The implementation only uses
//! Plico's public widget and layout API, so this page doubles as a composition
//! and deterministic-capture test rather than importing CUI code.

use plico::prelude::*;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CuiShowcase {
    pub platform: usize,
    pub runner_name: String,
    pub address: String,
    pub token_visible: bool,
    pub build_fails: bool,
    pub build_recovers: bool,
    pub nightly_report: bool,
    pub selected_day: usize,
    pub selected_slot: usize,
    pub release_url: String,
    pub release_access: usize,
    pub coverage_floor: f64,
    pub matrix: [bool; 6],
}

impl Default for CuiShowcase {
    fn default() -> Self {
        Self {
            platform: 0,
            runner_name: "linux-arm64-02".into(),
            address: "ssh://10.0.4.17:22".into(),
            token_visible: false,
            build_fails: true,
            build_recovers: false,
            nightly_report: true,
            selected_day: 17,
            selected_slot: 1,
            release_url: "https://kiln.app/r/2.4.0-rc.2".into(),
            release_access: 1,
            coverage_floor: 80.,
            matrix: [true, false, true, true, false, false],
        }
    }
}

#[component]
#[rustfmt::skip]
pub fn page(cx: &mut UiContext, state: &mut CuiShowcase) {
    column(cx).width_percent(100.).height(0.).grow(1.).scroll_y(true).padding_right(24).gap(16) + |cx| {
        column(cx).gap(5) + |cx| {
            text(cx, "Release workspace").font_size(28).weight(700);
            text(cx, "A dense, interactive composition of Plico controls based on CUI's component showcase.")
                .color(Color::Muted);
        };
        row(cx).width_percent(100.).gap(16).wrap(true).align(Align::Start) + |cx| {
            column(cx).width(300.).grow(1.).min_width(280.).gap(16) + |cx| {
                runner_card(cx, state);
                notification_card(cx, state);
                cache_card(cx);
            };
            column(cx).width(420.).grow(1.35).min_width(360.).gap(16) + |cx| {
                metric_cards(cx);
                schedule_card(cx, state);
                build_chart(cx);
            };
            column(cx).width(320.).grow(1.).min_width(300.).gap(16) + |cx| {
                reviewers_card(cx);
                release_card(cx, state);
                coverage_card(cx, state);
                matrix_card(cx, state);
            };
        };
        text(
            cx,
            "Composition adapted from CUI · MIT license · public Plico widgets only",
        ).font_size(11).color(Color::Muted).padding_bottom(8);
    };
}

#[component]
#[rustfmt::skip]
fn runner_card(cx: &mut UiContext, state: &mut CuiShowcase) {
    panel(cx, Some("Add a runner")).description("Runners pick up jobs for the platforms you choose.") + |cx| {
        segmented(cx, &mut state.platform, &["Linux", "macOS", "Windows"]);
        field(cx, "Name") + |cx| {
            text_input_string(cx, "Runner name", &mut state.runner_name).width_percent(100.);
        };
        field(cx, "Address") + |cx| {
            text_input_string(cx, "Runner address", &mut state.address).width_percent(100.);
        };
        field(cx, "Registration token").description("Shown once. Store it with the runner.") + |cx| {
            let metrics = cx.theme().metrics;
            row(cx).width_percent(100.).gap(4).align(Align::Center) + |cx| {
                text(
                    cx,
                    if state.token_visible { "ck_live_A7y3QZx92M" } else { "••••••••••••••••••" },
                ).height(metrics.control_height as f32).padding_x(12).grow(1.).radius(metrics.radius as f32)
                    .background(Color::Control).border(1., Color::Border).align_self(Align::Center);
                if button(cx).text(if state.token_visible { "Hide" } else { "Show" })
                    .aria_label(if state.token_visible {
                    "Hide registration token"
                } else {
                    "Show registration token"
                }).ghost().clicked()
                {
                    state.token_visible = !state.token_visible;
                }
            };
        };
        button(cx).text("Add runner").primary().width_percent(100.);
    };
}

#[component]
#[rustfmt::skip]
fn notification_card(cx: &mut UiContext, state: &mut CuiShowcase) {
    panel(cx, Some("Email me when")) + |cx| {
        setting_toggle(cx, "A build fails", "Only for branches you pushed to", &mut state.build_fails);
        setting_toggle(cx, "A build recovers", "After at least one failure", &mut state.build_recovers);
        setting_toggle(cx, "The nightly report is ready", "Every weekday at 07:00", &mut state.nightly_report);
    };
}

#[component]
#[rustfmt::skip]
fn setting_toggle(cx: &mut UiContext, title: &str, subtitle: &str, value: &mut bool) {
    row(cx).width_percent(100.).gap(12).align(Align::Center) + |cx| {
        column(cx).gap(2).grow(1.).min_width(0.) + |cx| {
            text(cx, title).weight(550);
            text(cx, subtitle).font_size(12).color(Color::Muted);
        };
        // The adjacent title is the visible label; keeping the switch itself
        // indicator-only matches settings rows without repeating the copy.
        switch(cx, "", value).shrink(0.);
    };
}

#[component]
#[rustfmt::skip]
fn cache_card(cx: &mut UiContext) {
    panel(cx, Some("Build cache")) + |cx| {
        row(cx).width_percent(100.).justify(Justify::SpaceBetween) + |cx| {
            text(cx, "Used").weight(550);
            text(cx, "68.4 / 100 GB").color(Color::Muted);
        };
        row(cx).width_percent(100.).height(8.).radius(4.).background(Color::Control) + |cx| {
            column(cx).width_percent(68.4).height_percent(100.).radius(4.).background(Color::Foreground).end();
        };
    };
}

#[component]
#[rustfmt::skip]
fn metric_cards(cx: &mut UiContext) {
    row(cx).width_percent(100.).gap(16) + |cx| {
        column(cx).width(0.).grow(1.) + |cx| {
            panel(cx, Some("Median build")) + |cx| {
                row(cx).gap(8).align(Align::End) + |cx| {
                    text(cx, "3m 42s").font_size(27).weight(650);
                    text(cx, "↘ −17.6%").font_size(12).weight(650).color(Color::Success)
                        .padding_bottom(4);
                };
                spark_bars(cx, &[28., 20., 24., 16., 17., 11., 8., 4., 6., 0.], Color::Accent);
            };
        };
        column(cx).width(0.).grow(1.) + |cx| {
            panel(cx, Some("Flaky tests")) + |cx| {
                row(cx).gap(8).align(Align::End) + |cx| {
                    text(cx, "7").font_size(27).weight(650);
                    text(cx, "↗ +2 this week").font_size(12).weight(650).color(Color::Danger)
                        .padding_bottom(4);
                };
                spark_bars(cx, &[2., 8., 1., 14., 7., 20., 14., 21., 28., 28.], Color::Danger);
            };
        };
    };
}

#[component]
fn spark_bars(cx: &mut UiContext, values: &[f32], color: Color) {
    row(cx).height(34.).gap(3).align(Align::End).children(|cx| {
        for (index, value) in values.iter().copied().enumerate() {
            cx.keyed(index as u64, |cx| {
                column(cx)
                    .width(0.)
                    .grow(1.)
                    .height(value.max(3.))
                    .radius(2.)
                    .background(color)
                    .end();
            });
        }
    });
}

#[component]
#[rustfmt::skip]
fn schedule_card(cx: &mut UiContext, state: &mut CuiShowcase) {
    panel(cx, Some("November 2026")) + |cx| {
        row(cx).width_percent(100.).gap(20).wrap(true).align(Align::Start) + |cx| {
            column(cx).width(210.).gap(8) + |cx| {
                grid(cx).grid_columns([
                    fr(1.),
                    fr(1.),
                    fr(1.),
                    fr(1.),
                    fr(1.),
                    fr(1.),
                    fr(1.),
                ]).column_gap(3).row_gap(4) + |cx|
                {
                    for day in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                        cx.keyed(day, |cx| {
                            text(cx, day).font_size(11).color(Color::Muted).justify_self(Align::Center);
                        });
                    }
                    for (index, day) in [
                        26, 27, 28, 29, 30, 31, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
                        12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
                        27, 28, 29,
                    ].into_iter().enumerate()
                    {
                        cx.keyed(index as u64 + 100, |cx| {
                            let muted = !(6..=29).contains(&index);
                            let selected = !muted && day == state.selected_day;
                            if button_base(cx, day.to_string()).selected(selected).width(28.)
                                .height(28.).min_width(28.).padding(0).align(Align::Center)
                                .justify(Justify::Center).align_self(Align::Center)
                                .justify_self(Align::Center).radius(14.).corner_smoothing(0.)
                                .background(if selected { Color::Foreground } else { Color::Panel })
                                .color(if selected {
                                    Color::Panel
                                } else if muted { Color::Muted } else { Color::Foreground })
                                .children(|cx| {
                                text!(cx, "{}", day).font_size(12)
                                    .weight(if selected { 600 } else { 400 });
                            }).response.clicked()
                                && !muted
                            {
                                state.selected_day = day;
                            }
                        });
                    }
                };
            };
            column(cx).width(150.).grow(1.).min_width(140.).gap(8) + |cx| {
                text(cx, "Release 2.4").weight(650);
                text(cx, "Pick a window while the on-call team is in.").font_size(12).color(Color::Muted);
                for (index, slot) in
                    ["09:00", "11:30", "14:00", "16:30"].into_iter().enumerate()
                {
                    cx.keyed(slot, |cx| {
                        let mut control = button(cx).text(slot).width_percent(100.);
                        if state.selected_slot == index {
                            control = control.primary();
                        }
                        if control.clicked() {
                            state.selected_slot = index;
                        }
                    });
                }
            };
        };
    };
}

#[component]
#[rustfmt::skip]
fn build_chart(cx: &mut UiContext) {
    panel(cx, Some("Build minutes")).description("Linux and Windows runners · last 7 days") + |cx| {
        row(cx).gap(16) + |cx| {
            legend(cx, "Linux", Color::Accent);
            legend(cx, "Windows", Color::Warning);
        };
        row(cx).height(190.).gap(12).align(Align::End) + |cx| {
            for (index, (day, linux, windows)) in [
                ("Mon", 120., 72.),
                ("Tue", 145., 90.),
                ("Wed", 105., 76.),
                ("Thu", 158., 94.),
                ("Fri", 142., 86.),
                ("Sat", 34., 16.),
                ("Sun", 28., 13.),
            ].into_iter().enumerate()
            {
                cx.keyed(index as u64, |cx| {
                    column(cx).width(0.).grow(1.).height_percent(100.).justify(Justify::End).gap(6) + |cx| {
                        row(cx).height(160.).align(Align::End).gap(3) + |cx| {
                            column(cx).width(0.).grow(1.).height(linux).radius(4.)
                                .background(Color::Accent).end();
                            column(cx).width(0.).grow(1.).height(windows).radius(4.)
                                .background(Color::Warning).end();
                        };
                        text(cx, day).font_size(11).color(Color::Muted).align_self(Align::Center);
                    };
                });
            }
        };
    };
}

#[component]
fn legend(cx: &mut UiContext, label: &str, color: Color) {
    row(cx).gap(6).align(Align::Center)
        + |cx| {
            column(cx)
                .width(8.)
                .height(8.)
                .radius(2.)
                .background(color)
                .end();
            text(cx, label).font_size(12).color(Color::Muted);
        };
}

#[component]
#[rustfmt::skip]
fn reviewers_card(cx: &mut UiContext) {
    panel(cx, Some("Reviewers")) + |cx| {
        reviewer(cx, "JB", "Jonas Berg", "jonas@northlight.io", "Required");
        reviewer(cx, "PN", "Priya Nair", "priya@harbor.dev", "Required");
        reviewer(cx, "TR", "Tomas Rivera", "tomas@kiln.app", "Optional");
    };
}

#[component]
#[rustfmt::skip]
fn reviewer(cx: &mut UiContext, initials: &str, name: &str, email: &str, role: &str) {
    row(cx).gap(10).align(Align::Center) + |cx| {
        avatar(cx).fallback(initials).size(34.);
        column(cx).gap(1).grow(1.).min_width(0.) + |cx| {
            text(cx, name).weight(600).ellipsis(true);
            text(cx, email).font_size(11).color(Color::Muted).ellipsis(true);
        };
        button(cx).text(role);
    };
}

#[component]
#[rustfmt::skip]
fn release_card(cx: &mut UiContext, state: &mut CuiShowcase) {
    panel(cx, Some("Release candidate")).description("Builds for all three platforms, signed.") + |cx| {
        text_input_string(cx, "Release URL", &mut state.release_url).width_percent(100.);
        segmented(cx, &mut state.release_access, &["Public", "Team only"]);
    };
}

#[component]
#[rustfmt::skip]
fn coverage_card(cx: &mut UiContext, state: &mut CuiShowcase) {
    panel(cx, None) + |cx| {
        row(cx).gap(14).align(Align::Center) + |cx| {
            column(cx).position(Position::Relative).width(76.).height(76.).align(Align::Center)
                .justify(Justify::Center) + |cx|
            {
                progress_ring_sized(cx, 0.763, 76., 7., Color::Foreground, Color::Border);
                column(cx).absolute(0., 0.).width(76.).height(76.).align(Align::Center)
                    .justify(Justify::Center) + |cx|
                {
                    text(cx, "76%").font_size(20).weight(650);
                };
            };
            column(cx).gap(3) + |cx| {
                text(cx, "Line coverage").weight(650);
                text(cx, "76.3% on main, 3,912 of 5,127 lines.").font_size(11).color(Color::Muted);
            };
        };
        row(cx).width_percent(100.).justify(Justify::SpaceBetween) + |cx| {
            text(cx, "Fail below").weight(550);
            text!(cx, "{:.0}%", state.coverage_floor).color(Color::Muted);
        };
        slider(cx, "Fail below", &mut state.coverage_floor, 0.0..=100.0).width_percent(100.);
    };
}

#[component]
#[rustfmt::skip]
fn matrix_card(cx: &mut UiContext, state: &mut CuiShowcase) {
    panel(cx, Some("Build matrix")) + |cx| {
        row(cx).gap(8).wrap(true) + |cx| {
            for (index, label) in ["x86-64", "ARM64", "Debug", "Release", "ASan", "MinGW"].into_iter()
                .enumerate()
            {
                cx.keyed(label, |cx| {
                    chip(cx, label, &mut state.matrix[index]);
                });
            }
        };
    };
}
