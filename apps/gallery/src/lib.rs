mod calendar;
mod color_icons;
pub mod icons;
#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
pub mod images;
use plico_core::{Align, Color, Justify, Motion, Theme, UiContext, component};
use plico_dev::App;
mod components;
pub mod game2048;
pub mod glass;
mod layout_demo;
pub mod markdown_chat;
pub mod workspace;
use components::icon_button;
use plico_core::widgets::{
    badge, button, checkbox, collapsible, column, nav_menu, progress, row, segmented, separator,
    svg, switch, text, virtual_list,
};

#[cfg(not(target_arch = "wasm32"))]
static AURORA: plico_core::ShaderAsset = plico_core::shader_asset!("../assets/shaders/aurora.wgsl");

static LOGO: plico_core::SvgAsset = plico_core::svg_asset!("../assets/plico.svg");

#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    plico_core::Route,
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
    #[route(label = "2048", icon = color_icons::PUZZLE_PIECE)]
    Game2048,
    #[route(label = "Markdown chat", icon = color_icons::CHAT)]
    Markdown,
    #[route(label = "Glass", icon = color_icons::IMAGE)]
    Glass,
    #[route(label = "Calendar", icon = color_icons::CALENDAR)]
    Calendar,
    #[route(label = "Images", icon = color_icons::IMAGE)]
    Images,
}

#[derive(Default, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Gallery {
    #[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
    #[serde(skip)]
    pub images: images::Images,
    pub game2048: game2048::Game,
    pub markdown_chat: markdown_chat::Chat,
    pub glass: glass::Glass,
    pub count: u32,
    pub notifications: bool,
    pub autosave: bool,
    pub dark: bool,
    pub large: bool,
    pub expanded: bool,
    pub router: plico_core::Router<Page>,
    pub alignment: u8,
    pub wrapped: bool,
    pub compact: bool,
    pub release_details: bool,
    pub workspace: workspace::Workspace,
    pub dialog: plico_core::ModalState,
    pub quick_settings: plico_core::PopoverState,
    pub action_menu: plico_core::PopoverState,
    pub channel_select: plico_core::PopoverState,
    pub confirm_dialog: plico_core::ModalState,
    pub preview_notifications: bool,
}

impl App for Gallery {
    fn set_frame_time(&mut self, time: plico_core::FrameTime) {
        self.game2048.time = time.now;
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
            _ => self.router.show(Page::Game2048) && self.game2048.key(key),
        }
    }
    fn theme(&self) -> Theme {
        let mut theme = if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        if self.compact {
            theme.metrics = plico_core::Metrics::compact();
        }
        if self.large {
            theme.metrics.font_size = theme.metrics.font_size.saturating_add(2);
        }
        theme
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, ui: &mut UiContext) {
        let m = ui.theme().metrics;
        row(ui).width_percent(100.).height_percent(100.).align(Align::Stretch) + |ui| {
            column(ui).width(192.).shrink(0.).padding(12).gap(12).background(Color::Panel) + |ui| {
                row(ui).gap(8) + |ui| {
                    svg(ui, &LOGO).size(26., 26.);
                    text(ui, "plico").font_size(23).weight(750);
                };
                text(ui, "INTERFACE LAB").font_size(10).weight(600).color(Color::Muted);
                column(ui).gap(6).padding(0) + |ui| {
                    nav_menu(ui, &mut self.router);
                };
                column(ui).grow(1.).children(|_ui| {});
                checkbox(ui, "Compact", &mut self.compact);
                text(ui, "Rust, in motion.").font_size(12).weight(600);
                text(ui, "Development gallery").font_size(10).color(Color::Muted);
            };
            column(ui).width(0.).grow(1.).padding(m.space(6)).gap(m.space(4)).children(|ui| {
                row(ui).justify(Justify::SpaceBetween).wrap(true).gap(8) + |ui| {
                    text(ui, "Workspace / Gallery").font_size(12).color(Color::Muted).grow(1.);
                    switch(ui, "Dark theme", &mut self.dark);
                    checkbox(ui, "Large text", &mut self.large);
                };
                match self.router.current() {
                    Page::Workspace => self.workspace.show(ui),
                    Page::Layout => self.layout_page(ui),
                    Page::Controls => self.controls_page(ui),
                    Page::Motion => self.motion_page(ui),
                    Page::Lists => self.lists_page(ui),
                    Page::Game2048 => self.game2048.show(ui),
                    Page::Markdown => self.markdown_chat.show(ui),
                    Page::Glass => self.glass.show(ui),
                    Page::Calendar => calendar::page(ui),
                    Page::Images => self.images_page(ui),
                }
            })
        };
    }
}

impl Gallery {
    #[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
    fn images_page(&mut self, cx: &mut UiContext) {
        self.images.show(cx);
    }
    #[cfg(not(all(feature = "desktop", not(target_arch = "wasm32"))))]
    fn images_page(&mut self, cx: &mut UiContext) {
        text(
            cx,
            "Network image playback is available in the desktop gallery.",
        );
    }

    #[component]
    #[rustfmt::skip]
    fn lists_page(&mut self, ui: &mut UiContext) {
        heading(ui, "Virtual lists", "A bounded viewport over a large data set.");
        column(ui).padding(16).gap(8).background(Color::Panel).radius(14.) + |ui| {
            text(ui, "10,000 virtual rows").font_size(15).weight(650);
            text(ui, "Only the visible rows are built. Scroll to explore.").color(Color::Muted);
            virtual_list(ui, 10_000, 36.).height(216.).smooth_scroll(true).rows(
                |i| i as u64,
                |ui, i| {
                    let _ = row(ui).height(36.).padding(8).align(Align::Center).children(|ui| {
                        plico_core::text!(ui, "Record {:05}", i + 1);
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
        heading(ui, "Controls", "Small interactions, shared state. Leave the page and your values stay.");
        column(ui).height(0.).grow(1.).scroll_y(true).gap(16) + |ui| {
            column(ui).padding(16).gap(8).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
                text(ui, "Hover and activation").font_size(17).weight(650);
                let activations = ui.state(|| 0_u32);
                let response = button(ui).text("Hover or activate").response();
                let mut hint =
                    "Move the pointer over the button, or activate it with the keyboard.";
                if response.hovered() {
                    hint = "Pointer over the button. Hover and click are independent.";
                }
                if response.clicked() {
                    activations.update(|count| *count += 1);
                }
                text(ui, hint).font_size(12).color(Color::Muted);
                plico_core::text!(ui, "Activated {} times", activations.get()).weight(600);
            };
            column(ui).padding(16).gap(8).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
                text(ui, "Buttons & feedback").font_size(17).weight(650);
                row(ui).gap(8).wrap(true) + |ui| {
                    let increment =
                        plico_core::widgets::tooltip(ui, "Add one to the counter").children(|ui| {
                            icon_button(ui, "Increment", &crate::icons::PLUS, true, true)
                        });
                    if increment.inner.clicked() {
                        self.count += 1;
                    }
                    let reset =
                        plico_core::widgets::tooltip(ui, "Reset the counter to zero").children(|ui| {
                            icon_button(ui, "Reset", &crate::icons::ROTATE_CCW, true, false)
                        });
                    if reset.inner.clicked() {
                        self.count = 0;
                    }
                    icon_button(ui, "Disabled", &crate::icons::LOCK, false, false);
                };
                row(ui).gap(12).align(Align::Center).wrap(true).children(|ui| {
                    plico_core::text!(ui, "Clicked {} times", self.count).font_size(24).weight(650)
                        .grow(1.);
                    plico_core::widgets::popover(ui, &mut self.quick_settings, "Quick settings") + |ui| {
                        text(ui, "Workspace").weight(650);
                        switch(ui, "Notifications", &mut self.notifications);
                        checkbox(ui, "Save automatically", &mut self.autosave);
                        separator(ui);
                        row(ui).gap(8).align(Align::Center).children(|ui| {
                            if button(ui).text("Done").primary().clicked() {
                                ui.close_popover();
                            }
                            plico_core::widgets::menu(ui, &mut self.action_menu, "Actions") + |ui| {
                                if plico_core::widgets::menu_item(ui, "Increment count")
                                    .icon(&crate::icons::PLUS).clicked()
                                {
                                    self.count += 1;
                                }
                                if plico_core::widgets::menu_item(ui, "Reset count")
                                    .icon(&crate::icons::ROTATE_CCW).disabled(self.count == 0).clicked()
                                {
                                    self.count = 0;
                                }
                            };
                        })
                    };
                })
            };
            let mut applied = false;
            plico_core::widgets::modal(ui, &mut self.dialog).title("Workspace preferences") + |ui| {
                text(ui, "Choose how this workspace keeps you up to date.").color(Color::Muted);
                checkbox(ui, "Enable notifications", &mut self.preview_notifications);
                separator(ui);
                row(ui).gap(8).justify(Justify::End).wrap(true) + |ui| {
                    if button(ui).text("Cancel").ghost().clicked() {
                        ui.close_modal();
                    }
                    if button(ui).text("Save preferences").primary().clicked() {
                        self.confirm_dialog.open();
                    }
                };
                plico_core::widgets::modal(ui, &mut self.confirm_dialog).title("Apply preferences?") + |ui| {
                    text(ui, "Your notification preference will apply to this workspace.").color(Color::Muted);
                    row(ui).gap(8).justify(Justify::End).wrap(true).children(|ui| {
                        if button(ui).text("Keep editing").ghost().clicked() {
                            ui.close_modal();
                        }
                        if button(ui).text("Apply changes").primary().clicked() {
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
            column(ui).padding(16).gap(8).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
                text(ui, "Preferences").font_size(17).weight(650);
                row(ui).gap(8).wrap(true) + |ui| {
                    switch(ui, "Notifications", &mut self.notifications).motion(Motion::Smooth);
                    if button(ui).text("Edit preferences").font_size(12).clicked() {
                        self.preview_notifications = self.notifications;
                        self.dialog.open();
                    }
                };
                row(ui).gap(8).wrap(true) + |ui| {
                    checkbox(ui, "Save automatically", &mut self.autosave);
                    checkbox(ui, "Disabled", &mut self.autosave).disabled(true);
                };
                text(
                    ui,
                    if self.notifications { "Notifications are enabled" } else { "Notifications are paused" },
                ).font_size(12).color(Color::Muted);
            };
            release_channel(ui, &mut self.channel_select, &mut self.release_details);
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
        heading(ui, "Motion studies", "One state change. Dimensions, spacing, color and opacity move together.");
        column(ui).padding(20).gap(20).radius(14.).background(Color::Panel) + |ui| {
            row(ui).justify(Justify::SpaceBetween).gap(8) + |ui| {
                text(ui, "Layout + style").font_size(17).weight(650).grow(1.);
                if button(ui).text("Animate layout").primary().font_size(12).clicked()
                {
                    self.expanded = !self.expanded;
                }
            };
            column(ui).height(150.).justify(Justify::Center) + |ui| {
                column(ui).width(if self.expanded { 320. } else { 200. })
                    .height(if self.expanded { 110. } else { 62. }).padding(if self.expanded { 22 } else { 12 })
                    .radius(if self.expanded { 24. } else { 8. }).opacity(if self.expanded { 1. } else { 0.7 })
                    .background(if self.expanded { Color::Literal(0x2259c9) } else { Color::Literal(0x207e99) })
                    .motion(Motion::Smooth).children(|ui| {
                    text(ui, "A little more room.").font_size(17).weight(600)
                        .color(Color::Literal(0xffffff));
                })
            };
            text(ui, if self.expanded { "Expanded / 240 ms" } else { "Compact / 240 ms" }).font_size(12).color(Color::Muted);
        };
        shader_demo(ui);
    }
}

#[component]
#[rustfmt::skip]
fn release_channel(ui: &mut UiContext, menu: &mut plico_core::PopoverState, details: &mut bool) {
    let channel = ui.persisted_state("selected", || 0_usize);
    let mut selected = channel.get();
    column(ui).padding(16).gap(8).radius(10.).background(Color::Panel).border(1., Color::Border) + |ui| {
        row(ui).gap(8).align(Align::Center).wrap(true) + |ui| {
            text(ui, "Release channel").font_size(17).weight(650).grow(1.);
            badge(ui, if selected == 0 { "Recommended" } else { "Preview" });
        };
        let channels = [
            plico_core::SelectOption::new("stable", 0, "Stable"),
            plico_core::SelectOption::new("beta", 1, "Beta"),
            plico_core::SelectOption::new("nightly", 2, "Nightly"),
        ];
        row(ui).gap(8).align(Align::Center).wrap(true) + |ui| {
            segmented(ui, &mut selected, &["Stable", "Beta", "Nightly"]);
            plico_core::widgets::select(ui, menu, "Channel", &mut selected, &channels).width(132.);
        };
        separator(ui);
        collapsible(ui, "Channel details", details, |ui| {
            progress(
                ui,
                "Validation coverage",
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
        });
    };
    channel.set(selected);
}

#[component]
#[rustfmt::skip]
fn shader_demo(ui: &mut UiContext) {
    let paused = ui.persisted_state("paused", || false);
    let exposure = ui.persisted_state("exposure", || 0_f64);
    let mut current_exposure = exposure.get();
    column(ui).padding(16).gap(10).radius(12.).background(Color::Panel) + |ui| {
        row(ui).gap(8).align(Align::Center).wrap(true) + |ui| {
            text(ui, "Custom WGSL").font_size(17).weight(650).grow(1.);
            text(ui, "Exposure").font_size(12).color(Color::Muted);
            plico_core::widgets::slider(ui, "Exposure", &mut current_exposure, -1.0..=1.0).step(0.05)
                .width(120.);
            if button(ui).text(if paused.get() { "Resume shader" } else { "Pause shader" }).ghost()
                .font_size(12).clicked()
            {
                paused.update(|value| *value = !*value);
            }
        };
        exposure.set(current_exposure);
        #[cfg(not(target_arch = "wasm32"))]
        plico_core::widgets::shader(ui, &AURORA).width_percent(100.).height(110.)
            .values(&[(0.8 + current_exposure * 0.7) as f32]).animated(true).paused(paused.get());
        #[cfg(target_arch = "wasm32")]
        text(ui, "Custom WGSL shaders require WebGPU; this browser host uses WebGL2.").color(Color::Muted);
        text(ui, "Native GPU surface · injected clock · retained resources").font_size(11).color(Color::Muted);
    };
}

#[component]
#[rustfmt::skip]
fn heading(ui: &mut UiContext, title: &str, subtitle: &str) {
    column(ui).gap(6) + |ui| {
        text(ui, title).font_size(28).weight(700);
        text(ui, subtitle).font_size(12).color(Color::Muted);
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

#[cfg(all(feature = "abi", not(target_arch = "wasm32")))]
plico_dev::export_app!(Gallery);
#[cfg(feature = "desktop")]
plico_vello::impl_view_for_app!(Gallery);
