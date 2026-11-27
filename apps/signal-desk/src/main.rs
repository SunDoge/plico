use plico::{BoxShadow, prelude::*};
use plico_desktop::{NativeApp, PreviewScenario, PreviewViewport, WindowOptions};
use snafu::{ResultExt, Snafu};
use std::{collections::hash_map::RandomState, hash::BuildHasher, path::PathBuf, time::Duration};

mod icons;
mod telemetry;

use telemetry::{Host, Sample, Storage, Traffic, traffic_path};

const BACKGROUND: Color = Color::Literal(0x101522);
const CARD: Color = Color::Literal(0x161e2d);
const TRACK: Color = Color::Literal(0x2b3549);
const ACCENT: Color = Color::Literal(0x68e3ce);
const RECEIVE: Color = Color::Literal(0x73c9ff);
const TRANSMIT: Color = Color::Literal(0xb5a0fa);
const RADIUS: f32 = 14.;
const GAP: f32 = 12.;
const DEFAULT_WINDOW: (u32, u32) = (1280, 720);
const SIDEBAR_RATIO: f32 = 0.116;
const SIDEBAR_GUTTER: f32 = 14.;
const SAMPLE_PERIOD: Duration = Duration::from_secs(1);
const TOOLBAR_FACE: Color = Color::Literal(0x252930);
const SEGMENT_SELECTED: Color = Color::Literal(0x4d535d);
const SEGMENT_HOVERED: Color = Color::Literal(0x353b44);
const TOOLBAR_HEIGHT: f32 = 32.;
const SEGMENT_INSET: f32 = 2.;
const SEGMENT_GAP: f32 = 1.;
const SEGMENT_WIDTH: f32 = 73.;
const SEGMENT_HEIGHT: f32 = TOOLBAR_HEIGHT - 2. * SEGMENT_INSET;
static ATMOSPHERE: plico::ShaderAsset = plico::shader_asset!("../assets/atmosphere.wgsl");
const VIEWS: [(&str, &plico::SvgAsset); 4] = [
    ("Overview", &icons::ACTIVITY),
    ("Compute", &icons::CPU),
    ("Network", &icons::NETWORK),
    ("Storage", &icons::HARD_DRIVE),
];

#[derive(usage_rs::Cli)]
#[usage(
    bin = "signal-desk",
    about = "A native Liquid Glass system monitor demo"
)]
struct Options {
    /// Capture a deterministic dashboard image.
    #[usage(long)]
    capture: Option<PathBuf>,
    /// Override the screenshot width in pixels.
    #[usage(long)]
    capture_width: Option<u32>,
    /// Override the screenshot height in pixels.
    #[usage(long)]
    capture_height: Option<u32>,
}

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Could not open Signal Desk: {source}"))]
    Window { source: plico_desktop::DesktopError },
    #[snafu(display("Could not render the dashboard: {source}"))]
    Render { source: plico_desktop::PreviewError },
    #[snafu(display("Could not save the dashboard: {source}"))]
    Capture { source: plico_desktop::CaptureError },
}

struct Monitor {
    host: usize,
    sidebar_hidden: bool,
    view: usize,
    swap: bool,
    active_only: bool,
    paused: bool,
    opaque: bool,
    settings: PopoverState,
    sample: usize,
    next_sample: Duration,
    traffic: [Traffic; telemetry::HOSTS.len()],
}

impl Default for Monitor {
    fn default() -> Self {
        Self::with_seed(0x802c_753a_5cb3_bb13)
    }
}

impl Monitor {
    fn with_seed(seed: u64) -> Self {
        Self {
            host: 0,
            sidebar_hidden: false,
            view: 0,
            swap: false,
            active_only: false,
            paused: false,
            opaque: false,
            settings: PopoverState::default(),
            sample: 0,
            next_sample: Duration::ZERO,
            traffic: std::array::from_fn(|i| {
                Traffic::new(
                    &telemetry::HOSTS[i],
                    seed ^ (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
                )
            }),
        }
    }
}

impl plico_dev::App for Monitor {
    fn theme(&self) -> Theme {
        Theme {
            surface: 0x101522,
            panel: 0x1b2537,
            control: 0x263148,
            control_hover: 0x35415a,
            control_pressed: 0x3f4e67,
            foreground: 0xf0f5fd,
            muted: 0x9cacca,
            border: 0x314057,
            focus: 0x90f1e2,
            accent: 0x68e3ce,
            selection: 0x314c52,
            on_accent: 0x112220,
            ..Theme::dark()
        }
    }

    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        let [width, height] = cx.viewport_size();
        let compact = width < 1090.;
        let sidebar_available = width >= 900.;
        let sidebar_visible = sidebar_available && !self.sidebar_hidden;
        let sidebar_width = (width * SIDEBAR_RATIO).clamp(136., 172.).round();
        let now = cx.now();
        let reduced = cx.display_preferences().reduce_motion;
        if !self.paused && !reduced {
            if now >= self.next_sample {
                if !self.next_sample.is_zero() {
                    self.sample = self.sample.wrapping_add(1);
                    for (history, host) in self.traffic.iter_mut().zip(&telemetry::HOSTS) {
                        history.advance(host);
                    }
                }
                self.next_sample = now.saturating_add(SAMPLE_PERIOD);
            }
            cx.request_rebuild_at(self.next_sample);
        }
        let sample = Sample::new(self.host, self.sample, &self.traffic[self.host]);
        let glass = LiquidGlass {
            enabled: !self.opaque,
            tint: Color::Literal(0x273952),
            tint_opacity: 0.29,
            rim: 0.55,
            bevel: 16.,
            frost: 0.35,
            refraction: 0.84,
            fallback: Color::Panel,
            ..LiquidGlass::default()
        };
        column(cx).width_percent(100.).height_percent(100.).background(BACKGROUND).clip(true)
            .position(Position::Relative) + |cx|
        {
            atmosphere(cx, width, height);
            row(cx).width_percent(100.).height_percent(100.).padding_right(16)
                .padding_left(if sidebar_visible { 0. } else { 16. }).gap(SIDEBAR_GUTTER) + |cx|
            {
                if sidebar_visible {
                    self.sidebar(cx, glass, sidebar_width);
                }
                column(cx).width(0.).grow(1.).height_percent(100.).padding_top(12).padding_bottom(8).gap(12) + |cx| {
                    self.toolbar(cx, glass, sidebar_available, sidebar_visible);
                    column(cx).key(self.view).height(0.).grow(1.).width_percent(100.).scroll_y(true)
                        .padding_right(8).padding_bottom(12).gap(12) + |cx|
                    {
                        if self.view >= 4 {
                            self.sessions(cx);
                        } else {
                            machine(cx, sample.host, self.paused || reduced, compact);
                            self.dashboard(cx, &sample, compact);
                        }
                    };
                };
            };
            if sidebar_available {
                self.sidebar_toggle(cx, glass, sidebar_width);
            }
        };
    }
}

impl Monitor {
    #[component]
    #[rustfmt::skip]
    fn sidebar(&mut self, cx: &mut UiContext, glass: LiquidGlass, width: f32) {
        let rail = LiquidGlass {
            tint: Color::Literal(0x1b1f2b),
            tint_opacity: 0.86,
            bevel: 7.,
            rim: 0.3,
            fallback: Color::Literal(0x1b1f2b),
            ..glass
        };
        column(cx).width(width).height_percent(100.).shrink(0.).padding_x(6).padding_y(9.).gap(6).material(rail)
        + |cx|
        {
            row(cx).height(22.).gap(8).padding_x(5).align(Align::Center) + |cx| {
                svg(cx, &icons::RADIO).size(15., 15.).color(TRANSMIT);
                text(cx, "Signal Desk").font_size(12).weight(690);
            };
            column(cx).gap(1) + |cx| {
                if nav_item(cx, "Home", &icons::HOUSE, self.view == 5) {
                    self.view = 5;
                }
                if nav_item(cx, "Dashboard", &icons::LAYOUT_GRID, self.view == 0) {
                    self.view = 0;
                }
            };
            column(cx).gap(1) + |cx| {
                pane_heading(cx, "Manage");
                if nav_item(cx, "Servers", &icons::SERVER, self.view == 4) {
                    self.view = 4;
                }
                for (id, (label, icon)) in VIEWS.into_iter().enumerate().skip(1) {
                    cx.keyed(id, |cx| {
                        if nav_item(cx, label, icon, self.view == id) {
                            self.view = id;
                        }
                    });
                }
            };
            column(cx).gap(1) + |cx| {
                pane_heading(cx, "Sessions");
                for (id, host) in telemetry::HOSTS.iter().enumerate() {
                    cx.keyed(id, |cx| {
                        if nav_item(cx, host.name, &icons::MONITOR, self.host == id) {
                            self.host = id;
                            self.view = 0;
                        }
                    });
                }
            };
            column(cx).height(0.).grow(1.).end();
            self.preferences(cx, rail, false);
        };
    }

    #[component]
    #[rustfmt::skip]
    fn sidebar_toggle(&mut self, cx: &mut UiContext, glass: LiquidGlass, width: f32) {
        let (x, y, icon, label) = if self.sidebar_hidden {
            (16., 24., &icons::PANEL_LEFT_OPEN, "Show sidebar")
        } else {
            (width - 32., 8., &icons::PANEL_LEFT_CLOSE, "Hide sidebar")
        };
        let diameter = 24.;
        let radius = diameter / 2.;
        let material = LiquidGlass {
            bevel: 4.,
            refraction: 0.24,
            rim: 0.2,
            ..glass
        };
        column(cx).absolute(x, y).width(diameter).height(diameter).radius(radius).corner_smoothing(0.).clip(true)
            .material(material) + |cx|
        {
            if button(cx).icon(icon).aria_label(label).ghost().width(diameter).min_width(diameter).height(diameter)
                .min_height(diameter).padding(0).radius(radius).corner_smoothing(0.).clicked()
            {
                self.sidebar_hidden = !self.sidebar_hidden;
            }
        };
    }

    #[component]
    #[rustfmt::skip]
    fn toolbar(
        &mut self,
        cx: &mut UiContext,
        glass: LiquidGlass,
        sidebar_available: bool,
        sidebar_visible: bool,
    ) {
        let glass = LiquidGlass {
            tint: TOOLBAR_FACE,
            tint_opacity: 0.65,
            saturation: 0.65,
            frost: 0.5,
            bevel: 5.,
            refraction: 0.28,
            rim: 0.18,
            fallback: TOOLBAR_FACE,
            ..glass
        };
        row(cx).width_percent(100.).height(49.).shrink(0.).gap(8).align(Align::Center).justify(Justify::SpaceBetween)
        + |cx|
        {
            if !sidebar_available {
                if button(cx).icon(&icons::SERVER).text(telemetry::HOSTS[self.host].name).material(glass)
                    .height(TOOLBAR_HEIGHT).min_height(TOOLBAR_HEIGHT).radius(TOOLBAR_HEIGHT / 2.).clicked()
                {
                    self.host ^= 1;
                }
            } else {
                row(cx).gap(7).align(Align::Center).padding_left(if sidebar_visible { 0. } else { 34. }) + |cx| {
                    svg(cx, &icons::MONITOR).size(15., 15.).color(Color::Muted);
                    text(cx, "Sessions").font_size(12).color(Color::Muted);
                    text(cx, "/").font_size(12).color(Color::Muted);
                    text(cx, telemetry::HOSTS[self.host].name).font_size(12).weight(650);
                };
            }
            row(cx).height(TOOLBAR_HEIGHT).padding(SEGMENT_INSET).gap(SEGMENT_GAP).align(Align::Center)
                .position(Position::Relative).radius(TOOLBAR_HEIGHT / 2.).material(glass) + |cx|
            {
                let selected = self.view < VIEWS.len();
                let offset =
                    self.view.min(VIEWS.len() - 1) as f32 * (SEGMENT_WIDTH + SEGMENT_GAP);
                column(cx).absolute(SEGMENT_INSET, SEGMENT_INSET).width(SEGMENT_WIDTH).height(SEGMENT_HEIGHT)
                    .radius(SEGMENT_HEIGHT / 2.).background(SEGMENT_SELECTED).translate(offset, 0.)
                    .opacity(f32::from(selected)).motion(Motion::Quick).shadow(BoxShadow {
                    offset_x: 0.,
                    offset_y: 1.,
                    blur: 2.,
                    spread: 0.,
                    color: Color::Literal(0),
                    opacity: 0.12,
                }).end();
                for (index, (label, _)) in VIEWS.into_iter().enumerate() {
                    cx.keyed(index, |cx| {
                        let selected = self.view == index;
                        let item = button_base(cx, label).width(SEGMENT_WIDTH).height(SEGMENT_HEIGHT)
                            .radius(SEGMENT_HEIGHT / 2.).align(Align::Center).justify(Justify::Center)
                            .selected(selected).motion(Motion::Quick);
                        let item = if !selected {
                            item.hover_background(SEGMENT_HOVERED).pressed_background(SEGMENT_SELECTED)
                        } else {
                            item
                        };
                        if (item + |cx| {
                            text(cx, label).font_size(12).color(if selected {
                                Color::Foreground
                            } else {
                                Color::Literal(0xc0c6ce)
                            });
                        }).response.clicked()
                        {
                            self.view = index;
                        }
                    });
                }
            };
            row(cx).gap(8).align(Align::Center) + |cx| {
                let label = if self.paused { "Resume" } else { "Pause" };
                let icon = if self.paused { &icons::PLAY } else { &icons::PAUSE };
                if button(cx).text(label).icon(icon).height(TOOLBAR_HEIGHT).min_height(TOOLBAR_HEIGHT)
                    .padding_x(11).padding_y(0).radius(TOOLBAR_HEIGHT / 2.).font_size(12).ghost()
                    .material(LiquidGlass {
                    interactive: true,
                    ..glass
                }).clicked()
                {
                    self.paused = !self.paused;
                }
                if !sidebar_visible {
                    self.preferences(cx, glass, true);
                }
            };
        };
    }

    #[component]
    #[rustfmt::skip]
    fn preferences(&mut self, cx: &mut UiContext, glass: LiquidGlass, compact: bool) {
        let label = if compact { "Display settings" } else { "Settings" };
        popover(cx, &mut self.settings, label).trigger_icon(&icons::SETTINGS_2, compact).ghost_trigger().width(292.)
            .over_anchor(true).motion(Motion::Smooth).material(LiquidGlass {
            fallback: Color::Literal(0x222936),
            tint_opacity: 0.,
            ..glass
        }) + |cx|
        {
            column(cx).width_percent(100.).gap(10) + |cx| {
                text(cx, "Display settings").font_size(16).weight(650);
                column(cx).width_percent(100.).height(1.).background(Color::Border).end();
                plico::widgets::liquid::glass_switch_with_description(cx, "Opaque surfaces",
                    "Use solid panels", &mut self.opaque);
                plico::widgets::liquid::glass_switch_with_description(cx, "Pause sample playback",
                    "Freeze the live charts", &mut self.paused);
            };
        };
    }

    #[component]
    #[rustfmt::skip]
    fn sessions(&mut self, cx: &mut UiContext) {
        text(cx, if self.view == 4 { "Servers" } else { "Sessions" }).font_size(28).weight(740);
        card(cx) + |cx| {
            for (index, host) in telemetry::HOSTS.iter().enumerate() {
                cx.keyed(index, |cx| {
                    if nav_item(cx, host.name, &icons::SERVER, index == self.host) {
                        self.host = index;
                        self.view = 0;
                    }
                    text(cx, host.hostname).font_size(11).color(Color::Muted);
                });
            }
        };
    }

    #[component]
    #[rustfmt::skip]
    fn dashboard(&mut self, cx: &mut UiContext, sample: &Sample, compact: bool) {
        match self.view {
            1 => {
                load(cx, sample);
                processor(cx, sample);
                memory(cx, sample, &mut self.swap);
            }
            2 => {
                traffic_pair(cx, sample);
                network(cx, sample, &mut self.active_only);
            }
            3 => disks(cx, sample.host.storage),
            _ if compact => {
                load(cx, sample);
                processor(cx, sample);
                memory(cx, sample, &mut self.swap);
                traffic_pair(cx, sample);
                network(cx, sample, &mut self.active_only);
                disks(cx, sample.host.storage);
            }
            _ => {
                row(cx).width_percent(100.).gap(GAP).align(Align::Start) + |cx| {
                    column(cx).width(0.).grow(1.).gap(GAP) + |cx| {
                        load(cx, sample);
                        processor(cx, sample);
                        memory(cx, sample, &mut self.swap);
                        disks(cx, sample.host.storage);
                    };
                    column(cx).width(0.).grow(1.).gap(GAP) + |cx| {
                        traffic_pair(cx, sample);
                        network(cx, sample, &mut self.active_only);
                    };
                };
            }
        }
    }
}

#[component]
#[rustfmt::skip]
fn atmosphere(cx: &mut UiContext, width: f32, height: f32) {
    shader(cx, &ATMOSPHERE).absolute(0., 0.).width(width).height(height);
}

#[component]
#[rustfmt::skip]
fn nav_item(cx: &mut UiContext, label: &str, icon: &plico::SvgAsset, selected: bool) -> bool {
    let item = button_base(cx, label).width_percent(100.).height(27.).radius(6.).selected(selected).padding_x(7).gap(7)
        .align(Align::Center).hover_background(Color::ControlHover);
    let item = if selected { item.background(Color::Literal(0x34394a)) } else { item };
    (item + |cx| {
        svg(cx, icon).size(14., 14.).color(TRANSMIT);
        text(cx, label).font_size(12).weight(if selected { 650 } else { 450 });
    }).clicked()
}

#[component]
fn pane_heading(cx: &mut UiContext, label: &str) {
    row(cx)
        .height(18.)
        .padding_x(6)
        .align(Align::End)
        .children(|cx| {
            text(cx, label)
                .font_size(10)
                .weight(620)
                .color(Color::Muted);
        });
}

#[component]
fn section_label(cx: &mut UiContext, label: &str) {
    text(cx, label)
        .font_size(10)
        .weight(650)
        .color(Color::Muted);
}

#[component]
#[rustfmt::skip]
fn machine(cx: &mut UiContext, host: &Host, paused: bool, compact: bool) {
    column(cx).width_percent(100.).padding_x(2).gap(12).shrink(0.) + |cx| {
        row(cx).width_percent(100.).align(Align::Center).justify(Justify::SpaceBetween) + |cx| {
            text(cx, host.name).font_size(28).weight(740);
            row(cx).gap(6).align(Align::Center).padding_x(10).height(26.).radius(13.).background(Color::Control)
            + |cx|
            {
                column(cx).width(6.).height(6.).radius(3.)
                    .background(if paused { Color::Muted } else { ACCENT }).end();
                text(cx, if paused { "Demo paused" } else { "Demo data" }).font_size(11).weight(600);
            };
        };
        row(cx).width_percent(100.).gap(12).wrap(compact) + |cx| {
            for (index, (key, value)) in [
                ("HOSTNAME", host.hostname),
                ("OS", host.os),
                ("UPTIME", host.uptime),
                ("ADDRESS", host.address),
            ].into_iter().enumerate()
            {
                cx.keyed(index, |cx| {
                    column(cx).width(118.).grow(1.).gap(3) + |cx| {
                        section_label(cx, key);
                        text(cx, value).font_size(12).weight(580).ellipsis(true);
                    };
                });
            }
        };
    };
}

fn card<'a, 'e>(cx: &'a mut UiContext<'e>) -> plico::Layout<'a, 'e> {
    column(cx)
        .width_percent(100.)
        .shrink(0.)
        .padding(13)
        .gap(9)
        .radius(RADIUS)
        .background(CARD)
        .background_alpha(0.94)
        .border(1., Color::Border)
}

#[component]
#[rustfmt::skip]
fn heading(cx: &mut UiContext, label: &str, icon: &plico::SvgAsset) {
    row(cx).gap(7).align(Align::Center) + |cx| {
        svg(cx, icon).size(13., 13.).color(Color::Muted);
        text(cx, label).font_size(12).weight(650);
    };
}

#[component]
#[rustfmt::skip]
fn readout(cx: &mut UiContext, value: &str, unit: &str) {
    row(cx).gap(5).align(Align::Baseline) + |cx| {
        text(cx, value).font_size(27).weight(740);
        text(cx, unit).font_size(11).weight(550).color(Color::Muted);
    };
}

#[component]
#[rustfmt::skip]
fn meter(cx: &mut UiContext, portions: &[(f32, Color)]) {
    row(cx).height(7.).width_percent(100.).radius(3.5).clip(true).background(TRACK).gap(2) + |cx| {
        for (index, &(part, color)) in portions.iter().enumerate() {
            if part > 0. {
                column(cx).key(index).width_percent(part.clamp(0., 100.)).height_percent(100.).radius(3.5)
                    .background(color).end();
            }
        }
    };
}

#[component]
#[rustfmt::skip]
fn facts(cx: &mut UiContext, entries: &[(&str, String, Color)]) {
    row(cx).gap(8).width_percent(100.) + |cx| {
        for (index, (label, value, color)) in entries.iter().enumerate() {
            cx.keyed(index, |cx| {
                column(cx).width(0.).grow(1.).gap(3) + |cx| {
                    row(cx).gap(5).align(Align::Center) + |cx| {
                        column(cx).width(5.).height(5.).radius(2.).background(*color).end();
                        text(cx, *label).font_size(10).color(Color::Muted).ellipsis(true);
                    };
                    text(cx, value.as_str()).font_size(12).weight(620).ellipsis(true);
                };
            });
        }
    };
}

#[component]
#[rustfmt::skip]
fn load(cx: &mut UiContext, sample: &Sample) {
    card(cx) + |cx| {
        heading(cx, "Load average", &icons::ACTIVITY);
        readout(cx, &format!("{:.2}", sample.load), "1 min");
        facts(
            cx,
            &[
                ("5 MIN", format!("{:.2}", sample.load * 1.13), ACCENT),
                ("15 MIN", format!("{:.2}", sample.load * 1.25), RECEIVE),
                ("PROCESSES", format!("{} / 518", sample.running), TRANSMIT),
            ],
        );
    };
}

#[component]
#[rustfmt::skip]
fn processor(cx: &mut UiContext, sample: &Sample) {
    let total = sample.cpu_user + sample.cpu_system;
    card(cx) + |cx| {
        heading(cx, "Processor", &icons::CPU);
        readout(cx, &format!("{total:.1}"), "%");
        meter(cx, &[(sample.cpu_user, ACCENT), (sample.cpu_system, RECEIVE)]);
        facts(
            cx,
            &[
                ("USER", format!("{:.1}%", sample.cpu_user), ACCENT),
                ("SYSTEM", format!("{:.1}%", sample.cpu_system), RECEIVE),
                ("WAIT", "0.0%".into(), TRANSMIT),
                ("IDLE", format!("{:.1}%", 100. - total), TRACK),
            ],
        );
    };
}

#[component]
#[rustfmt::skip]
fn memory(cx: &mut UiContext, sample: &Sample, swap: &mut bool) {
    let (used, cache, capacity) = if *swap { (sample.swap, 0., 4.) } else { (sample.memory, 1.8, sample.host.memory) };
    let pc = used / capacity * 100.;
    card(cx) + |cx| {
        row(cx).width_percent(100.).align(Align::Center).justify(Justify::SpaceBetween) + |cx| {
            heading(cx, if *swap { "Swap memory" } else { "Memory" }, &icons::MEMORY_STICK);
            let mut index = usize::from(*swap);
            if segmented(cx, &mut index, &["RAM", "Swap"]) {
                *swap = index == 1;
            }
        };
        readout(cx, &format!("{pc:.1}"), "%");
        meter(cx, &[(pc, TRANSMIT), (cache / capacity * 100., RECEIVE)]);
        facts(
            cx,
            &[
                ("USED", format!("{used:.1} GB"), TRANSMIT),
                ("CACHE", format!("{cache:.1} GB"), RECEIVE),
                (
                    "AVAILABLE",
                    format!("{:.1} GB", capacity - used - cache),
                    ACCENT,
                ),
            ],
        );
    };
}

#[component]
#[rustfmt::skip]
fn disks(cx: &mut UiContext, storage: &[Storage]) {
    card(cx) + |cx| {
        heading(cx, "Volumes", &icons::HARD_DRIVE);
        for (index, disk) in storage.iter().enumerate() {
            cx.keyed(index, |cx| {
                column(cx).width_percent(100.).gap(4) + |cx| {
                    row(cx).width_percent(100.).gap(8).align(Align::Baseline) + |cx| {
                        text(cx, disk.name).font_size(11).weight(580).grow(1.).ellipsis(true);
                        text(cx, disk.capacity).font_size(10).color(Color::Muted);
                        text!(cx, "{:.0}%", disk.used).font_size(11).weight(620).width(30.);
                    };
                    meter(cx, &[(disk.used, TRANSMIT)]);
                };
            });
        }
    };
}

#[component]
#[rustfmt::skip]
fn traffic_pair(cx: &mut UiContext, sample: &Sample) {
    row(cx).width_percent(100.).gap(GAP).align(Align::Start) + |cx| {
        traffic(cx, "Receive", &icons::ARROW_DOWN, &sample.receive, RECEIVE, "#73c9ff");
        traffic(cx, "Transmit", &icons::ARROW_UP, &sample.transmit, TRANSMIT, "#b5a0fa");
    };
}

#[component]
#[rustfmt::skip]
fn traffic(
    cx: &mut UiContext,
    label: &str,
    icon: &plico::SvgAsset,
    samples: &[f32],
    color: Color,
    stroke: &str,
) {
    column(cx).width(0.).grow(1.).shrink(0.).padding(12).gap(6).radius(RADIUS).background(CARD).background_alpha(0.94)
        .border(1., Color::Border) + |cx|
    {
        row(cx).gap(5).align(Align::Center) + |cx| {
            svg(cx, icon).size(13., 13.).color(color);
            text(cx, label).font_size(12).weight(650);
        };
        readout(cx, &format!("{:.1}", samples.last().copied().unwrap_or(0.)), "KB/s");
        svg_source(cx, traffic_path(samples, stroke)).size(240., 57.).width_percent(100.);
        text(cx, "LAST 60 SECONDS").font_size(9).weight(600).color(Color::Muted);
    };
}

#[component]
#[rustfmt::skip]
fn network(cx: &mut UiContext, sample: &Sample, active_only: &mut bool) {
    card(cx) + |cx| {
        row(cx).width_percent(100.).align(Align::Center).justify(Justify::SpaceBetween).gap(6) + |cx| {
            heading(cx, "Network interfaces", &icons::NETWORK);
            if button(cx).text(if *active_only { "Show all" } else { "Active only" }).ghost().height(31.)
                .font_size(11).clicked()
            {
                *active_only = !*active_only;
            }
        };
        for (index, &(name, received, transmitted)) in sample.host.interfaces.iter().enumerate()
        {
            let (received, transmitted) = if index == 0 {
                (sample.receive[59], sample.transmit[59])
            } else {
                (received, transmitted)
            };
            if *active_only && received == 0. && transmitted == 0. {
                continue;
            }
            cx.keyed(index, |cx| {
                column(cx).width_percent(100.).padding_y(4).gap(4).border(0., Color::Border) + |cx| {
                    row(cx).width_percent(100.).gap(8).align(Align::Center) + |cx| {
                        text(cx, name).font_size(11).weight(620).grow(1.);
                        text!(cx, "↓ {received:.1}").font_size(11)
                            .color(if received > 0. { RECEIVE } else { Color::Muted });
                        text!(cx, "↑ {transmitted:.1} KB/s").font_size(11).color(
                            if transmitted > 0. { TRANSMIT } else { Color::Muted });
                    };
                    meter(cx, &[(received * 0.72, RECEIVE), (transmitted * 0.72, TRANSMIT)]);
                };
            });
        }
    };
}

fn main() -> Result<(), Error> {
    let options = Options::parse();
    if let Some(path) = options.capture {
        return PreviewScenario::<()> {
            viewport: PreviewViewport {
                width: options.capture_width.unwrap_or(DEFAULT_WINDOW.0),
                height: options.capture_height.unwrap_or(DEFAULT_WINDOW.1),
                scale: 1.,
            },
            ..Default::default()
        }
        .render(|()| NativeApp(Monitor::default()))
        .context(RenderSnafu)?
        .write_png(path)
        .context(CaptureSnafu);
    }

    plico_desktop::run(
        Monitor::with_seed(RandomState::new().hash_one("signal-desk")),
        WindowOptions::new()
            .title("Signal Desk")
            .initial_inner_size(DEFAULT_WINDOW.0, DEFAULT_WINDOW.1)
            .min_inner_size(760, 560),
    )
    .context(WindowSnafu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico_desktop::testing::Tester;

    #[test]
    #[ignore = "requires a working wgpu adapter"]
    fn dashboard_renders_ambient_glow_at_double_scale() {
        let frame = PreviewScenario::<()> {
            viewport: PreviewViewport {
                width: 1280,
                height: 752,
                scale: 2.,
            },
            ..Default::default()
        }
        .render(|()| NativeApp(Monitor::default()))
        .unwrap();
        assert_eq!([frame.width(), frame.height()], [2560, 1504]);
        let pixel = (128 * frame.width() as usize + 1560) * 4;
        assert!(frame.rgba()[pixel + 2] > 45, "top halo should be visible");
    }

    #[test]
    fn sidebar_can_be_hidden_and_reopened_from_the_work_area() {
        let mut driver = Tester::new(NativeApp(Monitor::default()), 1280, 720).unwrap();
        let shown = driver.find("Dashboard").unwrap();
        let assert_circle = |driver: &Tester<NativeApp<Monitor>>, label| {
            let [x0, y0, x1, y1] = driver.find(label).unwrap().bounds;
            assert_eq!([x1 - x0, y1 - y0], [24., 24.]);
        };
        assert_circle(&driver, "Hide sidebar");
        driver.click("Hide sidebar").unwrap();
        assert!(driver.view().0.sidebar_hidden);
        assert!(driver.find("Dashboard").is_err());
        assert!(driver.find("Display settings").is_ok());
        assert_circle(&driver, "Show sidebar");
        driver.click("Show sidebar").unwrap();
        assert!(!driver.view().0.sidebar_hidden);
        assert!(driver.find("Dashboard").is_ok());
        assert_eq!(driver.view().0.host, 0);
        assert_eq!(driver.find("Dashboard").unwrap().bounds, shown.bounds);
    }

    #[test]
    fn monitor_switches_sessions_filters_interfaces_and_pauses() {
        let mut driver = Tester::new(
            NativeApp(Monitor::default()),
            DEFAULT_WINDOW.0,
            DEFAULT_WINDOW.1,
        )
        .unwrap();
        let [left, top, right, bottom] = driver.find("Dashboard").unwrap().bounds;
        assert!((right - left - 136.).abs() < 1.);
        assert!((bottom - top - 27.).abs() < 1.);
        assert!(driver.has_text("studio-m2.local"));
        driver.click("Edge Node 01").unwrap();
        assert!(driver.has_text("edge-01.lab"));
        driver.click("Home").unwrap();
        assert_eq!(driver.view().0.view, 5);
        driver.click("Servers").unwrap();
        assert_eq!(driver.view().0.view, 4);
        driver.click("Dashboard").unwrap();
        assert_eq!(driver.view().0.view, 0);
        driver.click("Swap").unwrap();
        assert!(driver.view().0.swap);
        driver.click("RAM").unwrap();
        assert!(!driver.view().0.swap);

        driver.click("Active only").unwrap();
        assert!(!driver.has_text("docker0"));
        driver.click("Show all").unwrap();
        assert!(driver.has_text("docker0"));

        driver.advance(Duration::from_secs(1)).unwrap();
        let sample = driver.view().0.sample;
        let traffic = |monitor: &Monitor| {
            Sample::new(monitor.host, monitor.sample, &monitor.traffic[monitor.host]).receive
        };
        let before_pause = traffic(&driver.view().0);
        assert!(sample > 0);
        driver.click("Pause").unwrap();
        driver.advance(Duration::from_secs(3)).unwrap();
        assert_eq!(driver.view().0.sample, sample);
        assert_eq!(traffic(&driver.view().0), before_pause);
        driver.click("Resume").unwrap();
        driver.advance(Duration::from_secs(1)).unwrap();
        assert!(driver.view().0.sample > sample);
        assert_ne!(traffic(&driver.view().0), before_pause);
    }

    #[test]
    fn compact_toolbar_and_display_preferences_work() {
        let mut driver = Tester::new(NativeApp(Monitor::default()), 760, 560).unwrap();
        driver.click("Network").unwrap();
        assert!(driver.has_text("Network interfaces"));
        assert!(!driver.has_text("Processor"));
        driver.click("Display settings").unwrap();
        driver.advance(Duration::from_millis(250)).unwrap();
        driver.click("Opaque surfaces").unwrap();
        assert!(driver.view().0.opaque);
        driver.click("Opaque surfaces").unwrap();
        assert!(!driver.view().0.opaque);
        assert!(driver.view().0.settings.is_open());
        assert!(driver.placed_nodes().iter().any(|node| {
            node.paint.text.as_deref() == Some("Use solid panels") && node.world_opacity == 1.
        }));
        driver.key("Escape").unwrap();
        assert!(!driver.view().0.settings.is_open());
        driver.click("Storage").unwrap();
        assert!(driver.has_text("Volumes"));
        assert!(!driver.has_text("Network interfaces"));
    }

    #[test]
    fn settings_content_reveals_and_retracts_with_the_popup() {
        let mut driver = Tester::new(NativeApp(Monitor::default()), 1280, 752).unwrap();
        let title_opacity = |driver: &Tester<NativeApp<Monitor>>| {
            driver
                .placed_nodes()
                .iter()
                .find(|node| node.paint.text.as_deref() == Some("Display settings"))
                .map(|node| node.world_opacity)
        };
        assert_eq!(title_opacity(&driver), None);
        driver.click("Settings").unwrap();
        assert_eq!(title_opacity(&driver), Some(0.));
        driver.advance(Duration::from_millis(700)).unwrap();
        assert_eq!(title_opacity(&driver), Some(1.));
        driver.key("Escape").unwrap();
        assert!(!driver.view().0.settings.is_open());
        driver.advance(Duration::from_millis(120)).unwrap();
        assert!(title_opacity(&driver).is_some_and(|alpha| alpha < 1.));
        driver.advance(Duration::from_millis(700)).unwrap();
        assert_eq!(title_opacity(&driver), None);
    }

    #[test]
    fn toolbar_selection_pill_moves_between_tabs() {
        let mut driver = Tester::new(NativeApp(Monitor::default()), 760, 560).unwrap();
        let pill_x = |driver: &Tester<NativeApp<Monitor>>| {
            let pill = driver
                .placed_nodes()
                .iter()
                .find(|node| {
                    node.rect[2] - node.rect[0] == SEGMENT_WIDTH
                        && node.rect[3] - node.rect[1] == SEGMENT_HEIGHT
                        && !node.paint.shadows.is_empty()
                })
                .expect("selected tab indicator");
            pill.world_transform.as_coeffs()[4]
        };
        let start = pill_x(&driver);
        driver.click("Compute").unwrap();
        assert_eq!(driver.view().0.view, 1);
        let duration = Duration::from_millis(Motion::Quick.duration_ms().into());
        driver.advance(duration / 2).unwrap();
        let midpoint = pill_x(&driver);
        driver.advance(duration).unwrap();
        let end = pill_x(&driver);
        assert!(
            start < midpoint && midpoint < end,
            "{start}, {midpoint}, {end}"
        );
        assert_eq!(end - start, f64::from(SEGMENT_WIDTH + SEGMENT_GAP));
    }
}
