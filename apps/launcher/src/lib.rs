//! A native command palette. The catalog is synthetic; application entries are
//! previews, never shell commands. Native buffers and host actions are not serialized.
pub mod icons;
use plico::prelude::*;
use plico_desktop::{HostAction, View, WindowCommand};
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
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

pub struct Launcher {
    pub query: TextBuffer,
    pub dark: bool,
    pub category: Category,
    matches: Vec<usize>,
    selected: Option<usize>,
    last_query: String,
    last_category: Category,
    reveal: bool,
    focus_search: bool,
    feedback: String,
    actions: VecDeque<HostAction>,
}
impl Default for Launcher {
    fn default() -> Self {
        Self {
            query: TextBuffer::new(),
            dark: true,
            category: Category::All,
            matches: (0..ENTRIES.len()).collect(),
            selected: Some(0),
            last_query: String::new(),
            last_category: Category::All,
            reveal: true,
            focus_search: false,
            feedback: String::new(),
            actions: VecDeque::new(),
        }
    }
}
impl Launcher {
    pub fn selected_id(&self) -> Option<&'static str> {
        self.selected
            .and_then(|i| self.matches.get(i))
            .map(|i| ENTRIES[*i].id)
    }
    pub fn result_count(&self) -> usize {
        self.matches.len()
    }
    pub fn feedback(&self) -> &str {
        &self.feedback
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
        self.reveal = true;
    }
    fn navigate(&mut self, action: InputNavigation) {
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
        self.reveal = true;
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
    #[component]
    #[rustfmt::skip]
    fn search(&mut self, cx: &mut UiContext) {
        row(cx).height(74.).padding_x(22).gap(16).align(Align::Center) + |cx| {
            icon(cx, &icons::SEARCH).size(22., 22.).color(Color::Muted);
            let response = text_input(cx, "搜索应用与命令", &self.query).placeholder("搜索应用与命令…").autofocus(true)
                .request_focus(std::mem::take(&mut self.focus_search)).navigation(true).width(0.).grow(1.)
                .height(48.).font_size(20).padding(0).border(0., Color::Border).background(Color::Surface)
                .response();
            if response.changed {
                self.refresh();
            }
            if let Some(action) = response.navigation {
                self.navigate(action);
            }
            if response.submitted
                && let Some(index) = self.selected
            {
                self.activate(index);
            }
            if button_base(cx, "关闭").padding(8).radius(6.).hover_background(Color::Control).children(|cx| {
                icon(cx, &icons::X).size(16., 16.).color(Color::Muted);
            }).response.clicked()
            {
                self.actions.push_back(HostAction::Window(WindowCommand::Close));
            }
        };
    }
    #[component]
    #[rustfmt::skip]
    fn filters(&mut self, cx: &mut UiContext) {
        row(cx).padding_x(16).height(42.).gap(6).align(Align::Center) + |cx| {
            for (category, label) in [
                (Category::All, "全部"),
                (Category::Applications, "应用"),
                (Category::Commands, "命令"),
            ] {
                cx.keyed(label, |cx| {
                    if button_base(cx, label).selected(self.category == category).padding_x(12).padding_y(5)
                        .radius(6.)
                        .background(if self.category == category { Color::Control } else { Color::Surface })
                        .hover_background(Color::Control).children(|cx| {
                        text(cx, label).font_size(12).weight(600);
                    }).response.clicked()
                    {
                        self.category = category;
                        self.refresh();
                        self.focus_search = true;
                    }
                });
            }
            column(cx).grow(1.) + |_cx| {};
            plico::text!(cx, "{} 个结果", self.matches.len()).font_size(11).color(Color::Muted);
        };
    }
    #[component]
    #[rustfmt::skip]
    fn results(&mut self, cx: &mut UiContext) {
        if self.matches.is_empty() {
            let _ = column(cx).height(336.).justify(Justify::Center).align(Align::Center).gap(12) + |cx| {
                icon(cx, &icons::SEARCH).size(28., 28.).color(Color::Muted);
                text(cx, "没有找到匹配项").font_size(16).weight(600);
                text(cx, "试试应用名称，或按 Escape 清除搜索").font_size(12).color(Color::Muted);
            };
            return;
        }
        let reveal = if std::mem::take(&mut self.reveal) { self.selected } else { None };
        let matches = &self.matches;
        let selected = self.selected;
        let mut clicked = None;
        virtual_list(cx, matches.len(), 56.).height(336.).smooth_scroll(true).reveal(reveal).rows(
            |i| ENTRIES[matches[i]].id,
            |cx, i| {
                let entry = &ENTRIES[matches[i]];
                let _ = column(cx).padding_x(10).children(|cx| {
                    if button_base(cx, entry.title).width_percent(100.).height(54.).padding_x(14).gap(14).radius(8.)
                        .align(Align::Center).selected(selected == Some(i))
                        .background(if selected == Some(i) { Color::Selection } else { Color::Surface })
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
                        if selected == Some(i) {
                            icon(cx, &icons::CORNER_DOWN_LEFT).size(15., 15.).color(Color::Muted);
                        }
                    }).response.clicked()
                    {
                        clicked = Some(i);
                    }
                });
            },
        );
        if let Some(i) = clicked {
            self.activate(i);
        }
    }
}
impl View for Launcher {
    fn on_unhandled_key(&mut self, key: &plico_desktop::KeyEvent) -> bool {
        if key.key == "Escape" && key.modifiers.is_empty() && !key.repeat {
            self.navigate(InputNavigation::Cancel);
            self.focus_search = true;
            true
        } else {
            false
        }
    }
    fn take_host_action(&mut self) -> Option<HostAction> {
        self.actions.pop_front()
    }
    fn theme(&self) -> Theme {
        if self.dark {
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
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        self.refresh();
        column(cx).width_percent(100.).height_percent(100.).background(Color::Surface) + |cx| {
            self.search(cx);
            separator(cx);
            self.filters(cx);
            self.results(cx);
            column(cx).grow(1.) + |_cx| {};
            separator(cx);
            row(cx).height(30.).padding_x(22).gap(8).align(Align::Center) + |cx| {
                text(cx, if self.feedback.is_empty() { "应用目录为演示数据" } else { &self.feedback }).font_size(11).color(Color::Muted);
            };
            row(cx).height(40.).padding_x(22).gap(8).align(Align::Center).background(Color::Panel).children(|cx| {
                icon(cx, &icons::COMMAND).size(14., 14.).color(Color::Muted);
                text(cx, "Plico").font_size(12).weight(650);
                column(cx).grow(1.) + |_cx| {};
                text(cx, "↑ ↓ 选择    ↵ 确认    esc 返回 / 关闭").font_size(11).color(Color::Muted);
            })
        };
    }
}
