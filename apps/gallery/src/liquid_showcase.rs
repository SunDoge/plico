//! Native component exhibition after onemorethink.dev's landing grid.
//! Upstream MIT attribution and revision are in third_party/liquid-glass.md.
use plico::{
    prelude::*,
    widgets::liquid::{self, ActionState},
};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Showcase {
    playing: bool,
    deployed: bool,
    #[serde(skip)]
    deployment_due: Option<std::time::Duration>,
    tab: usize,
    confirmed: bool,
    #[serde(skip)]
    pub(crate) clock_readout: Option<(u32, u32, u32)>,
    #[serde(skip)]
    pub(crate) card_date: Option<(usize, u32)>,
    #[serde(skip)]
    pub(crate) card_link: Option<&'static str>,
    #[serde(skip)]
    alarm_reset: Option<std::time::Duration>,
    brightness: f64,
    notifications: bool,
    remember: bool,
    remember_circle: bool,
    delivery: usize,
    selection: PopoverState,
    menu: PopoverState,
    stretched_menu: PopoverState,
    message_menu: PopoverState,
    reminder_choice: Option<usize>,
    reminder: PopoverState,
    action: String,
}
impl Default for Showcase {
    fn default() -> Self {
        Self {
            playing: false,
            deployed: false,
            deployment_due: None,
            tab: 1,
            confirmed: false,
            clock_readout: None,
            card_date: None,
            card_link: None,
            alarm_reset: None,
            brightness: 40.,
            notifications: true,
            remember: true,
            remember_circle: false,
            delivery: usize::MAX,
            selection: Default::default(),
            menu: Default::default(),
            stretched_menu: Default::default(),
            message_menu: Default::default(),
            reminder_choice: None,
            reminder: Default::default(),
            action: String::new(),
        }
    }
}

#[derive(Clone, Default, PartialEq)]
struct Autoplay {
    demo: Showcase,
    phase: Option<plico::LoopPhase>,
}

#[derive(Clone, Copy, PartialEq)]
enum DemoPopup {
    Delivery,
    More,
    Options,
    Reminder,
}

impl DemoPopup {
    fn tile(self) -> &'static str {
        match self {
            Self::Delivery => "Select",
            Self::More => "Button Group",
            Self::Options => "Dropdown Menu",
            Self::Reminder => "Popover",
        }
    }

    fn state(self, demo: &mut Showcase) -> &mut PopoverState {
        match self {
            Self::Delivery => &mut demo.selection,
            Self::More => &mut demo.message_menu,
            Self::Options => &mut demo.menu,
            Self::Reminder => &mut demo.reminder,
        }
    }
}

impl Autoplay {
    fn advance(&mut self, cx: &mut UiContext, columns: usize) -> Option<u64> {
        let phase = cx.loop_phases(
            "liquid showcase",
            Duration::from_millis(300),
            &[Duration::from_millis(2800), Duration::from_millis(950)],
        );
        let pulse = phase
            .filter(|phase| phase.index == 0)
            .map(|phase| phase.cycle);
        let running = pulse.is_some();
        let phase_changed = phase != self.phase;
        self.phase = phase;
        if phase_changed {
            self.demo.playing = false;
            self.demo.deployed = false;
            self.demo.deployment_due = running.then(|| cx.now() + Duration::from_millis(1400));
            self.demo.tab = if running { 3 } else { 1 };
            self.demo.brightness = if running { 82. } else { 40. };
            self.demo.notifications = !running;
            self.demo.remember = !running;
            self.demo.remember_circle = running;
            self.demo.confirmed = running;
            self.demo.alarm_reset = None;
        }

        let [offset, height] = cx.scroll_viewport().unwrap_or_default();
        let manual_popup_open = [
            &self.demo.selection,
            &self.demo.message_menu,
            &self.demo.menu,
            &self.demo.stretched_menu,
            &self.demo.reminder,
        ]
        .into_iter()
        .any(|state| state.is_open() && !state.is_preview());
        for popup in [
            DemoPopup::Delivery,
            DemoPopup::More,
            DemoPopup::Options,
            DemoPopup::Reminder,
        ] {
            let index = PANELS
                .iter()
                .position(|&tile| tile == popup.tile())
                .unwrap();
            let top = (index / columns) as f32 * (PANEL_HEIGHT + f32::from(PANEL_GAP));
            let visible = top >= offset && top + PANEL_HEIGHT <= offset + height;
            let show = running && visible && !manual_popup_open;
            let state = popup.state(&mut self.demo);
            if show && phase_changed {
                state.open_preview();
                if popup == DemoPopup::Delivery {
                    self.demo.delivery = usize::MAX;
                }
            } else if !show && state.is_preview() && state.is_open() {
                state.close();
                if popup == DemoPopup::Delivery && !running {
                    self.demo.delivery = 1;
                }
            }
        }
        pulse
    }
}

const TILES: [&str; 12] = [
    "Action Button",
    "Tabs",
    "Badge",
    "Slide to Confirm",
    "Card",
    "Slider",
    "Switch",
    "Checkbox",
    "Select",
    "Button Group",
    "Dropdown Menu",
    "Popover",
];

#[component]
fn glass_grid(cx: &mut UiContext, dark: bool) {
    for y in 0..7 {
        for x in 0..13 {
            let dist = ((x as f32 - 6.) / 6.5).hypot((y as f32 - 3.) / 3.5);
            let alpha = ((1. - dist) / 0.45).clamp(0., 1.) * 0.1;
            if alpha > 0. {
                let ink = Color::Literal(if dark { 0xffffff } else { 0x000000 });
                cx.keyed((y * 13 + x) as u64, |cx| {
                    column(cx)
                        .absolute(x as f32 * 40., y as f32 * 40.)
                        .width(40.)
                        .height(1.)
                        .background(ink)
                        .background_alpha(alpha)
                        .children(|_| {});
                    column(cx)
                        .absolute(x as f32 * 40., y as f32 * 40.)
                        .width(1.)
                        .height(40.)
                        .background(ink)
                        .background_alpha(alpha)
                        .children(|_| {});
                });
            }
        }
    }
}

fn play_stage(cx: &mut UiContext, playing: &mut bool, dark: bool) {
    // The original glass-button-clear scene is 13 × 7 cells of 40 DIP.
    column(cx)
        .width(520.)
        .height(280.)
        .shrink(0.)
        .position(Position::Relative)
        .align(Align::Center)
        .justify(Justify::Center)
        .children(|cx| {
            glass_grid(cx, dark);
            play_button(cx, playing, None);
        });
}

fn play_button(cx: &mut UiContext, playing: &mut bool, pulse: Option<u64>) {
    let (label, icon) = if *playing {
        ("Pause", &crate::icons::PAUSE)
    } else {
        ("Play", &crate::icons::PLAY)
    };
    let response = liquid::clear_button_with_pulse(cx, label, icon, pulse);
    if response.clicked {
        *playing = !*playing;
    }
    if response.pulse_finished {
        *playing = true;
        cx.request_rebuild();
    }
}

#[component]
fn grid_backdrop(cx: &mut UiContext, dark: bool) {
    column(cx)
        .absolute(0., 0.)
        .width_percent(100.)
        .height_percent(100.)
        .clip(true)
        .align(Align::Center)
        .justify(Justify::Center)
        .children(|cx| {
            column(cx)
                .width(520.)
                .height(280.)
                .shrink(0.)
                .position(Position::Relative)
                .children(|cx| glass_grid(cx, dark));
        });
}
fn message_button(
    cx: &mut UiContext,
    label: &str,
    icon: &plico::SvgAsset,
    show_label: bool,
    color: Color,
) -> bool {
    button_base(cx, label)
        .height(32.)
        .min_width(32.)
        .radius(16.)
        .gap(8)
        .align(Align::Center)
        .justify(Justify::Center)
        .padding_x(if show_label { 14 } else { 0 })
        .color(color)
        .children(|cx| {
            svg(cx, icon).size(16., 16.);
            if show_label {
                text(cx, label).font_size(14).weight(600);
            }
        })
        .response
        .clicked()
}

// Slot order follows place value, with independent separators. This keeps the
// units column anchored when the thousands or fractional part appears.
const DISTANCE_SLOTS: [[u8; 7]; 4] = [*b"    8.4", *b"   52.1", *b"  214  ", *b"2,480  "];

fn distance_readout(cx: &mut UiContext, tab: usize) {
    let now = cx.now();
    let motion = cx.state(|| (tab, tab, now));
    let (mut from, mut to, mut started) = motion.get();
    if to != tab {
        from = to;
        to = tab;
        started = now;
        motion.set((from, to, started));
    }
    let time = if cx.display_preferences().reduce_motion {
        1.
    } else {
        (now.saturating_sub(started).as_secs_f32() / 0.9).min(1.)
    };
    let progress = if time < 1. && from != to {
        cx.request_animation_frame();
        let t = time * 12.;
        (1. - (-t).exp() * (1. + t + t * t / 2. + t * t * t / 6.)).min(1.)
    } else {
        1.
    };
    let increasing = to > from;
    row(cx).gap(0).align(Align::Baseline).children(|cx| {
        let old = DISTANCE_SLOTS[from];
        let new = DISTANCE_SLOTS[to];
        for (place, (a, b)) in old.into_iter().zip(new).enumerate() {
            let width = if place == 1 || place == 5 { 15. } else { 37. };
            let old_width = if a == b' ' { 0. } else { width };
            let new_width = if b == b' ' { 0. } else { width };
            let shown_width = old_width + (new_width - old_width) * progress;
            if shown_width <= 0. {
                continue;
            }
            cx.keyed(place as u64, |cx| {
                column(cx)
                    .width(shown_width)
                    .height(72.)
                    .position(Position::Relative)
                    .clip(true)
                    .children(|cx| {
                        if !a.is_ascii_digit() && !b.is_ascii_digit() {
                            let byte = if b == b' ' { a } else { b };
                            let glyph = [byte];
                            text(cx, std::str::from_utf8(&glyph).unwrap())
                                .font_size(60)
                                .weight(600)
                                .opacity((shown_width / width).min(1.));
                            return;
                        }
                        let first = i32::from(if a.is_ascii_digit() { a - b'0' } else { 0 });
                        let last = i32::from(if b.is_ascii_digit() { b - b'0' } else { 0 });
                        let delta = if increasing {
                            (last - first).rem_euclid(10)
                        } else {
                            -(first - last).rem_euclid(10)
                        };
                        let position = first as f32 + delta as f32 * progress;
                        let index = position.floor() as i32;
                        for step in 0..2 {
                            cx.keyed(step as u64, |cx| {
                                let digit = (index + step).rem_euclid(10) as u8;
                                let glyph = [b'0' + digit];
                                let offset = (index as f32 + step as f32 - position) * 72.;
                                text(cx, std::str::from_utf8(&glyph).unwrap())
                                    .font_size(60)
                                    .weight(600)
                                    .absolute(0., offset)
                                    .opacity((shown_width / width).min(1.));
                            });
                        }
                    });
            });
        }
    });
}

static CARD_RIBBONS: plico::ShaderAsset =
    plico::shader_asset!("../assets/shaders/card_ribbons.wgsl");

fn weather_showcase(cx: &mut UiContext, width: f32, height: f32, s: &mut Showcase) {
    let animated = !cx.display_preferences().reduce_motion;
    row(cx)
        .position(Position::Relative)
        .width(width)
        .height(height)
        .gap(20)
        .align(Align::Center)
        .justify(Justify::Center)
        .children(|cx| {
            shader(cx, &CARD_RIBBONS)
                .absolute(0., 0.)
                .width(width)
                .height(height)
                .animated(animated);
            liquid::frosted_card(cx, 176., 176., 16., |cx| {
                column(cx).gap(2).children(|cx| {
                    row(cx).gap(4).align(Align::Center).children(|cx| {
                        text(cx, "Istanbul").font_size(13).weight(600);
                        svg(cx, &crate::icons::NAVIGATION).size(10., 10.);
                    });
                    text(cx, "16°").font_size(44).weight(300);
                });
                column(cx).gap(2).children(|cx| {
                    svg(cx, &crate::icons::CLOUD).size(16., 16.);
                    text(cx, "Cloudy").font_size(12).weight(600);
                    text(cx, "H:19° L:15°")
                        .font_size(12)
                        .weight(600)
                        .color(Color::Muted);
                });
            });
            if width >= 452. {
                liquid::frosted_card(cx, 256., 176., 20., |cx| {
                    row(cx)
                        .justify(Justify::SpaceBetween)
                        .align(Align::Baseline)
                        .children(|cx| {
                            const DAYS: [&str; 7] = [
                                "Monday",
                                "Tuesday",
                                "Wednesday",
                                "Thursday",
                                "Friday",
                                "Saturday",
                                "Sunday",
                            ];
                            let (weekday, day) = s.card_date.unwrap_or((0, 24));
                            text(cx, DAYS[weekday]).font_size(14).weight(600);
                            text!(cx, "{day}").font_size(34).weight(600);
                        });
                    column(cx).gap(8).children(|cx| {
                        for (title, note, url, color) in [
                            (
                                "Visit my GitHub",
                                "github.com/bilalgursen",
                                "https://github.com/bilalgursen",
                                0x0a84ff,
                            ),
                            (
                                "Star liquid-glass",
                                "Leave a star on the repo",
                                "https://github.com/bilalgursen/liquid-glass",
                                0xff6b3d,
                            ),
                        ] {
                            cx.keyed(title, |cx| {
                                if plico::widgets::link_base(cx, title)
                                    .gap(10)
                                    .width_percent(100.)
                                    .children(|cx| {
                                        column(cx)
                                            .width(4.)
                                            .height(32.)
                                            .radius(2.)
                                            .background(Color::Literal(color))
                                            .end();
                                        column(cx).children(|cx| {
                                            text(cx, title).font_size(13).weight(600);
                                            text(cx, note).font_size(11).color(Color::Muted);
                                        });
                                    })
                                    .response
                                    .clicked()
                                {
                                    s.card_link = Some(url);
                                }
                            });
                        }
                    });
                });
            }
        });
}

#[component]
fn demo(cx: &mut UiContext, title: &str, s: &mut Showcase, w: f32, compact: bool) {
    match title {
        "Action Button" => {
            let now = cx.now();
            if s.deployment_due.is_some_and(|deadline| now >= deadline) {
                s.deployed = true;
                s.deployment_due = None;
            }
            let (label, state) = if let Some(deadline) = s.deployment_due {
                cx.request_rebuild_at(deadline);
                ("Deploying", ActionState::Pending)
            } else if s.deployed {
                ("Deployed", ActionState::Success)
            } else {
                ("Deploy to production", ActionState::Idle)
            };
            if liquid::action_button(cx, label, state) {
                let deadline = now + std::time::Duration::from_millis(1400);
                s.deployment_due = Some(deadline);
                cx.request_rebuild_at(deadline);
                cx.request_animation_frame();
            }
            if button(cx)
                .text("Start over")
                .icon(&crate::icons::ROTATE_CCW)
                .ghost()
                .clicked()
            {
                s.deployment_due = None;
                s.deployed = false;
            }
        }
        "Tabs" => {
            let tab = s.tab.min(3);
            column(cx).gap(32).align(Align::Center).children(|cx| {
                column(cx).gap(8).align(Align::Center).children(|cx| {
                    row(cx).gap(6).align(Align::Baseline).children(|cx| {
                        distance_readout(cx, tab);
                        text(cx, "km").font_size(24).weight(500).color(Color::Muted);
                    });
                    text!(
                        cx,
                        "Distance this {}",
                        ["day", "week", "month", "year"][tab]
                    )
                    .font_size(14)
                    .color(Color::Muted);
                });
                liquid::tabs(
                    cx,
                    &mut s.tab,
                    &["Day", "Week", "Month", "Year"],
                    ((w - 52.) / 4.).clamp(32., 68.),
                );
            });
        }
        "Badge" => {
            let width = (w - 56.).clamp(120., 320.);
            column(cx)
                .width(width)
                .position(Position::Relative)
                .radius(28.)
                .border(1., Color::Border)
                .padding(24)
                .gap(8)
                .children(|cx| {
                    let hovered = cx.hovered_within();
                    const MONTHS: [&str; 12] = [
                        "Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos",
                        "Eylül", "Ekim", "Kasım", "Aralık",
                    ];
                    let month = s.clock_readout.map_or(10, |(_, _, month)| month as usize);
                    text(cx, MONTHS[month]).font_size(24).weight(600);
                    text(cx, "Sliders, switches and checkboxes, now in liquid glass.")
                        .font_size(14)
                        .color(Color::Muted);
                    row(cx)
                        .position(Position::Absolute)
                        .inset_top(-10.)
                        .inset_right(-10.)
                        .children(|cx| {
                            liquid::tinted_badge(cx, "New", 64., Color::Literal(0xea580c), hovered)
                        });
                });
        }
        "Slide to Confirm" => {
            let now = cx.now();
            if let Some(deadline) = s.alarm_reset {
                if now >= deadline {
                    s.confirmed = false;
                    s.alarm_reset = None;
                } else {
                    cx.request_rebuild_at(deadline);
                }
            }
            let wide = w >= 560.;
            let alarm = if wide { row(cx) } else { column(cx) };
            alarm
                .align(Align::Center)
                .gap(if compact { 8 } else { 24 })
                .children(|cx| {
                    column(cx)
                        .align(if wide { Align::Start } else { Align::Center })
                        .gap(4)
                        .children(|cx| {
                            row(cx).gap(8).align(Align::Center).children(|cx| {
                                svg(cx, &crate::icons::ALARM_CLOCK)
                                    .size(20., 20.)
                                    .color(Color::Muted);
                                text(cx, "Time to wake up")
                                    .font_size(if compact { 15 } else { 18 })
                                    .color(Color::Muted);
                            });
                            let (hour, minute, _) = s.clock_readout.unwrap_or((7, 0, 10));
                            text!(cx, "{hour:02}:{minute:02}")
                                .font_size(if compact { 32 } else { 60 })
                                .weight(600);
                        });
                    let track_width = (w - 48.).clamp(132., 288.);
                    let snooze_height = if compact { 48. } else { 96. };
                    column(cx).gap(if compact { 8 } else { 12 }).children(|cx| {
                        button_base(cx, "Snooze")
                            .width(track_width)
                            .height(snooze_height)
                            .radius(snooze_height * 0.5)
                            .background(Color::Literal(0xf97316))
                            .color(Color::Literal(0xffffff))
                            .pressed_background(Color::Literal(0xea580c))
                            .align(Align::Center)
                            .justify(Justify::Center)
                            .children(|cx| {
                                text(cx, "Snooze").font_size(20).weight(600);
                            });
                        let state = if s.confirmed {
                            ActionState::Success
                        } else {
                            ActionState::Idle
                        };
                        if liquid::slide_to_confirm_xl(
                            cx,
                            "Slide to stop",
                            "Stopped",
                            state,
                            track_width,
                        ) {
                            let deadline = now + std::time::Duration::from_millis(2600);
                            s.confirmed = true;
                            s.alarm_reset = Some(deadline);
                            cx.request_rebuild_at(deadline);
                        }
                    });
                });
        }
        "Card" => weather_showcase(cx, w, if compact { 220. } else { 288. }, s),
        "Slider" => {
            let width = (w - 48.).clamp(132., 384.);
            column(cx).width(width).gap(8).children(|cx| {
                row(cx)
                    .width_percent(100.)
                    .padding_x(4)
                    .justify(Justify::SpaceBetween)
                    .children(|cx| {
                        text(cx, "Brightness").font_size(13).weight(500);
                        text!(cx, "{:.0}", s.brightness)
                            .font_size(13)
                            .color(Color::Muted);
                    });
                row(cx).gap(16).align(Align::Center).children(|cx| {
                    svg(cx, &crate::icons::MOON)
                        .size(16., 16.)
                        .color(Color::Muted);
                    liquid::range_slider(
                        cx,
                        "Brightness",
                        &mut s.brightness,
                        0.0..=100.,
                        1.,
                        width - 64.,
                    );
                    svg(cx, &crate::icons::SUN)
                        .size(16., 16.)
                        .color(Color::Muted);
                });
            });
        }
        "Switch" => {
            liquid::glass_switch_with_description(
                cx,
                "Notifications",
                "Banners and sounds",
                &mut s.notifications,
            );
        }
        "Checkbox" => {
            column(cx).gap(16).children(|cx| {
                liquid::glass_checkbox(cx, "Remember this device", &mut s.remember);
                liquid::glass_checkbox_circle(cx, "Circle", &mut s.remember_circle);
            });
        }
        "Select" => {
            let field_width = (w - 32.).clamp(80., 256.);
            column(cx).width(field_width).gap(8).children(|cx| {
                text(cx, "Delivery").font_size(14).weight(500);
                liquid::select(
                    cx,
                    &mut s.selection,
                    "Delivery",
                    &mut s.delivery,
                    &[
                        SelectOption::new("standard", 0, "Standard"),
                        SelectOption::new("express", 1, "Express"),
                        SelectOption::new("same-day", 2, "Same day"),
                        SelectOption::new("pickup", 3, "Pickup (closed today)").disabled(true),
                    ],
                )
                .width(field_width)
                .placeholder("Choose a speed")
                .response();
            });
        }
        "Button Group" => {
            let labels = w > 440.;
            let white = LiquidGlass {
                tint: Color::Literal(0xffffff),
                tint_opacity: 1.,
                fallback: Color::Literal(0xffffff),
                ..LiquidGlass::clear()
            };
            row(cx).gap(8).align(Align::Center).children(|cx| {
                liquid::button_group(cx, false)
                    .gap(0)
                    .radius(20.)
                    .material(LiquidGlass {
                        tint: Color::Literal(0x171717),
                        tint_opacity: 1.,
                        fallback: Color::Literal(0x171717),
                        ..LiquidGlass::clear()
                    })
                    .children(|cx| {
                        let _ = message_button(
                            cx,
                            "Back",
                            &crate::icons::ARROW_LEFT,
                            false,
                            Color::Literal(0xffffff),
                        );
                    });
                liquid::button_group(cx, false)
                    .gap(0)
                    .radius(20.)
                    .material(white)
                    .children(|cx| {
                        for (label, icon) in [
                            ("Archive", crate::icons::ARCHIVE),
                            ("Report", crate::icons::FLAG),
                        ] {
                            cx.keyed(label, |cx| {
                                if message_button(
                                    cx,
                                    label,
                                    &icon,
                                    labels,
                                    Color::Literal(0x171717),
                                ) {
                                    s.action = label.into();
                                }
                            });
                        }
                    });
                liquid::button_group(cx, false)
                    .radius(20.)
                    .material(LiquidGlass::clear())
                    .children(|cx| {
                        if message_button(
                            cx,
                            "Snooze",
                            &crate::icons::CLOCK,
                            labels,
                            Color::Foreground,
                        ) {
                            s.action = "Snooze".into();
                        }
                    });
                liquid::dropdown_menu(cx, &mut s.message_menu, "More")
                    .over_anchor(true)
                    .trigger_icon(&crate::icons::ELLIPSIS, true)
                    .material(white)
                    .children(|cx| {
                        for (label, icon) in [
                            ("Mark as read", crate::icons::MAIL_OPEN),
                            ("Add label", crate::icons::TAG),
                            ("Mute thread", crate::icons::BELL_OFF),
                            ("Delete", crate::icons::TRASH),
                        ] {
                            cx.keyed(label, |cx| {
                                if label == "Delete" {
                                    column(cx).padding_x(14).padding_y(4).children(separator);
                                }
                                if menu_item(cx, label)
                                    .icon(&icon)
                                    .destructive(label == "Delete")
                                    .clicked()
                                {
                                    s.action = label.into();
                                }
                            });
                        }
                    });
            });
        }
        "Dropdown Menu" => {
            row(cx).gap(12).children(|cx| {
                for (label, state, centered) in [
                    ("Options", &mut s.menu, true),
                    ("Actions", &mut s.stretched_menu, false),
                ] {
                    cx.keyed(label, |cx| {
                        liquid::dropdown_menu(cx, state, label)
                            .over_anchor(centered)
                            .children(|cx| {
                                for (label, icon) in [
                                    ("Rename", crate::icons::PENCIL),
                                    ("Duplicate", crate::icons::COPY),
                                    ("Share", crate::icons::SHARE_2),
                                    ("Delete", crate::icons::TRASH),
                                ] {
                                    cx.keyed(label, |cx| {
                                        if label == "Delete" {
                                            liquid::menu_separator(cx);
                                        }
                                        if menu_item(cx, label)
                                            .icon(&icon)
                                            .destructive(label == "Delete")
                                            .clicked()
                                        {
                                            s.action = label.into();
                                        }
                                    });
                                }
                            });
                    });
                }
            });
        }
        "Popover" => {
            let times = [
                ("In an hour", crate::icons::ALARM_CLOCK),
                ("Tonight at 8", crate::icons::MOON),
                ("Tomorrow morning", crate::icons::SUNRISE),
            ];
            let label = s
                .reminder_choice
                .and_then(|index| times.get(index))
                .map_or("Remind me", |(label, _)| *label);
            liquid::popover(cx, &mut s.reminder, label)
                .over_anchor(true)
                .width(288.)
                .trigger_icon(&crate::icons::ALARM_CLOCK, false)
                .children(|cx| {
                    text(cx, "Remind me").font_size(16).weight(600);
                    text(cx, "We will send one notification, then leave you alone.")
                        .font_size(14)
                        .color(Color::Muted);
                    column(cx).gap(6).padding_top(4).children(|cx| {
                        for (index, (label, icon)) in times.into_iter().enumerate() {
                            cx.keyed(label, |cx| {
                                if button_base(cx, label)
                                    .width_percent(100.)
                                    .height(40.)
                                    .radius(20.)
                                    .align(Align::Center)
                                    .padding_x(16)
                                    .gap(8)
                                    .background(Color::Foreground)
                                    .background_alpha(0.10)
                                    .hover_background(Color::Muted)
                                    .children(|cx| {
                                        svg(cx, &icon).size(16., 16.);
                                        text(cx, label).font_size(14).weight(600).ellipsis(true);
                                    })
                                    .response
                                    .clicked()
                                {
                                    s.reminder_choice = Some(index);
                                    cx.close_popover();
                                }
                            });
                        }
                    });
                });
        }
        _ => unreachable!(),
    }
}
#[component]
pub fn page(cx: &mut UiContext, s: &mut Showcase, dark: bool, collapsed: bool) {
    let width = (cx.viewport_width() - if collapsed { 64. } else { 224. } - 48.).max(1.);
    let stage = Color::Literal(if dark { 0x121315 } else { 0xececee });
    let cols = if width >= 640. { 2 } else { 1 };
    column(cx)
        .height(0.)
        .grow(1.)
        .scroll_y(true)
        .padding_right(24)
        .gap(16)
        .children(|cx| {
            column(cx)
                .align(Align::Center)
                .justify(Justify::Center)
                .height(190.)
                .shrink(0.)
                .gap(12)
                .children(|cx| {
                    text(cx, "Glass that bends the page.")
                        .font_size(if width >= 640. { 48 } else { 30 })
                        .weight(650);
                    text(cx, "Liquid Glass components for Plico").color(Color::Muted);
                });
            column(cx)
                .width_percent(100.)
                .height(if width >= 640. { 520. } else { 360. })
                .shrink(0.)
                .radius(28.)
                .background(stage)
                .clip(true)
                .children(|cx| {
                    column(cx)
                        .grow(1.)
                        .align(Align::Center)
                        .justify(Justify::Center)
                        .children(|cx| {
                            play_stage(cx, &mut s.playing, dark);
                        });
                    column(cx)
                        .padding_x(28)
                        .padding_bottom(24)
                        .gap(4)
                        .children(|cx| {
                            text(cx, "Button").font_size(19).weight(600);
                            text(cx, "Swells under the pointer, bends the page behind it.")
                                .color(Color::Muted);
                        });
                });
            for tiles in TILES.chunks(cols) {
                cx.keyed(tiles[0], |cx| {
                    row(cx).gap(16).shrink(0.).children(|cx| {
                        for &title in tiles {
                            cx.keyed(title, |cx| {
                                column(cx)
                                    .width(0.)
                                    .grow(1.)
                                    .height(if cols == 2 || title == "Slide to Confirm" {
                                        420.
                                    } else {
                                        280.
                                    })
                                    .radius(28.)
                                    .background(stage)
                                    .children(|cx| {
                                        column(cx)
                                            .grow(1.)
                                            .position(Position::Relative)
                                            .align(Align::Center)
                                            .justify(Justify::Center)
                                            .gap(10)
                                            .children(|cx| {
                                                if matches!(
                                                    title,
                                                    "Select"
                                                        | "Dropdown Menu"
                                                        | "Popover"
                                                        | "Button Group"
                                                ) {
                                                    grid_backdrop(cx, dark);
                                                }
                                                demo(
                                                    cx,
                                                    title,
                                                    s,
                                                    (width - 16. * (cols - 1) as f32) / cols as f32,
                                                    false,
                                                );
                                            });
                                        text(cx, title)
                                            .padding_x(28)
                                            .padding_bottom(24)
                                            .font_size(19)
                                            .weight(600);
                                    });
                            });
                        }
                    });
                });
            }
        });
}

const PANELS: [&str; 12] = [
    "Button",
    "Action Button",
    "Tabs",
    "Badge",
    "Card",
    "Slider",
    "Switch & Checkbox",
    "Slide to Confirm",
    "Select",
    "Button Group",
    "Dropdown Menu",
    "Popover",
];
const PANEL_GAP: u16 = 16;
const PANEL_HEIGHT: f32 = 280.;

/// The compact, live layout used for the Gallery overview and single-pass recording.
#[component]
pub fn wall(cx: &mut UiContext, s: &mut Showcase, dark: bool, collapsed: bool) {
    let inset = cx.theme().metrics.space(6);
    let width = cx.viewport_width() - if collapsed { 64. } else { 224. } - f32::from(inset) * 2.;
    let columns = if width >= 1360. {
        4
    } else if width >= 1010. {
        3
    } else if width >= 660. {
        2
    } else {
        1
    };
    let tile_width =
        ((width - f32::from(PANEL_GAP) * (columns - 1) as f32) / columns as f32).max(1.);
    let stage = Color::Literal(if dark { 0x121315 } else { 0xececee });
    let heading = cx.tr("liquid-glass-showcase", "Liquid Glass Showcase");
    let autoplay_label = cx.tr("liquid-glass-autoplay", "Autoplay animations");
    let reduced_motion = cx.display_preferences().reduce_motion;
    let stored = cx.state(|| None::<Autoplay>);
    let mut autoplay = stored.get();
    column(cx)
        .height(0.)
        .grow(1.)
        .padding_right(inset)
        .gap(PANEL_GAP)
        .children(|cx| {
            let mut enabled = autoplay.is_some() && !reduced_motion;
            row(cx)
                .gap(12)
                .wrap(true)
                .align(Align::Center)
                .justify(Justify::SpaceBetween)
                .children(|cx| {
                    text(cx, heading.as_ref())
                        .font_size(if columns == 1 { 28 } else { 36 })
                        .weight(650);
                    checkbox(cx, autoplay_label.as_ref(), &mut enabled).disabled(reduced_motion);
                });
            if !enabled {
                autoplay = None;
            } else if autoplay.is_none() {
                autoplay = Some(Autoplay::default());
            }
            column(cx)
                .height(0.)
                .grow(1.)
                .scroll_y(true)
                .children(|cx| {
                    let (demo_state, button_pulse) = if let Some(playback) = &mut autoplay {
                        playback.demo.clock_readout = s.clock_readout;
                        playback.demo.card_date = s.card_date;
                        let button_pulse = playback.advance(cx, columns);
                        (&mut playback.demo, button_pulse)
                    } else {
                        (&mut *s, None)
                    };
                    grid(cx)
                        .width_percent(100.)
                        .grid_columns(std::iter::repeat_n(fr(1.), columns))
                        .gap(PANEL_GAP)
                        .shrink(0.)
                        .children(|cx| {
                            for title in PANELS {
                                cx.keyed(title, |cx| {
                                    column(cx)
                                        .min_width(0.)
                                        .height(PANEL_HEIGHT)
                                        .radius(28.)
                                        .background(stage)
                                        .children(|cx| {
                                            column(cx)
                                                .height(0.)
                                                .grow(1.)
                                                .position(Position::Relative)
                                                .align(Align::Center)
                                                .justify(Justify::Center)
                                                .gap(10)
                                                .children(|cx| match title {
                                                    "Button" => {
                                                        grid_backdrop(cx, dark);
                                                        play_button(
                                                            cx,
                                                            &mut demo_state.playing,
                                                            button_pulse,
                                                        );
                                                    }
                                                    "Switch & Checkbox" => {
                                                        demo(
                                                            cx, "Switch", demo_state, tile_width,
                                                            true,
                                                        );
                                                        demo(
                                                            cx, "Checkbox", demo_state, tile_width,
                                                            true,
                                                        );
                                                    }
                                                    _ => {
                                                        if matches!(
                                                            title,
                                                            "Select"
                                                                | "Button Group"
                                                                | "Dropdown Menu"
                                                                | "Popover"
                                                        ) {
                                                            grid_backdrop(cx, dark);
                                                        }
                                                        demo(
                                                            cx, title, demo_state, tile_width, true,
                                                        );
                                                    }
                                                });
                                            text(cx, title)
                                                .padding_x(20)
                                                .padding_bottom(16)
                                                .font_size(16)
                                                .weight(600);
                                        });
                                });
                            }
                        });
                });
        });
    if let Some(playback) = &mut autoplay
        && let Some(link) = playback.demo.card_link.take()
    {
        s.card_link = Some(link);
    }
    stored.set(autoplay);
}
