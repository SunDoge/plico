use crate::components::{icon_button, project_tile};
use plico::prelude::*;

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
        let l = ui.localizer();
        let m = ui.theme().metrics;
        column(ui).gap(m.space(2)) + |ui| {
            text(ui, l.text("project-workspace", "Project workspace")).font_size(m.title_size).weight(700);
            text(
                ui,
                l.text("choose-a-project-adjust-its-preferences-and-save-your-changes",
                    "Choose a project, adjust its preferences, and save your changes."),
            ).font_size(m.font_size).color(Color::Muted);
        };
        column(ui).height(0.).grow(1.).scroll_y(true).gap(m.space(7)).padding_y(m.space(2)).padding_right(m.space(6))
        + |ui|
        {
            column(ui).gap(m.space(3)) + |ui| {
                text(ui, l.text("projects", "Projects")).font_size(m.font_size + 2).weight(650);
                let active = self.projects.iter().filter(|p| !p.archived).count();
                if active == 0 {
                    panel(ui, None) + |ui| {
                        column(ui).gap(m.space(3)).padding(m.space(3)) + |ui| {
                            icon(ui, &crate::icons::FOLDER).size(28., 28.).color(Color::Muted);
                            text(ui, l.text("no-active-projects", "No active projects")).font_size(m.font_size + 2).weight(600);
                            text(ui, l.text("your-projects-are-archived-restore-them-to-continue",
                                "Your projects are archived. Restore them to continue.")).font_size(m.font_size)
                                .color(Color::Muted);
                            if icon_button(ui, &l.text("restore-projects", "Restore projects"),
                                &crate::icons::ARCHIVE_RESTORE, true, false).clicked()
                            {
                                for p in &mut self.projects {
                                    p.archived = false;
                                }
                                self.selected =
                                    self.projects.first().map_or(0, |p| p.id);
                            }
                        };
                    };
                } else {
                    row(ui).gap(m.space(3)).wrap(true) + |ui| {
                        for project in self.projects.iter().filter(|p| !p.archived) {
                            if ui.keyed(project.id, |ui| {
                                project_tile(ui, &project.name, &project.description,
                                    self.selected == project.id)
                            }).clicked()
                            {
                                self.selected = project.id;
                            }
                        }
                    };
                }
            };
            if let Some(index) = self.projects.iter().position(|p| p.id == self.selected && !p.archived) {
                let project = &mut self.projects[index];
                settings_section(ui, &l.text("project-preferences", "Project preferences")).description(&l.text(
                    "workspace-preferences-description", "Changes apply only to the selected project.")) + |ui|
                {
                    row(ui).padding_y(m.space(4)).gap(m.space(2)).align(Align::Center) + |ui| {
                        icon(ui, &crate::icons::FOLDER).size(18., 18.).color(Color::Accent);
                        text(ui, project.name.as_str()).font_size(m.font_size).weight(650).grow(1.);
                        text(ui, l.text("workspace-selected-project", "Selected project")).font_size(m.font_size).color(Color::Muted);
                    };
                    separator(ui);
                    settings_row(ui, &l.text("workspace-updates", "Updates")).description(
                        &l.text("workspace-updates-description",
                            "Activity and important changes in this project."),
                    ) + |ui|
                    {
                        switch(ui, l.text("project-notifications", "Project notifications"),
                            &mut project.draft.notifications);
                    };
                    separator(ui);
                    settings_row(ui, &l.text("workspace-editing", "Editing")).description(
                        &l.text("workspace-editing-description", "Allow this project to save automatically.")) + |ui|
                    {
                        checkbox(ui, l.text("save-automatically", "Save automatically"),
                            &mut project.draft.autosave);
                    };
                    separator(ui);
                    let dirty = project.draft != project.saved;
                    row(ui).padding_y(m.space(4)).gap(m.space(3)).wrap(true).align(Align::Center) + |ui| {
                        text(
                            ui,
                            if dirty {
                                l.text("unsaved-changes", "Unsaved changes")
                            } else {
                                l.text("all-changes-saved", "All changes saved")
                            },
                        ).font_size(m.font_size).color(if dirty { Color::Accent } else { Color::Muted })
                            .grow(1.);
                        row(ui).gap(m.space(2)).wrap(true) + |ui| {
                            if button(ui).text(l.text("reset-changes", "Reset changes")).ghost()
                                .disabled(!dirty).clicked()
                            {
                                project.draft = project.saved;
                            }
                            if icon_button(ui, &l.text("save-changes", "Save changes"),
                                &crate::icons::SAVE, dirty, true).clicked()
                            {
                                project.saved = project.draft;
                            }
                        };
                    };
                };
                // Secondary project actions live outside the save group.
                let archive = (row(ui).gap(m.space(2)) + |ui| {
                    icon_button(ui, &l.text("archive-project", "Archive project"), &crate::icons::ARCHIVE,
                        true, false).clicked()
                }).inner;
                if archive {
                    project.archived = true;
                    self.selected = self.projects.iter().find(|p| !p.archived).map_or(0, |p| p.id);
                }
            }
            text(
                ui,
                l.text("preferences-and-archived-projects-are-kept-for-this-session",
                    "Preferences and archived projects are kept for this session."),
            ).font_size(m.font_size).color(Color::Muted);
        };
    }
}
