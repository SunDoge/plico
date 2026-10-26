//! A routed desktop workspace shared by the FLTK, Vello and Skia backends.
mod animation;
mod color_icons;
mod icons;
use animation::Animations;
use plico_core::{DeclareView, Router, TextBuffer, Theme, prelude::*};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, plico_core::Route)]
pub enum Page {
    #[default]
    #[route(icon = color_icons::HOME)]
    Overview,
    #[route(icon = color_icons::CHART_MULTIPLE)]
    Activity,
    #[route(icon = color_icons::SETTINGS)]
    Preferences,
}
pub struct Settings {
    pub router: Router<Page>,
    pub dark: bool,
    pub advanced: bool,
    pub notifications: bool,
    pub name: TextBuffer,
    pub saved: String,
    pub saves: usize,
    pub compact: bool,
    pub inspector: bool,
    pub reduced_motion: bool,
    pub selected_project: u64,
    animations: Animations,
}
impl Default for Settings {
    fn default() -> Self {
        let name = TextBuffer::new();
        name.set("Plico workspace".to_owned().into());
        Self {
            router: Router::default(),
            dark: false,
            advanced: false,
            notifications: true,
            name,
            saved: String::new(),
            saves: 0,
            compact: false,
            inspector: false,
            reduced_motion: false,
            selected_project: 1,
            animations: Animations::default(),
        }
    }
}
impl Settings {
    pub fn progress(&self) -> f32 {
        self.animations.progress()
    }
    pub fn sidebar_width(&self) -> f32 {
        self.animations.sidebar()
    }
    pub fn inspector_width(&self) -> f32 {
        self.animations.inspector()
    }
    pub fn animation_active(&self) -> bool {
        self.animations.active()
    }
}
impl DeclareView for Settings {
    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        self.animations.advance(cx.now());
        if self.router.current() != Page::Overview {
            self.animations.cancel();
        }
        let narrow = cx.viewport_width() < 760.;
        self.animations.targets(
            self.compact || narrow,
            self.inspector
                && self.router.current() == Page::Overview
                && cx.viewport_width() >= 1000.,
            self.reduced_motion,
        );
        row(cx).width_percent(100.).height_percent(100.).align(Align::Stretch).background(Color::Surface) + |cx| {
            self.sidebar(cx);
            column(cx).width(0.).grow(1.).min_width(0.).padding(20).gap(18) + |cx| {
                row(cx).gap(10).wrap(true) + |cx| {
                    text(cx, "PLICO / WORKSPACE").font_size(12).color(Color::Muted).grow(1.);
                    if button(cx).icon(&icons::ARROW_LEFT).aria_label("Back")
                        .disabled(!self.router.can_back()).clicked()
                    {
                        self.router.back();
                    }
                    if button(cx).icon(&icons::ARROW_RIGHT).aria_label("Forward")
                        .disabled(!self.router.can_forward()).clicked()
                    {
                        self.router.forward();
                    }
                    checkbox(cx, "Dark theme", &mut self.dark);
                };
                row(cx).gap(12).wrap(true) + |cx| {
                    column(cx).width(0.).grow(1.).min_width(120.).gap(6) + |cx| {
                        text(cx, self.router.current().label()).font_size(28).weight(700);
                        text(cx, match self.router.current() {
                            Page::Overview => "A native workspace. Flex, Grid and retained controls working together.",
                            Page::Activity => "A keyed list, a fixed header and a native scroll viewport.",
                            Page::Preferences => "Application state survives navigation. The editor remains a native control.",
                        }).font_size(13).color(Color::Muted);
                    };
                    if self.router.current() == Page::Overview
                        && button(cx).icon(&icons::PANEL_RIGHT).text("Details").clicked()
                    {
                        self.inspector = !self.inspector;
                    }
                };
                match self.router.current() {
                    Page::Overview => self.overview(cx),
                    Page::Activity => self.activity(cx),
                    Page::Preferences => {
                        column(cx).height(0.).grow(1.).scroll_y(true).gap(18)
                            .children(|cx| self.preferences(cx)).inner
                    }
                }
                row(cx).gap(8) + |cx| {
                    text(cx, "Shared Plico view · Taffy geometry").font_size(12).color(Color::Muted)
                        .grow(1.);
                    text(cx, if self.animations.active() { "Animating" } else { "Idle · event driven" }).font_size(12).color(Color::Muted);
                };
            };
        };
        // Request only finite active frames; navigating offscreen stops its work.
        if self.animations.active() {
            cx.request_animation_frame();
        }
    }
}
impl Settings {
    #[component]
    #[rustfmt::skip]
    fn sidebar(&mut self, cx: &mut UiContext) {
        let expanded = self.animations.sidebar() > 160.;
        column(cx).width(self.animations.sidebar()).shrink(0.).padding(12).gap(18).clip(true).background(Color::Panel)
        + |cx|
        {
            row(cx).gap(8).height(38.) + |cx| {
                svg(cx, &color_icons::APPS).size(26., 26.);
                if expanded {
                    text(cx, "plico").font_size(23).weight(700);
                }
            };
            column(cx).gap(8) + |cx| {
                nav_menu(cx, &mut self.router);
            };
            column(cx).height(0.).grow(1.) + |_cx| {};
            if button(cx).icon(&icons::PANEL_LEFT_CLOSE).aria_label("Toggle sidebar").height(36.)
                .width_percent(100.).text(if expanded { "Collapse" } else { "" }).clicked()
            {
                self.compact = !self.compact;
            }
            if expanded {
                text(cx, "Backend playground").font_size(12).color(Color::Muted);
            }
        };
    }
    #[component]
    #[rustfmt::skip]
    fn overview(&mut self, cx: &mut UiContext) {
        let content = cx.viewport_width() - self.animations.sidebar() - 40.;
        column(cx).height(0.).grow(1.).scroll_y(true).gap(18) + |cx| {
            let tracks = if content >= 660. {
                3
            } else if content >= 400. { 2 } else { 1 };
            grid(cx).grid_columns(std::iter::repeat_n(Track::Fr(1.), tracks)).gap(12) + |cx| {
                stat(cx, "PROJECTS", "12", Some("4 workspaces active"));
                stat(cx, "STORAGE", "2.4 GB", Some("Local demo data"));
                stat(cx, "SYNC HEALTH", "98.6%", Some("Simulated workload"));
            };
            row(cx).gap(16).align(Align::Stretch).min_height(286.) + |cx| {
                column(cx).width(0.).grow(1.).min_width(0.).padding(18).gap(14).background(Color::Panel)
                    .border(1., Color::Border).radius(8.) + |cx|
                {
                    text(cx, "Projects").font_size(18).weight(650);
                    text(cx, "Select a project; open Details to test the animated split pane.")
                        .font_size(13).color(Color::Muted);
                    for (id, title, note) in [
                        (1u64, "Design system", "Components & typography"),
                        (2, "Desktop client", "Native backend integration"),
                        (3, "Embedded panel", "A smaller rendering target"),
                    ] {
                        cx.keyed(id, |cx| {
                            row(cx).gap(10).padding_y(8) + |cx| {
                                icon(
                                    cx,
                                    if id == 3 {
                                        &color_icons::PUZZLE_PIECE
                                    } else {
                                        &color_icons::APPS
                                    },
                                );
                                column(cx).width(0.).grow(1.).gap(4) + |cx| {
                                    text(cx, title).weight(600);
                                    text(cx, note).font_size(12).color(Color::Muted);
                                };
                                if button(cx)
                                    .text(if self.selected_project == id { "Selected" } else { "Open" })
                                    .clicked()
                                {
                                    self.selected_project = id;
                                    self.inspector = true;
                                }
                            };
                        });
                    }
                };
                if self.animations.inspector() > 0.
                    || self.inspector && cx.viewport_width() >= 1000.
                {
                    column(cx).width(self.animations.inspector()).shrink(0.).clip(true).background(Color::Panel)
                        .radius(8.) + |cx|
                    {
                        column(cx).width_percent(100.).padding(18).gap(14) + |cx| {
                            text(cx, "Project details").font_size(18).weight(650);
                            text!(cx, "Workspace 0{}", self.selected_project).weight(600);
                            text(cx, "The pane width is sampled from the same injected clock on both backends.")
                                .font_size(13).color(Color::Muted);
                            text(cx, "Status").weight(600);
                            text(cx, "Ready to sync").color(Color::Accent);
                            if button(cx).text("Close details").clicked() {
                                self.inspector = false;
                            }
                        };
                    };
                }
            };
            column(cx).padding(18).gap(12).background(Color::Panel).border(1., Color::Border).radius(8.) + |cx| {
                row(cx).gap(10).wrap(true) + |cx| {
                    text(cx, "Sync preview").font_size(18).weight(650).grow(1.);
                    if button(cx).icon(&icons::PLAY).text("Run preview").primary().clicked() {
                        self.animations.start(self.reduced_motion);
                    }
                    if button(cx).icon(&icons::PAUSE).text("Stop")
                        .disabled(!self.animations.progress_running()).clicked()
                    {
                        self.animations.cancel();
                    }
                };
                text(cx, "A finite, simulated job. No network traffic.").font_size(13).color(Color::Muted);
                progress_bar(cx, self.animations.progress());
                text!(
                    cx,
                    "{}% complete",
                    (self.animations.progress() * 100.).round() as u32
                ).font_size(13).color(Color::Muted);
            };
        };
    }
    #[component]
    #[rustfmt::skip]
    fn activity(&mut self, cx: &mut UiContext) {
        let reverse = cx.persisted_state("reverse-order", || false);
        row(cx).gap(12) + |cx| {
            text(cx, "30 local events").color(Color::Muted).grow(1.);
            if button(cx).text("Reverse order").clicked() {
                reverse.update(|value| *value = !*value);
            }
        };
        let reverse = reverse.get();
        // Share the actual column sizes across rows, not independent intrinsic
        // fr tracks. Reserve a small gutter for native or overlay scrollbars.
        let available =
            (cx.viewport_width() - self.animations.sidebar() - 40. - 24. - 24. - 20. - 84.).max(0.);
        let tracks = [
            Track::Px(available * 0.58),
            Track::Px(available * 0.42),
            Track::Px(84.),
        ];
        activity_row(cx, "EVENT", "WORKSPACE", "STATE", true, tracks);
        column(cx).height(0.).grow(1.).scroll_y(true).gap(4) + |cx| {
            for index in 0..30u64 {
                let id = if reverse { 29 - index } else { index };
                cx.keyed(id, |cx| {
                    activity_row(cx, format_args!("Sync checkpoint {:02}", id + 1),
                        if id % 2 == 0 { "Desktop client" } else { "Design system" },
                        if id % 5 == 0 { "Queued" } else { "Complete" }, false, tracks);
                });
            }
        };
    }
    #[component]
    #[rustfmt::skip]
    fn preferences(&mut self, cx: &mut UiContext) {
        column(cx).padding(18).gap(14).background(Color::Panel).border(1., Color::Border).radius(8.) + |cx| {
            text(cx, "Profile / 工作区").font_size(18).weight(600);
            text(cx, "Workspace name");
            text_input(cx, "Workspace name", &self.name).placeholder("Enter a name").width_percent(100.)
                .height(36.);
            text!(cx, "Current name: {}", self.name.text().unwrap_or_default()).color(Color::Muted);
            checkbox(cx, "Enable notifications", &mut self.notifications);
            checkbox(cx, "Show advanced options", &mut self.advanced);
            if self.advanced {
                column(cx).gap(8).padding(12).background(Color::Control) + |cx| {
                    text(cx, "Advanced options").weight(600);
                    text(cx, "This branch is mounted on demand. Toggle it to test removal and recreation.");
                    row(cx).gap(8) + |cx| {
                        button(cx).text("Nested action").disabled(true);
                        button(cx).text("Another action").disabled(true);
                    };
                };
            }
        };
        row(cx).gap(12).wrap(true) + |cx| {
            checkbox(cx, "Dark theme", &mut self.dark);
            button(cx).text("Disabled action").disabled(true);
        };
        row(cx).gap(10).wrap(true) + |cx| {
            if button(cx).icon(&icons::CHECK).text("Save changes").primary().clicked() {
                self.saves += 1;
                self.saved = format!(
                    "Saved {}: {}",
                    self.saves,
                    self.name.text().unwrap_or_default()
                );
            }
            if button(cx).text("Reset name").clicked() {
                self.name.set("Plico workspace".to_owned().into());
            }
        };
        text(cx, if self.saved.is_empty() { "No changes saved yet" } else { &self.saved }).color(Color::Muted);
        checkbox(cx, "Reduce motion", &mut self.reduced_motion);
    }
}
#[component]
#[rustfmt::skip]
fn progress_bar(cx: &mut UiContext, value: f32) {
    row(cx).height(8.).width_percent(100.).background(Color::Control).radius(4.).clip(true) + |cx| {
        column(cx).height(8.).width_percent(value.clamp(0., 1.) * 100.).background(Color::Accent).radius(4.) + |_cx| {};
    };
}
#[component]
#[rustfmt::skip]
fn activity_row(
    cx: &mut UiContext,
    event: impl std::fmt::Display,
    project: &str,
    state: &str,
    heading: bool,
    tracks: [Track; 3],
) {
    grid(cx).grid_columns(tracks).gap(12).padding(12).shrink(0.)
        .background(if heading { Color::Control } else { Color::Panel }) + |cx|
    {
        text!(cx, "{event}").font_size(13).weight(if heading { 600 } else { 400 }).ellipsis(true);
        text(cx, project).font_size(13).color(Color::Muted).ellipsis(true);
        text(cx, state).font_size(13).color(if heading { Color::Muted } else { Color::Accent });
    };
}
#[cfg(any(feature = "vello", feature = "skia"))]
impl plico_runtime::View for Settings {
    fn show(&mut self, cx: &mut UiContext) {
        DeclareView::show(self, cx);
    }
    fn theme(&self) -> Theme {
        DeclareView::theme(self)
    }
}
