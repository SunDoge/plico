//! Desktop paradigms demo: tab strips, toolbars, toasts, drag surfaces,
//! breadcrumbs, indicators, steppers, split panes, a source list, a search
//! field, radios, links, router history, date and time inputs, and a find bar
//! (the shared widgets in plico::widgets, MyGO semantics).
use crate::heading;
use plico::prelude::*;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Patterns {
    tab: usize,
    toasts: ToastHost,
    toast_count: u32,
    dot: (f64, f64),
    crumb: usize,
    disk: f64,
    stars: u8,
    copies: f64,
    pane: f32,
    mailbox: String,
    mail_open: bool,
    query: String,
    file: String,
    file_open: bool,
    tint: Color,
    email: String,
    size: String,
    birthday: CalendarDate,
    birthday_open: PopoverState,
    finding: bool,
    find_query: String,
    find_current: usize,
    tint_open: bool,
    meeting: TimeOfDay,
    alert: ModalState,
    deleted: usize,
    dismissed: usize,
    general_open: bool,
    privacy_open: bool,
}

impl Default for Patterns {
    fn default() -> Self {
        Self {
            tab: 0,
            toasts: ToastHost::default(),
            toast_count: 0,
            dot: (0., 0.),
            crumb: 0,
            disk: 42.,
            stars: 3,
            copies: 1.,
            pane: 200.,
            mailbox: "inbox".into(),
            mail_open: true,
            query: String::new(),
            file: "main.go".into(),
            file_open: true,
            tint: Color::Literal(0x3b82f6),
            email: String::new(),
            size: "Medium".into(),
            birthday: CalendarDate::new(2026, 11, 6).expect("demo date"),
            birthday_open: PopoverState::default(),
            finding: false,
            find_query: String::new(),
            find_current: 0,
            tint_open: false,
            meeting: TimeOfDay::new(15, 4).expect("demo time"),
            alert: ModalState::default(),
            deleted: 0,
            dismissed: 0,
            general_open: true,
            privacy_open: false,
        }
    }
}

impl Patterns {
    pub fn tick(&mut self) -> bool {
        !self.toasts.is_empty()
    }
}

#[component]
#[rustfmt::skip]
pub fn page(ui: &mut UiContext, state: &mut Patterns, router: &mut plico::Router<crate::Page>) {
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
            separator(ui);
            row(ui).gap(8).align(Align::Center) + |ui| {
                text!(ui, "{} copies", state.copies);
                stepper(ui, &mut state.copies, 1.0..=99.0, 1.0);
            };
        };
        panel(ui, "Resizable split")
            .description("Drag the divider: the sidebar width lives in the application state.") + |ui|
        {
            let _ = split(
                ui,
                &mut state.pane,
                |ui| {
                    column(ui).padding(12).height(150.).background(Color::Panel) + |ui| {
                        text(ui, "Sidebar").weight(650);
                    };
                },
                |ui| {
                    column(ui).padding(12).height(150.) + |ui| {
                        text(ui, "Content").weight(650);
                        text(ui, "The divider moves the first pane and keeps the rest.").color(Color::Muted);
                    };
                },
            );
        };
        panel(ui, "Source list and search").description(
            "A sidebar of sections with icon items and a search field, ported from MyGO.") + |ui|
        {
            row(ui).gap(16).align(Align::Start) + |ui| {
                let _ = sidebar(ui).width(220.).height(200.) + |ui| {
                    sidebar_section(
                        ui,
                        "Mailboxes",
                        Some(&mut state.mail_open),
                        |ui| {
                            if sidebar_item(ui, "Inbox", state.mailbox == "inbox",
                                Some(&crate::icons::FOLDER)).clicked()
                            {
                                state.mailbox = "inbox".into();
                            }
                            if sidebar_item(ui, "Sent", state.mailbox == "sent",
                                Some(&crate::icons::MESSAGE_SQUARE)).clicked()
                            {
                                state.mailbox = "sent".into();
                            }
                            if sidebar_item(ui, "Archive", state.mailbox == "archive",
                                Some(&crate::icons::ARCHIVE)).clicked()
                            {
                                state.mailbox = "archive".into();
                            }
                        },
                    );
                    sidebar_section(ui, "Tags", None, |ui| {
                        if sidebar_item(ui, "Work", state.mailbox == "work", None).clicked() {
                            state.mailbox = "work".into();
                        }
                        if sidebar_item(ui, "Receipts", state.mailbox == "receipts", None).clicked() {
                            state.mailbox = "receipts".into();
                        }
                    });
                };
                column(ui).gap(8.).grow(1.) + |ui| {
                    search_field(ui, "Search mail", "Search mail", &mut state.query);
                    text!(ui, "Query: {}", state.query).color(Color::Muted);
                    text!(ui, "Mailbox: {}", state.mailbox).color(Color::Muted);
                    text(ui, "Click a sidebar item, collapse Mailboxes, or clear the search text.")
                        .color(Color::Muted);
                };
            };
        };
        panel(ui, "Tree").description("A disclosure tree of files, ported from MyGO's Tree/TreeItem.") + |ui| {
            tree(ui, |ui| {
                let branch = tree_item(
                    ui,
                    "src",
                    state.file == "src",
                    Some(&mut state.file_open),
                    0,
                    |ui| {
                        if tree_item(ui, "main.go", state.file == "main.go", None, 1, |_| {}).clicked() {
                            state.file = "main.go".into();
                        }
                        if tree_item(ui, "widgets.rs", state.file == "widgets.rs", None, 1, |_| {}).clicked() {
                            state.file = "widgets.rs".into();
                        }
                    },
                );
                if branch.clicked() {
                    state.file = "src".into();
                }
                if tree_item(ui, "Cargo.toml", state.file == "Cargo.toml", None, 0, |_| {}).clicked() {
                    state.file = "Cargo.toml".into();
                }
            });
            text!(ui, "Selected: {}", state.file).color(Color::Muted);
        };
        panel(ui, "Color picker").description("Hue, saturation, brightness and named swatches, ported from MyGO.")
        + |ui|
        {
            row(ui).gap(16.).align(Align::Start) + |ui| {
                column(ui).gap(8.).width(260.) + |ui| {
                    if color_picker(ui, &mut state.tint) {
                        ui.request_rebuild();
                    }
                };
                column(ui).gap(8.).grow(1.) + |ui| {
                    text(ui, "Preview").font_size(12).color(Color::Muted);
                    column(ui).width(140.).height(80.).radius(8.).background(state.tint)
                        .border(1., Color::Border) + |_ui|
                    {};
                };
            };
        };
        panel(ui, "Form fields").description(
            "A label above its control with help text and a danger error, ported from MyGO.") + |ui|
        {
            form(ui, |ui| {
                let error = if !state.email.is_empty() && !state.email.contains('@') {
                    "Enter an email address."
                } else {
                    ""
                };
                let _ = field(ui, "Email").description("For receipts.").error(error) + |ui| {
                    text_input_string(ui, "Email", &mut state.email);
                };
                let _ = field(ui, "Copies").description("How many to print.") + |ui| {
                    stepper(ui, &mut state.copies, 1.0..=99.0, 1.0);
                };
            });
        };
        panel(ui, "Radios, links and history").description(
            "Radio buttons, a text link and router history buttons, ported from MyGO.") + |ui|
        {
            radio_group(ui, |ui| {
                for size in ["Small", "Medium", "Large"] {
                    if radio(ui, size, state.size == size).clicked() {
                        state.size = size.into();
                    }
                }
            });
            if link(ui, "Open the workspace").clicked() {
                router.go(crate::Page::Workspace);
                ui.request_rebuild();
            }
            row(ui).gap(4.) + |ui| {
                back_button(ui, router);
                forward_button(ui, router);
            };
        };
        panel(ui, "Date input").description("An ISO date field with a calendar popover, ported from MyGO.") + |ui| {
            if date_input(ui, "Birthday", &mut state.birthday, &mut state.birthday_open) {
                ui.request_rebuild();
            }
            text!(
                ui,
                "Chosen {}-{:02}-{:02}",
                state.birthday.year(),
                state.birthday.month(),
                state.birthday.day()
            ).color(Color::Muted);
        };
        panel(ui, "Find bar").description(
            "A query field with match status, previous/next and Done, ported from MyGO.") + |ui|
        {
            const SAMPLE: &str = "the quick brown fox jumps over the lazy dog; the fox naps";
            if button(ui).text(if state.finding { "Close find bar" } else { "Open find bar" }).ghost().clicked() {
                state.finding = !state.finding;
            }
            let matches = if state.find_query.is_empty() {
                0
            } else {
                SAMPLE.matches(state.find_query.as_str()).count()
            };
            if find_bar(ui, &mut state.finding, &mut state.find_query, matches, &mut state.find_current).changed() {
                ui.request_rebuild();
            }
            text!(ui, "Sample: {SAMPLE}").color(Color::Muted);
            if !state.find_query.is_empty() && matches > 0 {
                text!(ui, "Match {} of {matches}", state.find_current + 1).color(Color::Muted);
            }
        };
        panel(ui, "Alert dialog").description("A modal alert returning the chosen button, ported from MyGO.") + |ui| {
            if button(ui).text("Delete notes…").ghost().clicked() {
                state.alert.open();
            }
            text!(
                ui,
                "Deleted {} times; dismissed {} times",
                state.deleted,
                state.dismissed
            ).color(Color::Muted);
        };
        panel(ui, "Accordion").description("A bordered stack of collapsible sections, ported from MyGO.") + |ui| {
            accordion(ui, |ui| {
                let _ = collapsible(ui, "General", &mut state.general_open, |ui| {
                    text(ui, "General settings live here.").color(Color::Muted);
                });
                let _ = collapsible(ui, "Privacy", &mut state.privacy_open, |ui| {
                    text(ui, "Privacy settings live here.").color(Color::Muted);
                });
            });
        };
        panel(ui, "Time input").description(
            "A validated HH:MM field with canonical Enter formatting, ported from MyGO.") + |ui|
        {
            if time_input(ui, "Meeting", &mut state.meeting) {
                ui.request_rebuild();
            }
            text!(ui, "Meeting at {}", state.meeting).color(Color::Muted);
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
    match alert_dialog(ui, &mut state.alert, "Delete notes?", "You can't undo this.", &["Cancel", "Delete"]) {
        1 => state.deleted += 1,
        ALERT_DISMISSED => state.dismissed += 1,
        _ => {}
    }
}
