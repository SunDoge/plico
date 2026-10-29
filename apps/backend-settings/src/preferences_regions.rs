//! Same Preferences page through synchronous and explicit-region APIs.
//! Opt-in comparison; the existing routed ImmediateSettings application stays unchanged.
pub mod scale;
use crate::{ImmediateSettings, icons};
use plico::{DeclareView, Theme, prelude::*};
use plico_runtime::{
    View,
    regions::{
        Access, BindState, OnChange, OnClick, Reactive, RegionApp, RegionProgram, Regions, WatchUi,
    },
};

/// Baseline: the existing Preferences function and ordinary application state.
#[derive(Default)]
pub struct ImmediatePreferences(pub ImmediateSettings);
impl View for ImmediatePreferences {
    fn theme(&self) -> Theme {
        DeclareView::theme(&self.0)
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        frame(cx, |cx| self.0.preferences(cx));
    }
}

#[component]
#[rustfmt::skip]
fn frame(cx: &mut UiContext, contents: impl FnOnce(&mut UiContext<'_>)) {
    column(cx).width_percent(100.).height_percent(100.).padding(20).gap(18).background(Color::Surface) + |cx| {
        text(cx, "Preferences").font_size(28).weight(700);
        text(cx, "Application state survives updates. The editor remains a native control.").font_size(13).color(Color::Muted);
        column(cx).height(0.).grow(1.).scroll_y(true).gap(18).children(contents);
    };
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Saved {
    message: String,
    count: usize,
}

/// Global theme/motion state belongs to the root. Profile state belongs to its
/// component. The existing adapter supplies View and best-effort reload support.
pub type RegionPreferences = RegionApp<PreferencesProgram>;
pub struct PreferencesProgram;
impl RegionProgram for PreferencesProgram {
    fn build() -> Regions {
        Regions::new(|app| {
            let theme = app.persisted_state(1, Theme::light);
            let reduced = app.persisted_state(2, || false);
            app.theme_state(theme);
            app.root((theme, reduced), |cx, (theme, reduced)| {
                frame(cx, |cx| {
                    cx.component((theme.handle(), reduced.handle()), preferences);
                });
            });
        })
    }
}

fn save((name, saved): (Access<'_, TextBuffer>, Access<'_, Saved>)) {
    let name = name.read().text().unwrap_or_default();
    saved.update(|saved| {
        saved.count += 1;
        saved.message = format!("Saved {}: {}", saved.count, name);
    });
}

#[component]
#[rustfmt::skip]
fn preferences(cx: &mut UiContext, (theme, reduced): (Access<'_, Theme>, Access<'_, bool>)) {
    let name = cx.local_state(|| {
        let name = TextBuffer::new();
        name.set("Plico workspace".to_owned().into());
        name
    });
    let notifications = cx.local_persisted_state(1, || true);
    let advanced = cx.local_persisted_state(2, || false);
    let saved = cx.local_persisted_state(3, Saved::default);
    // The component boundary is a real column: it must own the original gap.
    column(cx).gap(18) + |cx| {
        column(cx).padding(18).gap(14).background(Color::Panel).border(1., Color::Border).radius(8.) + |cx| {
            text(cx, "Profile / 工作区").font_size(18).weight(600);
            text(cx, "Workspace name");
            cx.watch((name,), |cx, (name,)| {
                text_input(cx, "Workspace name", &name.read()).placeholder("Enter a name").width_percent(100.)
                    .height(36.).bind(name.handle());
            });
            text_empty(cx).color(Color::Muted).rx((name,), |node, (name,)| {
                let current = name.read().text().unwrap_or_default();
                node.text(format!("Current name: {current}"));
            });
            cx.watch((notifications,), |cx, (notifications,)| {
                let mut value = *notifications.read();
                checkbox(cx, "Enable notifications", &mut value).bind(notifications.handle());
            });
            cx.watch((advanced,), |cx, (advanced,)| {
                column(cx).gap(14) + |cx| {
                    let mut value = *advanced.read();
                    checkbox(cx, "Show advanced options", &mut value).bind(advanced.handle());
                    if value {
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
            });
        };
        cx.watch((theme.handle(),), |cx, (theme,)| {
            row(cx).gap(12).wrap(true) + |cx| {
                let mut value = *theme.read() == Theme::dark();
                checkbox(cx, "Dark theme", &mut value).on_change(
                    (theme.handle(),),
                    |(theme,), dark| {
                        theme.set(if dark { Theme::dark() } else { Theme::light() });
                    },
                );
                button(cx).text("Disabled action").disabled(true);
            };
        });
        // These handles are only used by events. A component boundary keeps
        // the toolbar out of the name/saved subscriptions without hidden captures.
        cx.component((name, saved), |cx, (name, saved)| {
            row(cx).gap(10).wrap(true) + |cx| {
                button(cx).icon(&icons::CHECK).text("Save changes").primary()
                    .on_click((name.handle(), saved.handle()), save);
                button(cx).text("Reset name").on_click((name.handle(),), |(name,)| {
                    name.read().set("Plico workspace".to_owned().into());
                    name.notify();
                });
            };
        });
        text(cx, "No changes saved yet").color(Color::Muted).rx((saved,), |node, (saved,)| {
            let saved = saved.read();
            if !saved.message.is_empty() {
                node.text(saved.message.clone());
            }
        });
        cx.watch((reduced.handle(),), |cx, (reduced,)| {
            let mut value = *reduced.read();
            checkbox(cx, "Reduce motion", &mut value).bind(reduced.handle());
        });
    };
}
