//! Port of Wabou gallery's data-table.tsx sample and project fixture (Apache-2.0).
//! The six-row model is ordinary Rust; this is not a TanStack Table adapter.
use plico::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Active,
    Review,
    Paused,
}
impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Review => "Review",
            Self::Paused => "Paused",
        }
    }
    fn label(self, l: &dyn plico::Localizer) -> std::borrow::Cow<'_, str> {
        match self {
            Self::Active => l.text("table-active", "Active"),
            Self::Review => l.text("table-review", "Review"),
            Self::Paused => l.text("table-paused", "Paused"),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Project {
    pub id: &'static str,
    pub name: &'static str,
    pub owner: &'static str,
    pub status: Status,
    pub score: u16,
}
pub const PROJECTS: [Project; 6] = [
    Project {
        id: "router",
        name: "Router",
        owner: "Mina",
        status: Status::Active,
        score: 98,
    },
    Project {
        id: "cache",
        name: "Resource cache",
        owner: "Arun",
        status: Status::Review,
        score: 94,
    },
    Project {
        id: "table",
        name: "Data table",
        owner: "Iris",
        status: Status::Active,
        score: 91,
    },
    Project {
        id: "virtual",
        name: "Virtual lists",
        owner: "Noah",
        status: Status::Active,
        score: 88,
    },
    Project {
        id: "a11y",
        name: "Accessibility",
        owner: "Mina",
        status: Status::Review,
        score: 84,
    },
    Project {
        id: "editor",
        name: "Editor widget",
        owner: "Arun",
        status: Status::Paused,
        score: 76,
    },
];
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sort {
    #[default]
    Project,
    Owner,
    Status,
    Score,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableState {
    pub query: String,
    pub sort: Sort,
    pub descending: bool,
    pub selected: BTreeSet<String>,
}
impl Default for TableState {
    fn default() -> Self {
        Self {
            query: String::new(),
            sort: Sort::Score,
            descending: true,
            selected: BTreeSet::new(),
        }
    }
}
impl TableState {
    pub fn sort_by(&mut self, column: Sort) {
        if self.sort == column {
            self.descending = !self.descending;
        } else {
            self.sort = column;
            self.descending = false;
        }
    }
    pub fn visible(&self) -> Vec<&'static Project> {
        self.filtered(None)
    }
    fn visible_localized(&self, localizer: &dyn plico::Localizer) -> Vec<&'static Project> {
        self.filtered(Some(localizer))
    }
    fn filtered(&self, localizer: Option<&dyn plico::Localizer>) -> Vec<&'static Project> {
        let query = self.query.trim().to_lowercase();
        let mut rows: Vec<_> = PROJECTS
            .iter()
            .filter(|p| {
                query.is_empty()
                    || [p.name, p.owner, p.status.name()]
                        .iter()
                        .any(|s| s.to_lowercase().contains(&query))
                    || p.score.to_string().contains(&query)
                    || localizer.is_some_and(|l| p.status.label(l).to_lowercase().contains(&query))
            })
            .collect();
        rows.sort_by(|a, b| {
            let order = match self.sort {
                Sort::Project => a.name.cmp(b.name),
                Sort::Owner => a.owner.cmp(b.owner),
                Sort::Status => a.status.name().cmp(b.status.name()),
                Sort::Score => a.score.cmp(&b.score),
            };
            let order = if self.descending {
                order.reverse()
            } else {
                order
            };
            order.then_with(|| a.id.cmp(b.id))
        });
        rows
    }
    /// Select only the current filtered set; hidden selections remain untouched.
    pub fn select_visible(&mut self, checked: bool) {
        self.select_rows(&self.visible(), checked);
    }
    fn select_rows(&mut self, rows: &[&Project], checked: bool) {
        for project in rows {
            if checked {
                self.selected.insert(project.id.into());
            } else {
                self.selected.remove(project.id);
            }
        }
    }
}

// One track definition drives headers and rows, even when labels change language.
const COLUMNS: [Track; 4] = [
    minmax(TrackSize::Px(0.), TrackSize::Fr(2.4)),
    minmax(TrackSize::Px(0.), TrackSize::Fr(1.)),
    minmax(TrackSize::Px(0.), TrackSize::Fr(1.2)),
    minmax(TrackSize::Px(56.), TrackSize::Fr(0.7)),
];

#[component]
#[rustfmt::skip]
pub fn page(cx: &mut UiContext) {
    let l = cx.localizer();
    crate::heading(cx, &l.text("table-title", "Project table"),
        &l.text("table-description", "Filter, sort and select the projects you want to work on."));
    let storage = cx.persisted_state("projects", TableState::default);
    let mut state = storage.get();
    column(cx).height(0.).grow(1.).scroll_y(true).gap(16) + |cx| {
        column(cx).radius(12.).clip(true).background(Color::Panel).border(1., Color::Border) + |cx| {
            row(cx).padding(16).gap(12).wrap(true).align(Align::Center) + |cx| {
                let search = l.text("table-filter", "Filter projects");
                text_input_string(cx, search, &mut state.query).key("filter").placeholder(l.text(
                    "table-filter-placeholder", "Filter by project, owner or status…")).width(320.).max_width(percent(100.));
                if !state.query.is_empty()
                    && button(cx).text(l.text("table-clear-filter", "Clear filter")).ghost().clicked()
                {
                    state.query.clear();
                }
                column(cx).grow(1.).children(|_| {});
                text!(
                    cx,
                    "{} / {}",
                    state.visible_localized(l).len(),
                    PROJECTS.len()
                ).font_size(14).color(Color::Muted);
            };
            separator(cx);
            // Header sits outside the scrolling rows, so its column titles stay visible.
            grid(cx).grid_columns(COLUMNS).gap(12).padding_x(16).padding_y(8).background(Color::Control) + |cx| {
                sort_header(cx, &mut state, Sort::Project, &l.text("table-project", "Project"));
                sort_header(cx, &mut state, Sort::Owner, &l.text("table-owner", "Owner"));
                sort_header(cx, &mut state, Sort::Status, &l.text("table-status", "Status"));
                sort_header(cx, &mut state, Sort::Score, &l.text("table-score", "Score"));
            };
            let rows = state.visible_localized(l);
            column(cx).height(316.).scroll_y(true) + |cx| {
                if rows.is_empty() {
                    column(cx).height_percent(100.).align(Align::Center).justify(Justify::Center).gap(12) + |cx| {
                        text(cx, l.text("table-empty", "No matching projects")).font_size(17)
                            .weight(600);
                        text(cx, l.text("table-empty-hint", "Try a different name, owner or status.")).font_size(14).color(Color::Muted);
                        if button(cx).text(l.text("table-show-all", "Show all projects")).ghost()
                            .clicked()
                        {
                            state.query.clear();
                        }
                    };
                }
                for project in rows {
                    cx.keyed(project.id, |cx| project_row(cx, project, &mut state));
                }
            };
            separator(cx);
            row(cx).padding(16).gap(12).wrap(true).align(Align::Center) + |cx| {
                let rows = state.visible_localized(l);
                let mut all = !rows.is_empty()
                    && rows.iter().all(|p| state.selected.contains(p.id));
                if checkbox(cx, l.text("table-select-visible", "Select visible"), &mut all).disabled(rows.is_empty()).changed()
                {
                    state.select_rows(&rows, all);
                }
                column(cx).grow(1.).children(|_| {});
                let label = l.text("table-selected", "selected");
                text!(cx, "{} {}", state.selected.len(), label).font_size(14).color(Color::Muted);
                if button(cx).text(l.text("table-clear-selection", "Clear selection")).ghost()
                    .disabled(state.selected.is_empty()).clicked()
                {
                    state.selected.clear();
                }
            };
        };
        text(
            cx,
            l.text("table-selection-hint",
                "Selections stay with each project when you sort, filter or switch pages."),
        ).font_size(14).color(Color::Muted);
    };
    storage.set(state);
}

#[component]
#[rustfmt::skip]
fn sort_header(cx: &mut UiContext, state: &mut TableState, column: Sort, label: &str) {
    let active = state.sort == column;
    let clicked = button_base(cx, label).width_percent(100.).min_width(0.).padding_y(4)
        .padding_left(if column == Sort::Project { 32. } else { 0. }).gap(6).background(Color::Control)
        .hover_background(Color::ControlHover).radius(6.).children(|cx| {
        text(cx, label).font_size(13).weight(600).color(if active { Color::Foreground } else { Color::Muted });
        if active {
            svg(
                cx,
                if state.descending { &crate::legacy_icons::ARROW_DOWN } else { &crate::legacy_icons::ARROW_UP },
            ).size(14., 14.).color(Color::Accent);
        }
    }).clicked();
    if clicked {
        state.sort_by(column);
    }
}
#[component]
#[rustfmt::skip]
fn project_row(cx: &mut UiContext, project: &Project, state: &mut TableState) {
    let l = cx.localizer();
    let mut checked = state.selected.contains(project.id);
    grid(cx).grid_columns(COLUMNS).height(52.).shrink(0.).padding_x(16).gap(12).align(Align::Center)
        .background(if checked { Color::Selection } else { Color::Panel }) + |cx|
    {
        column(cx).min_width(0.) + |cx| {
            if checkbox(cx, project.name, &mut checked).changed() {
                if checked {
                    state.selected.insert(project.id.into());
                } else {
                    state.selected.remove(project.id);
                }
            }
        };
        text(cx, project.owner).min_width(0.).font_size(14).ellipsis(true);
        row(cx).min_width(0.).align(Align::Center) + |cx| {
            row(cx).gap(6).padding_x(8).padding_y(4).radius(6.).background(Color::Control).align(Align::Center) + |cx| {
                let dot = match project.status {
                    Status::Active => 0x16a34a,
                    Status::Review => 0xd97706,
                    Status::Paused => 0x71717a,
                };
                column(cx).width(6.).height(6.).radius(3.).background(Color::Literal(dot)).children(|_| {});
                text(cx, project.status.label(l)).font_size(13).ellipsis(true);
            };
        };
        text!(cx, "{}", project.score).min_width(0.).font_size(14).weight(600);
    };
}
