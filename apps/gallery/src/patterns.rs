//! Desktop paradigms demo: tab strips, toolbars, toasts and drag surfaces
//! (the shared widgets in plico::widgets, MyGO semantics).
use crate::heading;
use plico::prelude::*;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Patterns {
    tab: usize,
    toasts: ToastHost,
    toast_count: u32,
    dot: (f64, f64),
    crumb: usize,
    disk: f64,
    stars: u8,
}

impl Patterns {
    pub fn tick(&mut self) -> bool {
        !self.toasts.is_empty()
    }
}

#[component]
#[rustfmt::skip]
pub fn page(ui: &mut UiContext, state: &mut Patterns) {
    heading(ui, "Desktop patterns", "Tabs, toolbars, toasts and drag surfaces — the shared desktop widgets.");
    toolbar(ui).wrap(true) + |ui| {
        if button(ui).text("Toast").primary().clicked() {
            state.toast_count += 1;
            state.toasts.toast(Toast::message(format!("Saved {} times", state.toast_count)));
        }
        if button(ui).text("Action toast").ghost().clicked() {
            state.toasts.toast(Toast::message("Background task completed").with_action("Dismiss", || {}));
        }
        if button(ui).text("Failure").ghost().clicked() {
            state.toasts.toast(Toast {
                kind: ToastKind::Error,
                description: Some("The disk filled up mid-write.".into()),
                ..Toast::message("Save failed")
            });
        }
        if button(ui).text("Sticky").ghost().clicked() {
            state.toasts.toast(Toast::message("Still here until closed").sticky());
        }
        toolbar_spacer(ui);
        text!(ui, "{} in flight", state.toasts.len()).color(Color::Muted);
    };
    separator(ui);
    if tabs(ui, &mut state.tab, &["Overview", "Details", "Activity"]) {
        ui.request_rebuild();
    }
    column(ui).height(0.).grow(1.).gap(16).scroll_y(true) + |ui| {
        panel(ui, None) + |ui| {
            match state.tab {
                0 => {
                    text(ui, "Overview").font_size(16).weight(650);
                    text(ui, "Click a tab above; the strip reports the change and the tab underlines with the accent color.")
                        .color(Color::Muted);
                }
                1 => {
                    text(ui, "Details").font_size(16).weight(650);
                    text(ui, "Each tab keeps its semantic role for screen readers; each tab reports its selected state.")
                        .color(Color::Muted);
                }
                _ => {
                    text(ui, "Activity").font_size(16).weight(650);
                    text(ui, "Use the toolbar to try transient messages, errors and persistent notifications.")
                        .color(Color::Muted);
                }
            };
        };
        panel(ui, "Navigation and indicators")
            .description("Breadcrumb links, a leveled meter, a spinning indicator and a star rating, ported from MyGO.")
        + |ui|
        {
            if breadcrumbs(ui, &["Macintosh HD", "Users", "ada", "Documents"], &mut state.crumb) {
                ui.request_rebuild();
            }
            text!(ui, "Chosen: {}", state.crumb).color(Color::Muted);
            separator(ui);
            meter(ui, "Disk", state.disk, 0.0..=100.0, MeterLevels {
                warning: Some(80.),
                critical: Some(95.),
            });
            let _ = slider(ui, "Disk used", &mut state.disk, 0.0..=100.0).step(1.);
            row(ui).gap(8).align(Align::Center) + |ui| {
                spinner(ui);
                text(ui, "Loading messages…").color(Color::Muted);
            };
            if rating(ui, &mut state.stars, 5) {
                ui.request_rebuild();
            }
            text!(ui, "{} of 5 stars", state.stars).color(Color::Muted);
        };
        crate::shader_demo(ui);
        panel(ui, "Drag surface").description("Hold the pointer down and move it: the dot follows the drag.") + |ui| {
            row(ui).height(120.) + |ui| {
                column(ui).grow(1.).height_percent(100.).radius(8.).background(Color::Surface)
                    .border(1., Color::Border) + |ui|
                {
                    column(ui).absolute(0., 0.).width_percent(100.).height_percent(100.) + |ui| {
                        drag_area(ui, &mut |dx, dy| {
                            state.dot.0 = (state.dot.0 + dx).clamp(-80., 80.);
                            state.dot.1 = (state.dot.1 + dy).clamp(-40., 40.);
                        });
                    };
                    let (x, y) = state.dot;
                    row(ui).width_percent(100.).height_percent(100.).align(Align::Center)
                        .justify(Justify::Center) + |ui|
                    {
                        column(ui).width(14.).height(14.).radius(7.).background(Color::Accent)
                            .translate(x as f32, y as f32).children(|_| {});
                    };
                };
            };
        };
        column(ui).selectable(true).padding(12).gap(8).radius(10.).background(Color::Panel)
            .border(1., Color::Border) + |ui|
        {
            text(ui, "Text selection").font_size(16).weight(650);
            text(ui, "Drag to select text across these paragraphs. Double-click a word, triple-click a paragraph, then Ctrl/Cmd+C to copy.")
                .color(Color::Muted);
            text(ui, "中文与日本語 · Selectable text · e\u{301} · 👩‍💻").font_size(18);
            text(ui, "Ctrl/Cmd+A selects this group; Escape clears the selection.").color(Color::Muted);
        };
    };
    // Overlay after the page so retiring cards remain above the content.
    toast_viewport(ui, &mut state.toasts);
}
