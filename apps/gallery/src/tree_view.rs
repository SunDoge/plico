//! Port of Wabou gallery's tree-view.tsx (Apache-2.0), composed from ordinary
//! Plico buttons. This page does not introduce a backend widget or an ARIA tree.
use plico::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
pub struct TreeNode {
    pub id: &'static str,
    pub label: &'static str,
    pub disabled: bool,
    pub children: &'static [TreeNode],
}
impl TreeNode {
    const fn leaf(id: &'static str, label: &'static str) -> Self {
        Self {
            id,
            label,
            disabled: false,
            children: &[],
        }
    }
    const fn branch(id: &'static str, label: &'static str, children: &'static [TreeNode]) -> Self {
        Self {
            id,
            label,
            disabled: false,
            children,
        }
    }
}
pub const WORKSPACE: &[TreeNode] = &[TreeNode::branch(
    "workspace",
    "Plico",
    &[
        TreeNode::branch(
            "crates",
            "crates",
            &[
                TreeNode::leaf("core", "plico"),
                TreeNode::leaf("runtime", "plico-runtime"),
                TreeNode::leaf("renderer", "plico-renderer"),
            ],
        ),
        TreeNode::branch(
            "apps",
            "apps",
            &[
                TreeNode::leaf("gallery", "gallery"),
                TreeNode::leaf("launcher", "launcher"),
            ],
        ),
        TreeNode {
            id: "target",
            label: "target (ignored)",
            disabled: true,
            children: &[],
        },
    ],
)];
pub const DEPLOYMENTS: &[TreeNode] = &[
    TreeNode::branch(
        "production",
        "Production",
        &[
            TreeNode::leaf("api", "API service"),
            TreeNode::leaf("worker", "Background worker"),
        ],
    ),
    TreeNode::leaf("preview", "Preview"),
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeState {
    pub expanded: BTreeSet<String>,
    pub selected: Option<String>,
}
impl Default for TreeState {
    fn default() -> Self {
        Self {
            expanded: ["workspace".into(), "crates".into()].into(),
            selected: Some("core".into()),
        }
    }
}
impl TreeState {
    pub fn deployment() -> Self {
        Self {
            expanded: ["production".into()].into(),
            selected: None,
        }
    }
    pub fn toggle(&mut self, nodes: &[TreeNode], id: &str) -> bool {
        let Some(node) = find(nodes, id) else {
            return false;
        };
        if node.disabled || node.children.is_empty() {
            return false;
        }
        if !self.expanded.remove(id) {
            self.expanded.insert(id.to_owned());
        }
        true
    }
    pub fn select(&mut self, nodes: &[TreeNode], id: &str) -> bool {
        if find(nodes, id).is_none_or(|n| n.disabled) {
            return false;
        }
        self.selected = Some(id.to_owned());
        true
    }
    pub fn expand_all(&mut self, nodes: &[TreeNode]) {
        for node in nodes {
            if !node.children.is_empty() && !node.disabled {
                self.expanded.insert(node.id.into());
                self.expand_all(node.children);
            }
        }
    }
    pub fn visible(&self, nodes: &'static [TreeNode]) -> Vec<(&'static TreeNode, usize)> {
        fn collect(
            state: &TreeState,
            nodes: &'static [TreeNode],
            depth: usize,
            out: &mut Vec<(&'static TreeNode, usize)>,
        ) {
            for node in nodes {
                out.push((node, depth));
                if state.expanded.contains(node.id) && !node.disabled {
                    collect(state, node.children, depth + 1, out);
                }
            }
        }
        let mut rows = Vec::new();
        collect(self, nodes, 0, &mut rows);
        rows
    }
}
pub fn find<'a>(nodes: &'a [TreeNode], id: &str) -> Option<&'a TreeNode> {
    nodes.iter().find_map(|n| {
        if n.id == id {
            Some(n)
        } else {
            find(n.children, id)
        }
    })
}
fn label<'a>(node: &TreeNode, l: &'a dyn plico::Localizer) -> std::borrow::Cow<'a, str> {
    match node.id {
        "production" => l.text("tree-production", "Production"),
        "api" => l.text("tree-api", "API service"),
        "worker" => l.text("tree-worker", "Background worker"),
        "preview" => l.text("tree-preview", "Preview"),
        _ => std::borrow::Cow::Borrowed(node.label),
    }
}

#[component]
#[rustfmt::skip]
pub fn page(cx: &mut UiContext) {
    let l = cx.localizer();
    crate::heading(cx, &l.text("tree-title", "Tree navigation"),
        &l.text("tree-description", "Explore a workspace and customize how each row is presented."));
    let files = cx.persisted_state("workspace-tree", TreeState::default);
    let deployments = cx.persisted_state("deployment-tree", TreeState::deployment);
    let mut files_state = files.get();
    let mut deployment_state = deployments.get();
    column(cx).height(0.).grow(1.).scroll_y(true).gap(20) + |cx| {
        row(cx).gap(20).wrap(true).align(Align::Start) + |cx| {
            column(cx).flex_basis(360.).grow(1.).min_width(280.).padding(20).gap(16).radius(12.)
                .background(Color::Panel).border(1., Color::Border) + |cx|
            {
                text(cx, l.text("tree-workspace", "Workspace navigation")).font_size(18).weight(600);
                row(cx).gap(8).wrap(true) + |cx| {
                    if button(cx).text(l.text("tree-expand-all", "Expand all")).ghost().font_size(13)
                        .clicked()
                    {
                        files_state.expand_all(WORKSPACE);
                    }
                    if button(cx).text(l.text("tree-collapse-all", "Collapse all")).ghost()
                        .font_size(13).clicked()
                    {
                        files_state.expanded.clear();
                    }
                };
                separator(cx);
                cx.keyed("files", |cx| tree(cx, WORKSPACE, &mut files_state, false));
                separator(cx);
                selection(cx, WORKSPACE, &files_state);
            };
            column(cx).flex_basis(320.).grow(1.).min_width(280.).padding(20).gap(16).radius(12.)
                .background(Color::Panel).border(1., Color::Border) + |cx|
            {
                text(cx, l.text("tree-custom", "Application-rendered rows")).font_size(18).weight(600);
                text(
                    cx,
                    l.text("tree-custom-description",
                        "The same navigation, with service labels and badges."),
                ).font_size(14).color(Color::Muted);
                separator(cx);
                cx.keyed("deployments", |cx| {
                    tree(cx, DEPLOYMENTS, &mut deployment_state, true)
                });
                separator(cx);
                selection(cx, DEPLOYMENTS, &deployment_state);
            };
        };
        text(cx, l
            .text("tree-hint", "Use Tab to move between controls and Enter or Space to activate. Collapsing a folder keeps its selection."))
            .font_size(14).color(Color::Muted);
    };
    files.set(files_state);
    deployments.set(deployment_state);
}
#[component]
#[rustfmt::skip]
fn tree(cx: &mut UiContext, roots: &'static [TreeNode], state: &mut TreeState, custom: bool) {
    let l = cx.localizer();
    column(cx).gap(2) + |cx| {
        for (node, depth) in state.visible(roots) {
            cx.keyed(node.id, |cx| {
                let branch = !node.children.is_empty();
                let expanded = state.expanded.contains(node.id);
                let selected = state.selected.as_deref() == Some(node.id);
                let title = label(node, l);
                row(cx).height(36.).padding_left(px(depth as f32 * 16.)).gap(2).align(Align::Center) + |cx| {
                    if branch {
                        let action = if expanded {
                            l.text("tree-collapse", "Collapse")
                        } else {
                            l.text("tree-expand", "Expand")
                        };
                        let name = format!("{action} {title}");
                        if button(cx).key("toggle").text("").icon(if expanded {
                            &plico::icons::CHEVRON_DOWN
                        } else {
                            &plico::icons::CHEVRON_RIGHT
                        }).aria_label(name).width(24.).height(30.).padding(0).ghost()
                            .disabled(node.disabled).clicked()
                        {
                            state.toggle(roots, node.id);
                        }
                    } else {
                        column(cx).width(24.).shrink(0.).children(|_| {});
                    }
                    let item = button_base(cx, title.as_ref()).key("item").width(0.).grow(1.).min_width(0.)
                        .height(34.).padding_x(8).gap(8).align(Align::Center).radius(6.).selected(selected)
                        .disabled(node.disabled).opacity(if node.disabled { 0.45 } else { 1. })
                        .background(if selected { Color::Selection } else { Color::Panel })
                        .hover_background(Color::ControlHover).pressed_background(Color::ControlPressed)
                        .color(if selected { Color::Accent } else { Color::Foreground });
                    let item = if branch { item.expanded(expanded) } else { item };
                    let clicked = item.children(|cx| {
                        let image = if branch {
                            if expanded { &crate::legacy_icons::FOLDER_OPEN } else { &crate::icons::FOLDER }
                        } else {
                            &crate::legacy_icons::FILE_TEXT
                        };
                        svg(cx, image).size(16., 16.);
                        text(cx, title.as_ref()).font_size(14).weight(if selected { 600 } else { 400 })
                            .width(0.).grow(1.).min_width(0.).ellipsis(true);
                        if custom {
                            let kind = if branch {
                                l.text("tree-group", "Group")
                            } else {
                                l.text("tree-service", "Service")
                            };
                            text(cx, kind).font_size(12).padding_x(6).padding_y(3).radius(4.)
                                .background(Color::Control).color(Color::Muted);
                        }
                    }).response.clicked();
                    if clicked {
                        state.select(roots, node.id);
                    }
                };
            });
        }
    };
}
#[component]
#[rustfmt::skip]
fn selection(cx: &mut UiContext, nodes: &[TreeNode], state: &TreeState) {
    let l = cx.localizer();
    row(cx).gap(8).wrap(true).align(Align::Center) + |cx| {
        text(cx, l.text("tree-selected", "Selected node")).font_size(13).color(Color::Muted);
        let name = state.selected.as_deref().and_then(|id| find(nodes, id));
        let title = name.map_or_else(|| l.text("tree-none", "None"), |node| label(node, l));
        text(cx, title).font_size(14).weight(600);
    };
}
