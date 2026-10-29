//! A native command palette. The catalog is synthetic; application entries are
//! previews, never shell commands. Native buffers and host actions are not serialized.
pub mod icons;
use plico::{Engine, Entity, InputEdit, prelude::*};
use plico_desktop::{HostAction, View, WindowCommand};
use plico_runtime::regions::RegionUi;
use std::collections::VecDeque;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Category {
    #[default]
    All,
    Applications,
    Commands,
}
#[derive(Clone, Copy)]
enum Action {
    Preview,
    Theme,
    Clear,
    Close,
}
struct Entry {
    id: &'static str,
    title: &'static str,
    detail: &'static str,
    keywords: &'static str,
    icon: &'static SvgAsset,
    action: Action,
}
static ENTRIES: &[Entry] = &[
    Entry {
        id: "code",
        title: "Visual Studio Code",
        detail: "代码与项目",
        keywords: "vscode editor code 编辑器 代码",
        icon: &icons::CODE,
        action: Action::Preview,
    },
    Entry {
        id: "terminal",
        title: "Terminal",
        detail: "终端",
        keywords: "terminal shell console 终端",
        icon: &icons::TERMINAL,
        action: Action::Preview,
    },
    Entry {
        id: "files",
        title: "Files",
        detail: "文件与目录",
        keywords: "files folder 文件 目录",
        icon: &icons::FOLDER,
        action: Action::Preview,
    },
    Entry {
        id: "browser",
        title: "Browser",
        detail: "浏览器",
        keywords: "browser web internet 浏览器",
        icon: &icons::GLOBE,
        action: Action::Preview,
    },
    Entry {
        id: "notes",
        title: "Notes",
        detail: "笔记",
        keywords: "notes writing 笔记",
        icon: &icons::NOTEBOOK,
        action: Action::Preview,
    },
    Entry {
        id: "calculator",
        title: "Calculator",
        detail: "计算器",
        keywords: "calculator math 计算器",
        icon: &icons::CALCULATOR,
        action: Action::Preview,
    },
    Entry {
        id: "calendar",
        title: "Calendar",
        detail: "日历",
        keywords: "calendar schedule 日历",
        icon: &icons::CALENDAR,
        action: Action::Preview,
    },
    Entry {
        id: "settings",
        title: "Settings",
        detail: "系统设置",
        keywords: "settings preferences 设置",
        icon: &icons::SETTINGS,
        action: Action::Preview,
    },
    Entry {
        id: "theme",
        title: "切换明暗主题",
        detail: "外观",
        keywords: "theme dark light 主题",
        icon: &icons::SUN_MOON,
        action: Action::Theme,
    },
    Entry {
        id: "clear",
        title: "清除搜索",
        detail: "回到全部结果",
        keywords: "clear reset 清除 搜索",
        icon: &icons::ROTATE_CCW,
        action: Action::Clear,
    },
    Entry {
        id: "close",
        title: "关闭启动器",
        detail: "Escape",
        keywords: "close quit exit 退出 关闭",
        icon: &icons::X,
        action: Action::Close,
    },
];

#[derive(Default)]
struct LauncherModel {
    query: TextBuffer,
    dark: bool,
    category: Category,
    matches: Vec<usize>,
    selected: Option<usize>,
    last_query: String,
    last_category: Category,
    reveal: Option<usize>,
    focus_search: bool,
    feedback: String,
    actions: VecDeque<HostAction>,
}
impl LauncherModel {
    fn new() -> Self {
        Self {
            dark: true,
            matches: (0..ENTRIES.len()).collect(),
            selected: Some(0),
            last_category: Category::All,
            ..Self::default()
        }
    }
    fn refresh(&mut self) {
        let query = self.query.text().unwrap_or_default();
        if query == self.last_query && self.category == self.last_category {
            return;
        }
        let normalized = query.trim().to_lowercase();
        self.matches.clear();
        for (i, entry) in ENTRIES.iter().enumerate() {
            let application = matches!(entry.action, Action::Preview);
            if (self.category == Category::Applications && !application)
                || (self.category == Category::Commands && application)
            {
                continue;
            }
            let matches = normalized.split_whitespace().all(|token| {
                entry.title.to_lowercase().contains(token)
                    || entry.keywords.contains(token)
                    || entry.detail.contains(token)
            });
            if matches {
                self.matches.push(i);
            }
        }
        // Prefix matches first; stable catalog order breaks ties deterministically.
        self.matches
            .sort_by_key(|i| !ENTRIES[*i].title.to_lowercase().starts_with(&normalized));
        self.selected = (!self.matches.is_empty()).then_some(0);
        self.last_query = query;
        self.last_category = self.category;
        self.feedback.clear();
        self.reveal = self.selected;
    }
    fn navigate(&mut self, action: InputNavigation) {
        self.focus_search = false;
        if action == InputNavigation::Cancel {
            if !self.query.is_empty() {
                self.query.clear();
                self.refresh();
            } else {
                self.actions
                    .push_back(HostAction::Window(WindowCommand::Close));
            }
            return;
        }
        if self.matches.is_empty() {
            return;
        }
        let step = match action {
            InputNavigation::Previous => -1,
            InputNavigation::Next => 1,
            InputNavigation::PageUp => -6,
            InputNavigation::PageDown => 6,
            InputNavigation::Cancel => unreachable!(),
        };
        self.selected = Some(
            self.selected
                .unwrap_or(0)
                .saturating_add_signed(step)
                .min(self.matches.len() - 1),
        );
        self.reveal = self.selected;
    }
    fn activate(&mut self, index: usize) {
        let Some(&id) = self.matches.get(index) else {
            return;
        };
        self.selected = Some(index);
        let entry = &ENTRIES[id];
        match entry.action {
            Action::Preview => {
                self.feedback = format!("已预览 {} · 演示不会启动外部程序", entry.title)
            }
            Action::Theme => {
                self.dark = !self.dark;
                self.feedback = "已切换主题".into();
            }
            Action::Clear => {
                self.query.clear();
                self.refresh();
            }
            Action::Close => self
                .actions
                .push_back(HostAction::Window(WindowCommand::Close)),
        }
        self.focus_search = true;
    }
}

pub struct Launcher {
    regions: plico_runtime::regions::Regions,
    model: plico_runtime::regions::State<LauncherModel>,
}
impl Default for Launcher {
    fn default() -> Self {
        use plico_runtime::regions::Regions;
        let mut model = None;
        let regions = Regions::new(|app| {
            let state = app.state(LauncherModel::new);
            model = Some(state);
            app.root((state,), |cx, (state,)| application(cx, state.handle()));
        });
        Self {
            regions,
            model: model.expect("Launcher root state"),
        }
    }
}
impl Launcher {
    pub fn selected_id(&self) -> Option<&'static str> {
        let access = self.regions.access(self.model);
        let model = access.read();
        model
            .selected
            .and_then(|i| model.matches.get(i))
            .map(|i| ENTRIES[*i].id)
    }
    pub fn result_count(&self) -> usize {
        self.regions.access(self.model).read().matches.len()
    }
    pub fn feedback(&self) -> String {
        self.regions.access(self.model).read().feedback.clone()
    }
    pub fn dark(&self) -> bool {
        self.regions.access(self.model).read().dark
    }
    pub fn query_is_empty(&self) -> bool {
        self.regions.access(self.model).read().query.is_empty()
    }
    pub fn query_is_composing(&self) -> bool {
        self.regions.access(self.model).read().query.is_composing()
    }
    pub fn clear_query(&mut self) {
        self.regions.access(self.model).update(|model| {
            model.query.clear();
            model.refresh();
        });
    }
    pub fn set_category(&mut self, category: Category) {
        self.regions.access(self.model).update(|model| {
            model.category = category;
            model.refresh();
            model.focus_search = true;
        });
    }
    pub fn set_dark(&mut self, dark: bool) {
        self.regions
            .access(self.model)
            .update(|model| model.dark = dark);
    }
    pub fn edit_query(&mut self, edit: InputEdit) {
        self.regions.access(self.model).update(|model| {
            model.query.edit(edit);
            model.refresh();
        });
    }
}

#[component]
#[rustfmt::skip]
fn application(cx: &mut UiContext, model: plico_runtime::regions::State<LauncherModel>) {
    column(cx).width_percent(100.).height_percent(100.).background(Color::Surface) + |cx| {
        search(cx, model);
        separator(cx);
        filters(cx, model);
        results(cx, model);
        column(cx).grow(1.).end();
        separator(cx);
        cx.region(model, |cx, model| {
            let model = model.read();
            row(cx).height(30.).padding_x(22).gap(8).align(Align::Center) + |cx| {
                text(cx, if model.feedback.is_empty() { "应用目录为演示数据" } else { &model.feedback }).font_size(11).color(Color::Muted);
            };
        });
        row(cx).height(40.).padding_x(22).gap(8).align(Align::Center).background(Color::Panel) + |cx| {
            icon(cx, &icons::COMMAND).size(14., 14.).color(Color::Muted);
            text(cx, "Plico").font_size(12).weight(650);
            column(cx).grow(1.).end();
            text(cx, "↑ ↓ 选择    ↵ 确认    esc 返回 / 关闭").font_size(11).color(Color::Muted);
        };
    };
}

#[component]
#[rustfmt::skip]
fn search(cx: &mut UiContext, model: plico_runtime::regions::State<LauncherModel>) {
    use plico_runtime::regions::{FocusOnce, OnChange, OnClick, OnNavigation, OnSubmit};
    cx.region(model, |cx, model| {
        row(cx).height(74.).padding_x(22).gap(16).align(Align::Center) + |cx| {
            icon(cx, &icons::SEARCH).size(22., 22.).color(Color::Muted);
            let state = model.handle();
            let current = model.read();
            text_input(cx, "搜索应用与命令", &current.query).placeholder("搜索应用与命令…").autofocus(true)
                .request_focus_once(current.focus_search, state, |state| {
                state.update_if(|model| {
                    model.focus_search = false;
                    false
                });
            }).navigation(true).width(0.).grow(1.).height(48.).font_size(20).padding(0)
                .border(0., Color::Border).background(Color::Surface)
                .on_change((state,), |(state,), _| state.update(LauncherModel::refresh))
                .on_submit((state,), |(state,)| {
                state.update(|model| {
                    if let Some(index) = model.selected {
                        model.activate(index);
                    }
                });
            }).on_navigation((state,), |(state,), action| {
                state.update(|model| model.navigate(action))
            });
            button_base(cx, "关闭").padding(8).radius(6.).hover_background(Color::Control).children(|cx| {
                icon(cx, &icons::X).size(16., 16.).color(Color::Muted);
            }).response.on_click(state, |state| {
                state.update(|model| {
                    model.actions.push_back(HostAction::Window(WindowCommand::Close));
                })
            });
        };
    });
}

#[component]
#[rustfmt::skip]
fn filters(cx: &mut UiContext, model: plico_runtime::regions::State<LauncherModel>) {
    use plico_runtime::regions::OnClickWith;
    cx.region(model, |cx, model| {
        let current = model.read();
        row(cx).padding_x(16).height(42.).gap(6).align(Align::Center) + |cx| {
            for (category, label) in [
                (Category::All, "全部"),
                (Category::Applications, "应用"),
                (Category::Commands, "命令"),
            ] {
                cx.keyed(label, |cx| {
                    button_base(cx, label).selected(current.category == category).padding_x(12).padding_y(5)
                        .radius(6.)
                        .background(if current.category == category { Color::Control } else { Color::Surface })
                        .hover_background(Color::Control).children(|cx| {
                        text(cx, label).font_size(12).weight(600);
                    }).response.on_click_with((model.handle(),), category, |(model,), category| {
                        model.update(|model| {
                            model.category = category;
                            model.refresh();
                            model.focus_search = true;
                        });
                    });
                });
            }
            column(cx).grow(1.).end();
            plico::text!(cx, "{} 个结果", current.matches.len()).font_size(11).color(Color::Muted);
        };
    });
}

#[component]
#[rustfmt::skip]
fn results(cx: &mut UiContext, model: plico_runtime::regions::State<LauncherModel>) {
    use plico_runtime::regions::{OnClickWith, RevealOnce};
    cx.region(model, |cx, model| {
        let current = model.read();
        if current.matches.is_empty() {
            let _ = column(cx).height(336.).justify(Justify::Center).align(Align::Center).gap(12) + |cx| {
                icon(cx, &icons::SEARCH).size(28., 28.).color(Color::Muted);
                text(cx, "没有找到匹配项").font_size(16).weight(600);
                text(cx, "试试应用名称，或按 Escape 清除搜索").font_size(12).color(Color::Muted);
            };
            return;
        }
        let state = model.handle();
        virtual_list(cx, current.matches.len(), 56.).height(336.).smooth_scroll(true)
            .reveal_once(current.reveal, state, |state| {
            state.update_if(|model| {
                model.reveal = None;
                false
            });
        }).rows(
            |i| ENTRIES[current.matches[i]].id,
            |cx, i| {
                let entry = &ENTRIES[current.matches[i]];
                let selected = current.selected == Some(i);
                let _ = column(cx).padding_x(10) + |cx| {
                    button_base(cx, entry.title).width_percent(100.).height(54.).padding_x(14).gap(14)
                        .radius(8.).align(Align::Center).selected(selected)
                        .background(if selected { Color::Selection } else { Color::Surface })
                        .hover_background(Color::Control).motion(Motion::Quick).children(|cx| {
                        column(cx).width(32.).height(32.).radius(8.).align(Align::Center)
                            .justify(Justify::Center).background(Color::Panel) + |cx|
                        {
                            icon(cx, entry.icon).size(19., 19.);
                        };
                        column(cx).grow(1.).gap(3) + |cx| {
                            text(cx, entry.title).font_size(14).weight(600);
                            text(cx, entry.detail).font_size(11).color(Color::Muted);
                        };
                        text(cx, if matches!(entry.action, Action::Preview) { "预览" } else { "执行" }).font_size(11).color(Color::Muted);
                        if selected {
                            icon(cx, &icons::CORNER_DOWN_LEFT).size(15., 15.).color(Color::Muted);
                        }
                    }).response.on_click_with((state,), i, |(state,), i| {
                        state.update(|model| model.activate(i));
                    });
                };
            },
        );
    });
}

impl View for Launcher {
    fn on_unhandled_key(&mut self, key: &plico_desktop::KeyEvent) -> bool {
        if key.key == "Escape" && key.modifiers.is_empty() && !key.repeat {
            self.regions.access(self.model).update(|model| {
                model.navigate(InputNavigation::Cancel);
                model.focus_search = true;
            });
            true
        } else {
            false
        }
    }
    fn take_host_action(&mut self) -> Option<HostAction> {
        let mut action = None;
        self.regions.access(self.model).update_if(|model| {
            action = model.actions.pop_front();
            action.is_some()
        });
        action
    }
    fn theme(&self) -> Theme {
        if self.dark() {
            Theme {
                surface: 0x20232a,
                panel: 0x2d313a,
                selection: 0x35445b,
                control: 0x2d323c,
                foreground: 0xf1f3f7,
                muted: 0xa3aebb,
                border: 0x393e48,
                ..Theme::dark()
            }
        } else {
            Theme::light()
        }
    }
    fn show(&mut self, cx: &mut UiContext) {
        View::show(&mut self.regions, cx);
    }
    fn replay_regions(
        &mut self,
        engine: &mut Engine,
    ) -> Option<Result<plico_runtime::regions::ReplayBatch, plico::FrameError>> {
        self.regions.replay_regions(engine)
    }
    fn scene_cache_roots(&self) -> Vec<Entity> {
        self.regions.scene_cache_roots()
    }
    fn set_wake_callback(&mut self, wake: plico_desktop::WakeCallback) {
        self.regions.set_wake_callback(wake);
    }
    fn poll_reload(&mut self) -> bool {
        self.regions.poll_reload()
    }
    fn snapshot_changed(&self) -> bool {
        self.regions.snapshot_changed()
    }
}
