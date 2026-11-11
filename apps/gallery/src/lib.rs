mod black_hole;
mod calendar;
mod color_icons;
pub mod data_table;
mod effects;
pub mod i18n;
pub mod icons;
#[cfg(all(feature = "host", not(target_arch = "wasm32")))]
pub mod images;
mod legacy_icons;
mod shader_playground;
pub mod tree_view;
use plico::{Align, Color, Justify, Motion, SpringOptions, Theme, UiContext, component};
use plico_dev::App;
mod component_kit;
mod components;
pub mod game2048;
pub mod glass;
mod layout_demo;
pub mod markdown_chat;
pub mod mines;
mod patterns;
mod sidebar;
pub mod workspace;
use components::icon_button;
use plico::widgets::{
    avatar, badge, button, checkbox, collapsible, column, panel, progress, row, ruby_text,
    segmented, separator, settings_row, settings_section, switch, text, virtual_list,
};

#[cfg(not(target_arch = "wasm32"))]
static AURORA: plico::ShaderAsset = plico::shader_asset!("../assets/shaders/aurora.wgsl")
    .with_sksl(include_str!("../assets/shaders/aurora.sksl"));

static LOGO: plico::SvgAsset = plico::svg_asset!("../assets/plico.svg");

#[derive(
    Default, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, plico::Route,
)]
pub enum Page {
    #[default]
    #[route(label = "Workspace", icon = color_icons::APPS)]
    Workspace,
    #[route(label = "Layout", icon = color_icons::HOME)]
    Layout,
    #[route(label = "Controls", icon = color_icons::SETTINGS)]
    Controls,
    #[route(label = "Motion", icon = color_icons::DESIGN_IDEAS)]
    Motion,
    #[route(label = "Lists", icon = color_icons::TEXT_BULLET_LIST_SQUARE)]
    Lists,
    #[route(label = "Data table", icon = color_icons::TEXT_BULLET_LIST_SQUARE)]
    DataTable,
    #[route(label = "Tree navigation", icon = color_icons::HOME)]
    TreeView,
    #[route(label = "2048", icon = color_icons::PUZZLE_PIECE)]
    Game2048,
    #[route(label = "Mines", icon = color_icons::PUZZLE_PIECE)]
    Mines,
    #[route(label = "Markdown chat", icon = color_icons::CHAT)]
    Markdown,
    #[route(label = "Glass", icon = color_icons::IMAGE)]
    Glass,
    #[route(label = "Calendar", icon = color_icons::CALENDAR)]
    Calendar,
    #[route(label = "Localization", icon = color_icons::CHAT)]
    Localization,
    #[route(label = "Images", icon = color_icons::IMAGE)]
    Images,
    #[route(label = "Black hole", icon = color_icons::IMAGE)]
    BlackHole,
    #[route(label = "Patterns", icon = color_icons::SETTINGS)]
    Patterns,
    #[route(label = "Effects", icon = color_icons::DESIGN_IDEAS)]
    Effects,
    #[route(label = "Shader playground", icon = color_icons::DESIGN_IDEAS)]
    ShaderPlayground,
    #[route(label = "Component kit", icon = color_icons::DESIGN_IDEAS)]
    ComponentKit,
}

/// A spring-driven demo chip on the Motion page. The framework owns the
/// transient channel; this app state only keeps the serializable preference.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpringDemo {
    out: bool,
}

#[derive(Default, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Gallery {
    pub language: i18n::Language,
    /// Whether the gravitational-lens overlay distorts every page.
    pub black_hole: bool,
    /// The desktop-patterns page's local demo state.
    pub patterns: patterns::Patterns,
    pub effects: effects::Effects,
    pub shader_playground: shader_playground::Playground,
    pub component_kit: component_kit::ComponentKit,
    #[serde(skip)]
    pub spring: SpringDemo,
    #[cfg(all(feature = "host", not(target_arch = "wasm32")))]
    #[serde(skip)]
    pub images: images::Images,
    pub game2048: game2048::Game,
    pub mines: mines::Mines,
    pub markdown_chat: markdown_chat::Chat,
    pub glass: glass::Glass,
    pub count: u32,
    pub notifications: bool,
    pub autosave: bool,
    pub dark: bool,
    pub large: bool,
    pub expanded: bool,
    pub router: plico::Router<Page>,
    pub alignment: u8,
    pub wrapped: bool,
    pub compact: bool,
    pub sidebar_collapsed: bool,
    pub release_details: bool,
    pub workspace: workspace::Workspace,
    pub dialog: plico::ModalState,
    pub quick_settings: plico::PopoverState,
    pub action_menu: plico::PopoverState,
    pub channel_select: plico::PopoverState,
    pub confirm_dialog: plico::ModalState,
    pub preview_notifications: bool,
}

impl App for Gallery {
    fn set_frame_time(&mut self, time: plico::FrameTime) {
        self.game2048.time = time.now;
        self.mines.time = time.now;
    }
    fn on_key(&mut self, key: &str) -> bool {
        match key {
            "Alt+ArrowLeft" if self.router.can_back() => {
                self.router.back();
                true
            }
            "Alt+ArrowRight" if self.router.can_forward() => {
                self.router.forward();
                true
            }
            _ if self.router.show(Page::Game2048) => self.game2048.key(key),
            _ if self.router.show(Page::Mines) => self.mines.key(key),
            _ => false,
        }
    }
    fn theme(&self) -> Theme {
        let mut theme = if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        if self.compact {
            theme.metrics = plico::Metrics::compact();
        }
        if self.large {
            theme.metrics.font_size = theme.metrics.font_size.saturating_add(2);
        }
        theme
    }
    fn show(&mut self, ui: &mut UiContext) {
        ui.localized(i18n::catalog(self.language), |ui| self.show_localized(ui));
    }
}

impl Gallery {
    #[component]
    #[rustfmt::skip]
    fn show_localized(&mut self, ui: &mut UiContext) {
        let l = ui.localizer();
        let m = ui.theme().metrics;
        row(ui).width_percent(100.).height_percent(100.).align(Align::Stretch) + |ui| {
            sidebar::show(ui, self);
            column(ui).width(0.).min_width(0.).grow(1.).padding(m.space(6)).gap(m.space(4)).children(|ui| {
                row(ui).justify(Justify::SpaceBetween).wrap(true).gap(8) + |ui| {
                    text(ui, l.text("workspace-gallery", "Workspace / Gallery")).font_size(12)
                        .color(Color::Muted).grow(1.);
                    switch(ui, l.text("dark-theme", "Dark theme"), &mut self.dark);
                    checkbox(ui, l.text("large-text", "Large text"), &mut self.large);
                };
                match self.router.current() {
                    Page::Workspace => self.workspace.show(ui),
                    Page::Layout => self.layout_page(ui),
                    Page::Controls => self.controls_page(ui),
                    Page::Motion => self.motion_page(ui),
                    Page::Lists => self.lists_page(ui),
                    Page::DataTable => data_table::page(ui),
                    Page::TreeView => tree_view::page(ui),
                    Page::Game2048 => self.game2048.show(ui),
                    Page::Mines => self.mines.show(ui, self.sidebar_collapsed),
                    Page::Markdown => self.markdown_chat.show(ui),
                    Page::Glass => self.glass.show(ui),
                    Page::Calendar => calendar::page(ui),
                    Page::Localization => i18n::page(ui, self.language),
                    Page::Images => self.images_page(ui),
                    Page::BlackHole => black_hole::page(ui, &mut self.black_hole),
                    Page::Patterns => {
                        patterns::page(ui, &mut self.patterns, &mut self.router)
                    }
                    Page::Effects => effects::page(ui, &mut self.effects),
                    Page::ShaderPlayground => {
                        shader_playground::page(ui, &mut self.shader_playground)
                    }
                    Page::ComponentKit => component_kit::page(ui, &mut self.component_kit),
                }
            })
        };
        // The gravitational lens overlays every page when enabled. Declared
        // last so its backdrop is everything painted before it.
        black_hole::overlay(ui, &mut self.black_hole);
    }
    #[cfg(all(feature = "host", not(target_arch = "wasm32")))]
    fn images_page(&mut self, cx: &mut UiContext) {
        self.images.show(cx);
    }
    #[cfg(not(all(feature = "host", not(target_arch = "wasm32"))))]
    fn images_page(&mut self, cx: &mut UiContext) {
        text(
            cx,
            "Network image playback is available in the desktop gallery.",
        );
    }

    #[component]
    #[rustfmt::skip]
    fn lists_page(&mut self, ui: &mut UiContext) {
        let l = ui.localizer();
        heading(ui, &l.text("virtual-lists", "Virtual lists"),
            &l.text("a-bounded-viewport-over-a-large-data-set", "A bounded viewport over a large data set."));
        panel(ui, l.text("virtual-rows", "10,000 virtual rows").as_ref()).description(&l.text(
            "only-the-visible-rows-are-built-scroll-to-explore", "Only the visible rows are built. Scroll to explore."))
        + |ui|
        {
            virtual_list(ui, 10_000, 36.).height(216.).rows(
                |i| i as u64,
                |ui, i| {
                    let _ = row(ui).height(36.).padding(8).align(Align::Center).children(|ui| {
                        plico::text!(ui, "Record {:05}", i + 1);
                    });
                },
            );
        };
    }
    fn layout_page(&mut self, ui: &mut UiContext) {
        layout_demo::show(ui, &mut self.expanded);
    }
    #[component]
    #[rustfmt::skip]
    fn controls_page(&mut self, ui: &mut UiContext) {
        let l = ui.localizer();
        heading(
            ui,
            &l.text("controls", "Controls"),
            &l.text("small-interactions-shared-state-leave-the-page-and-your-values-stay",
                "Small interactions, shared state. Leave the page and your values stay."),
        );
        column(ui).height(0.).grow(1.).scroll_y(true).gap(16) + |ui| {
            panel(ui, l.text("hover-and-activation", "Hover and activation").as_ref()) + |ui| {
                let activations = ui.state(|| 0_u32);
                let response = button(ui).text(l.text("hover-or-activate", "Hover or activate")).response();
                let mut hint = l.text("hover-hint",
                    "Move the pointer over the button, or activate it with the keyboard.");
                if response.hovered() {
                    hint = l.text("hover-active", "Pointer over the button. Hover and click are independent.");
                }
                if response.clicked() {
                    activations.update(|count| *count += 1);
                }
                text(ui, hint).font_size(12).color(Color::Muted);
                text(ui, i18n::count_message(self.language, "activated-count", activations.get().into())).weight(600);
            };
            panel(ui, l.text("buttons-feedback", "Buttons & feedback").as_ref()) + |ui| {
                row(ui).gap(8).wrap(true) + |ui| {
                    let increment = plico::widgets::tooltip(ui,
                        l.text("add-one-to-the-counter", "Add one to the counter")).children(|ui| {
                        icon_button(ui, &l.text("increment", "Increment"), &crate::icons::PLUS, true, true)
                    });
                    if increment.inner.clicked() {
                        self.count += 1;
                    }
                    let reset = plico::widgets::tooltip(ui,
                        l.text("reset-the-counter-to-zero", "Reset the counter to zero")).children(|ui| {
                        icon_button(ui, &l.text("reset", "Reset"), &crate::icons::ROTATE_CCW, true, false)
                    });
                    if reset.inner.clicked() {
                        self.count = 0;
                    }
                    icon_button(ui, &l.text("disabled", "Disabled"), &crate::icons::LOCK, false, false);
                };
                row(ui).gap(12).align(Align::Center).wrap(true).children(|ui| {
                    text(ui, i18n::count_message(self.language, "clicked-count", self.count.into())).font_size(24).weight(650).grow(1.);
                    plico::widgets::popover(ui, &mut self.quick_settings,
                        l.text("quick-settings", "Quick settings")) + |ui|
                    {
                        text(ui, l.text("workspace", "Workspace")).weight(650);
                        switch(ui, l.text("notifications", "Notifications"), &mut self.notifications);
                        checkbox(ui, l.text("save-automatically", "Save automatically"), &mut self.autosave);
                        separator(ui);
                        row(ui).gap(8).align(Align::Center).children(|ui| {
                            if button(ui).text(l.text("done", "Done")).primary().clicked() {
                                ui.close_popover();
                            }
                            plico::widgets::menu(ui, &mut self.action_menu, l.text("actions", "Actions")) + |ui| {
                                if plico::widgets::menu_item(ui, l.text("increment-count", "Increment count")).icon(&crate::icons::PLUS).clicked()
                                {
                                    self.count += 1;
                                }
                                if plico::widgets::menu_item(ui, l.text("reset-count", "Reset count")).icon(&crate::icons::ROTATE_CCW).disabled(self.count == 0).clicked()
                                {
                                    self.count = 0;
                                }
                            };
                        })
                    };
                })
            };
            let mut applied = false;
            plico::widgets::modal(ui, &mut self.dialog)
                .title(l.text("workspace-preferences", "Workspace preferences")) + |ui|
            {
                text(
                    ui,
                    l.text("choose-how-this-workspace-keeps-you-up-to-date",
                        "Choose how this workspace keeps you up to date."),
                ).color(Color::Muted);
                checkbox(ui, l.text("enable-notifications", "Enable notifications"),
                    &mut self.preview_notifications);
                separator(ui);
                row(ui).gap(8).justify(Justify::End).wrap(true) + |ui| {
                    if button(ui).text(l.text("cancel", "Cancel")).ghost().clicked() {
                        ui.close_modal();
                    }
                    if button(ui).text(l.text("save-preferences", "Save preferences")).primary().clicked() {
                        self.confirm_dialog.open();
                    }
                };
                plico::widgets::modal(ui, &mut self.confirm_dialog)
                    .title(l.text("apply-preferences", "Apply preferences?")) + |ui|
                {
                    text(ui, l
                        .text("your-notification-preference-will-apply-to-this-workspace", "Your notification preference will apply to this workspace."))
                        .color(Color::Muted);
                    row(ui).gap(8).justify(Justify::End).wrap(true).children(|ui| {
                        if button(ui).text(l.text("keep-editing", "Keep editing")).ghost().clicked() {
                            ui.close_modal();
                        }
                        if button(ui).text(l.text("apply-changes", "Apply changes")).primary().clicked() {
                            self.notifications = self.preview_notifications;
                            applied = true;
                            ui.close_modal();
                        }
                    })
                };
            };
            if applied {
                self.dialog.close();
            }
            settings_section(ui, l.text("preferences", "Preferences").as_ref()) + |ui| {
                let status = if self.notifications {
                    l.text("notifications-enabled", "Notifications are enabled")
                } else {
                    l.text("notifications-paused", "Notifications are paused")
                };
                settings_row(ui, l.text("notification-delivery", "Delivery").as_ref()).description(&status) + |ui| {
                    switch(ui, l.text("notifications", "Notifications"), &mut self.notifications).motion(Motion::Smooth);
                    if button(ui).text(l.text("edit-preferences", "Edit preferences")).clicked() {
                        self.preview_notifications = self.notifications;
                        self.dialog.open();
                    }
                };
                separator(ui);
                settings_row(ui, l.text("saving", "Saving").as_ref()) + |ui| {
                    checkbox(ui, l.text("save-automatically", "Save automatically"), &mut self.autosave);
                    checkbox(ui, l.text("disabled", "Disabled"), &mut self.autosave).disabled(true);
                };
            };
            release_channel(ui, &mut self.channel_select, &mut self.release_details);
            column(ui).padding(16).gap(12).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
                text(ui, l.text("avatars-ruby-text", "Avatars & ruby text")).font_size(17).weight(650);
                row(ui).gap(12).align(Align::Center).wrap(true) + |ui| {
                    avatar(ui).fallback("JD").size(40.);
                    avatar(ui).fallback("花").size(40.).background(Color::Selection).color(Color::Accent);
                    text(ui, "Images or explicit initials; loading stays in the app.").color(Color::Muted);
                };
                row(ui).gap(0).align(Align::Start) + |ui| {
                    ruby_text(ui, "夜").annotation("よる").font_size(28);
                    ruby_text(ui, "空").annotation("そら").font_size(28);
                    ruby_text(ui, "へ").font_size(28);
                };
            };
            column(ui).padding(16).gap(12).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
                text(ui, "Icon & label alignment").font_size(17).weight(650);
                row(ui).gap(8).wrap(true) + |ui| {
                    button(ui).icon(&crate::icons::SAVE).text("Save changes").height(40.);
                    button(ui).icon(&crate::icons::SAVE).text("保存更改").height(40.);
                    button(ui).icon(&crate::icons::SAVE).text("Agjp 0123").font_size(18).height(40.);
                    button(ui).icon(&crate::icons::SAVE).text("A long action label that stays on one line")
                        .width(180.).height(40.);
                };
                text(ui, "Icons keep their size; constrained action labels truncate without losing their accessible name.")
                    .font_size(12).color(Color::Muted);
            };
        };
    }
    #[component]
    #[rustfmt::skip]
    fn motion_page(&mut self, ui: &mut UiContext) {
        let l = ui.localizer();
        heading(ui, &l.text("motion-studies", "Motion studies"),
            &l.text("motion-description", "One state change. Dimensions, spacing, color and opacity move together."));
        // The two demo cards plus the shader do not fit the 640px minimum
        // window; the page scrolls rather than overflowing the viewport.
        column(ui).height(0.).grow(1.).scroll_y(true).gap(20) + |ui| {
            column(ui).padding(20).gap(20).radius(14.).background(Color::Panel) + |ui| {
                row(ui).justify(Justify::SpaceBetween).gap(8) + |ui| {
                    text(ui, "Layout + style").font_size(17).weight(650).grow(1.);
                    if button(ui).text("Animate layout").primary().font_size(12).clicked() {
                        self.expanded = !self.expanded;
                    }
                };
                column(ui).height(150.).justify(Justify::Center) + |ui| {
                    column(ui).width(if self.expanded { 320. } else { 200. })
                        .height(if self.expanded { 110. } else { 62. })
                        .padding(if self.expanded { 22 } else { 12 })
                        .radius(if self.expanded { 24. } else { 8. })
                        .opacity(if self.expanded { 1. } else { 0.7 }).background(if self.expanded {
                        Color::Literal(0x2259c9)
                    } else {
                        Color::Literal(0x207e99)
                    }).motion(Motion::Smooth).children(|ui| {
                        text(ui, l.text("a-little-more-room", "A little more room.")).font_size(17).weight(600).color(Color::Literal(0xffffff));
                    })
                };
                text(
                    ui,
                    if self.expanded {
                        l.text("expanded-tween", "Expanded / 240 ms")
                    } else {
                        l.text("compact-tween", "Compact / 240 ms")
                    },
                ).font_size(12).color(Color::Muted);
            };
            column(ui).padding(20).gap(20).radius(14.).background(Color::Panel) + |ui| {
                row(ui).justify(Justify::SpaceBetween).gap(8) + |ui| {
                    text(ui, l.text("spring-physics", "Spring physics")).font_size(17).weight(650).grow(1.);
                    if button(ui).text(l.text("animate-spring", "Animate spring")).primary().font_size(12)
                        .clicked()
                    {
                        self.spring.out = !self.spring.out;
                    }
                };
                column(ui).height(150.).justify(Justify::Center) + |ui| {
                    // The spring overshoots before settling: width goes past the
                    // target and comes back, unlike the 240 ms tween above.
                    let target = if self.spring.out { 320. } else { 200. };
                    let width = ui.spring(
                        "motion.demo.width",
                        target,
                        SpringOptions {
                            mass: 1.,
                            stiffness: 170.,
                            damping: 14.,
                            precision: 0.001,
                            ..Default::default()
                        },
                    );
                    column(ui).width(width).height(78.).radius(12.).background(Color::Accent)
                        .children(|ui| {
                        text(ui, l.text("overshoot-then-settle", "Overshoot, then settle.")).font_size(15).weight(600).color(Color::OnAccent);
                    });
                };
                text(
                    ui,
                    if self.spring.out {
                        l.text("spring-out", "Sprung / out")
                    } else {
                        l.text("spring-in", "Sprung / in")
                    },
                ).font_size(12).color(Color::Muted);
            };
        };
        shader_demo(ui);
    }
}

#[component]
#[rustfmt::skip]
fn release_channel(ui: &mut UiContext, menu: &mut plico::PopoverState, details: &mut bool) {
    let l = ui.localizer();
    let channel = ui.persisted_state("selected", || 0_usize);
    let mut selected = channel.get();
    column(ui).padding(16).gap(8).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
        row(ui).gap(8).align(Align::Center).wrap(true) + |ui| {
            text(ui, l.text("release-channel", "Release channel")).font_size(17).weight(650).grow(1.);
            badge(
                ui,
                &if selected == 0 {
                    l.text("recommended", "Recommended")
                } else {
                    l.text("preview", "Preview")
                },
            );
        };
        let channels = [
            plico::SelectOption::new("stable", 0, "Stable"),
            plico::SelectOption::new("beta", 1, "Beta"),
            plico::SelectOption::new("nightly", 2, "Nightly"),
        ];
        row(ui).gap(8).align(Align::Center).wrap(true) + |ui| {
            segmented(ui, &mut selected, &["Stable", "Beta", "Nightly"]);
            plico::widgets::select(ui, menu, "Channel", &mut selected, &channels).width(132.);
        };
        separator(ui);
        collapsible(
            ui,
            &l.text("channel-details", "Channel details"),
            details,
            |ui| {
                progress(
                    ui,
                    &l.text("validation-coverage", "Validation coverage"),
                    match selected {
                        1 => 0.75,
                        2 => 0.4,
                        _ => 1.,
                    },
                );
                text(
                    ui,
                    match selected {
                        1 => "Early access with occasional breaking changes.",
                        2 => "Daily builds for testing the latest changes.",
                        _ => "Tested releases for everyday projects.",
                    },
                ).color(Color::Muted).font_size(12);
            },
        );
    };
    channel.set(selected);
}

#[component]
#[rustfmt::skip]
fn shader_demo(ui: &mut UiContext) {
    let l = ui.localizer();
    let paused = ui.persisted_state("paused", || false);
    let exposure = ui.persisted_state("exposure", || 0_f64);
    let mut current_exposure = exposure.get();
    column(ui).padding(16).gap(10).radius(12.).background(Color::Panel) + |ui| {
        row(ui).gap(8).align(Align::Center).wrap(true) + |ui| {
            text(ui, l.text("custom-shader", "Custom shader")).font_size(17).weight(650).grow(1.);
            text(ui, l.text("exposure", "Exposure")).font_size(12).color(Color::Muted);
            plico::widgets::slider(ui, "Exposure", &mut current_exposure, -1.0..=1.0).step(0.05).width(120.);
            if button(ui).text(if paused.get() {
                l.text("resume-shader", "Resume shader")
            } else {
                l.text("pause-shader", "Pause shader")
            }).ghost().font_size(12).clicked()
            {
                paused.update(|value| *value = !*value);
            }
        };
        exposure.set(current_exposure);
        #[cfg(not(target_arch = "wasm32"))]
        plico::widgets::shader(ui, &AURORA).width_percent(100.).height(110.)
            .values(&[(0.8 + current_exposure * 0.7) as f32]).animated(true).paused(paused.get());
        #[cfg(target_arch = "wasm32")]
        text(ui, "Custom WGSL shaders require WebGPU; this browser host uses WebGL2.").color(Color::Muted);
        text(
            ui,
            l.text("native-gpu-surface-injected-clock-retained-resources",
                "Native GPU surface · injected clock · retained resources"),
        ).font_size(11).color(Color::Muted);
    };
}

#[component]
#[rustfmt::skip]
fn heading(ui: &mut UiContext, title: &str, subtitle: &str) {
    let font_size = ui.theme().metrics.font_size;
    column(ui).gap(6) + |ui| {
        text(ui, title).font_size(28).weight(700);
        text(ui, subtitle).font_size(font_size).color(Color::Muted);
    };
}
#[component]
fn bar(ui: &mut UiContext, width: f32, height: f32, grow: f32, color: Color) {
    let _ = column(ui)
        .width(width)
        .grow(grow)
        .height(height)
        .radius(5.)
        .background(color)
        .children(|_ui| {});
}

#[cfg(feature = "host")]
plico_desktop::impl_host_view_for_app!(Gallery);
