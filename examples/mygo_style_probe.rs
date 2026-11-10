//! Deterministic reference sheet; scripts/compare-mygo-widgets.py runs both implementations.
use plico::prelude::*;
use plico::{CalendarState, WeekStart};
use plico_desktop::{
    NativeApp, PreviewError, PreviewScenario, PreviewStep, PreviewViewport, RenderBackend,
};
use plico_runtime::testing::Command;

#[derive(Default)]
struct Controls {
    dark: bool,
    value: String,
    checked: bool,
    selected: usize,
    calendar: Option<CalendarState>,
}

struct Families {
    dark: bool,
    tree_open: bool,
    general_open: bool,
    privacy_open: bool,
    selected: usize,
    select: PopoverState,
    tint: Color,
}
impl Default for Families {
    fn default() -> Self {
        Self {
            dark: false,
            tree_open: true,
            general_open: true,
            privacy_open: false,
            selected: 0,
            select: PopoverState::default(),
            tint: Color::Literal(0x3b82f6),
        }
    }
}
impl plico_dev::App for Families {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).padding(20).gap(12) + |cx| {
            text(cx, "MyGO component families");
            row(cx).gap(24).align(Align::Start) + |cx| {
                column(cx).width(320.).gap(12) + |cx| {
                    tree(cx, |cx| {
                        tree_item(cx, "src", false, Some(&mut self.tree_open), 0, |cx| {
                            tree_item(cx, "main.rs", true, None, 1, |_| {});
                            tree_item(cx, "lib.rs", false, None, 1, |_| {});
                        });
                        tree_item(cx, "Cargo.toml", false, None, 0, |_| {});
                    });
                    accordion(cx, |cx| {
                        collapsible(cx, "General", &mut self.general_open, |cx| {
                            text(cx, "Shared settings");
                        });
                        collapsible(cx, "Privacy", &mut self.privacy_open, |cx| {
                            text(cx, "Private settings");
                        });
                    });
                    let options = [
                        SelectOption::new("stable", 0, "Stable"),
                        SelectOption::new("beta", 1, "Beta"),
                        SelectOption::new("nightly", 2, "Nightly"),
                    ];
                    select(cx, &mut self.select, "Release", &mut self.selected, &options).width(220.);
                    tooltip(cx, "Reference tooltip") + |cx| {
                        button(cx).text("Tooltip anchor");
                    };
                };
                column(cx).width(400.).gap(12) + |cx| {
                    let columns = [TableColumn::new("Name", 1.), TableColumn::new("Status", 0.6)];
                    column(cx).width(400.).height(170.).clip(true) + |cx| {
                        table_header(cx, &columns);
                        table_row(cx, "Renderer", false, &columns, &["Renderer", "Ready"]);
                        table_row(cx, "Runtime", false, &columns, &["Runtime", "Active"]);
                        table_row(cx, "Desktop", false, &columns, &["Desktop", "Review"]);
                    };
                    color_picker(cx, &mut self.tint);
                };
            };
        };
    }
    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }
}
impl plico_dev::App for Controls {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).padding(20).gap(12) + |cx| {
            text(cx, "MyGO reference controls");
            row(cx).gap(8) + |cx| {
                button(cx).text("Ordinary");
                button(cx).text("Primary").primary();
                button(cx).text("Disabled").disabled(true);
            };
            toolbar(cx) + |cx| {
                button(cx).text("Toolbar action").ghost();
            };
            text_input_string(cx, "Name", &mut self.value).width(240.);
            checkbox(cx, "Remember this device", &mut self.checked);
            switch(cx, "", &mut self.checked);
            segmented(cx, &mut self.selected, &["Password", "Passphrase", "Username"]);
            sidebar(cx).width(240.).height(120.) + |cx| {
                sidebar_item(cx, "Inbox", true, None);
                sidebar_item(cx, "Archive", false, None);
            };
            let state = self.calendar.get_or_insert_with(|| {
                let date = CalendarDate::new(2026, 10, 8).unwrap();
                CalendarState::new(date).with_selected(date)
            });
            calendar(cx, state, WeekStart::Sunday);
            row(cx).gap(12).align(Align::Center) + |cx| {
                badge(cx, "12");
                avatar(cx).fallback("A");
                avatar(cx).fallback("花");
            };
            progress(cx, "Download", 0.5);
        };
    }
    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }
}
fn main() -> Result<(), PreviewError> {
    let output = std::path::PathBuf::from(
        std::env::var_os("PLICO_STYLE_OUTPUT").unwrap_or_else(|| "reports/mygo-widgets".into()),
    );
    for (name, dark) in [("light", false), ("dark", true)] {
        for (suffix, command) in [
            ("", None),
            ("-hover", Some(Command::PointerMove { x: 40., y: 166. })),
            (
                "-focus",
                Some(Command::Focus {
                    role: "textbox".into(),
                    label: "Name".into(),
                    occurrence: None,
                }),
            ),
        ] {
            let scenario: PreviewScenario<()> = PreviewScenario {
                viewport: PreviewViewport {
                    width: 560,
                    height: 760,
                    scale: 1.,
                },
                steps: command
                    .into_iter()
                    .map(|command| PreviewStep {
                        after_ms: 0,
                        command,
                    })
                    .collect(),
                ..Default::default()
            };
            scenario
                .render_with_backend(RenderBackend::Wgpu, |_| {
                    NativeApp(Controls {
                        dark,
                        value: "Agjpqy 中文 É".into(),
                        checked: true,
                        selected: 0,
                        calendar: None,
                    })
                })?
                .write_png(output.join(format!("plico-{name}{suffix}.png")))
                .map_err(|source| PreviewError::Capture { source })?;
        }
        let scenario: PreviewScenario<()> = PreviewScenario {
            viewport: PreviewViewport {
                width: 800,
                height: 760,
                scale: 1.,
            },
            ..Default::default()
        };
        scenario
            .render_with_backend(RenderBackend::Wgpu, |_| {
                NativeApp(Families {
                    dark,
                    tree_open: true,
                    general_open: true,
                    privacy_open: false,
                    selected: 0,
                    select: PopoverState::default(),
                    tint: Color::Literal(0x3b82f6),
                })
            })?
            .write_png(output.join(format!("plico-families-{name}.png")))
            .map_err(|source| PreviewError::Capture { source })?;
    }
    Ok(())
}
