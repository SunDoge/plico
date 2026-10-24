use crate::components::{icon_button, project_tile};
use plico_core::widgets::{checkbox, column, panel, row, switch, text};
use plico_core::{Color, Justify, UiContext, component};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Preferences {
    pub notifications: bool,
    pub autosave: bool,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub draft: Preferences,
    pub saved: Preferences,
    pub archived: bool,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub projects: Vec<Project>,
    pub selected: u64,
}
impl Default for Workspace {
    fn default() -> Self {
        let prefs = Preferences {
            notifications: true,
            autosave: false,
        };
        Self {
            projects: [
                (1, "Plico", "Interface toolkit"),
                (2, "Docs", "Developer guide"),
                (3, "Sandbox", "Experiments"),
            ]
            .into_iter()
            .map(|(id, name, description)| Project {
                id,
                name: name.into(),
                description: description.into(),
                draft: prefs,
                saved: prefs,
                archived: false,
            })
            .collect(),
            selected: 1,
        }
    }
}
impl Workspace {
    #[component]
    #[rustfmt::skip]
    pub fn show(&mut self, ui: &mut UiContext) {
        let m = ui.theme().metrics;
        column(ui).gap(m.space(2)) + |ui| {
            text(ui, "Project workspace").font_size(m.title_size).weight(700);
            text(ui, "Choose a project, adjust its preferences, and save your changes.").font_size(m.label_size).color(Color::Muted);
        };
        panel(ui, "Projects").padding(m.space(4)) + |ui| {
            let active = self.projects.iter().filter(|p| !p.archived).count();
            if active == 0 {
                column(ui).gap(m.space(3)).children(|ui| {
                    text(ui, "No active projects").font_size(m.font_size).weight(600);
                    text(ui, "Your projects are archived. Restore them to continue.").font_size(m.label_size)
                        .color(Color::Muted);
                    if icon_button(ui, "Restore projects", &crate::icons::ARCHIVE_RESTORE, true, false).clicked()
                    {
                        for p in &mut self.projects {
                            p.archived = false;
                        }
                        self.selected = self.projects.first().map_or(0, |p| p.id);
                    }
                })
            } else {
                row(ui).gap(m.space(3)).wrap(true).children(|ui| {
                    for project in self.projects.iter().filter(|p| !p.archived) {
                        if ui.keyed(project.id, |ui| {
                            project_tile(ui, &project.name, &project.description, self.selected == project.id)
                        }).clicked()
                        {
                            self.selected = project.id;
                        }
                    }
                })
            }
        };
        if let Some(index) = self.projects.iter().position(|p| p.id == self.selected && !p.archived)
        {
            let project = &mut self.projects[index];
            let archive = panel(ui, "Project preferences").children(|ui| {
                row(ui).justify(Justify::SpaceBetween).gap(m.space(2)) + |ui| {
                    text(ui, project.name.as_str()).font_size(m.font_size).weight(650);
                    text(ui,
                        if project.draft != project.saved { "Unsaved changes" } else { "All changes saved" }).font_size(m.label_size)
                        .color(if project.draft != project.saved { Color::Accent } else { Color::Muted });
                };
                switch(ui, "Project notifications", &mut project.draft.notifications);
                checkbox(ui, "Save automatically", &mut project.draft.autosave);
                let dirty = project.draft != project.saved;
                let mut archive = false;
                row(ui).gap(m.space(2)).wrap(true) + |ui| {
                    if icon_button(ui, "Save changes", &crate::icons::SAVE, dirty, true).clicked()
                    {
                        project.saved = project.draft;
                    }
                    if icon_button(ui, "Reset changes", &crate::icons::ROTATE_CCW, dirty, false).clicked()
                    {
                        project.draft = project.saved;
                    }
                    archive = icon_button(ui, "Archive project", &crate::icons::ARCHIVE, true, false).clicked();
                };
                archive
            }).inner;
            if archive {
                project.archived = true;
                self.selected = self.projects.iter().find(|p| !p.archived).map_or(0, |p| p.id);
            }
        }
        text(ui, "Preferences and archived projects are kept for this session.").font_size(m.label_size).color(Color::Muted);
    }
}
