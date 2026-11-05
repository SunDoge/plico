//! Hierarchical tree items ported from MyGO's `ui/tree.go` (the looked
//! `Tree`/`TreeItem` pair, not the look-less `TreeBase`).
//!
//! The application owns the disclosure and selection values; the widgets only
//! declare rows and report their clicks. Depth is explicit because Plico's
//! immediate API has no tree builder state to carry it.
use crate::prelude::*;
use crate::{WidgetRole, icons};

/// A tree (MyGO `Tree`): a column with the tree role holding the items
/// `content` declares, flush with the left edge. Items report their clicks;
/// nothing in the tree is chosen or opened by itself.
///
/// ```text
/// tree(cx, |cx| {
///     tree_item(cx, "src", chosen == "src", Some(&mut src_open), 0, |cx| {
///         tree_item(cx, "main.go", chosen == "main.go", None, 1, |_| {});
///     });
/// });
/// ```
pub fn tree(cx: &mut UiContext, content: impl FnOnce(&mut UiContext)) {
    column(cx)
        .gap(1.)
        .width_percent(100.)
        .semantic_role(WidgetRole::Tree)
        .children(|cx| content(cx));
}

/// One row of a [`tree`] (MyGO `TreeItem`): a disclosure arrow for branches,
/// then a selectable label. `depth` is 0 at the top and one deeper per level,
/// indenting the row by 16 logical pixels. `open` makes a branch: its arrow
/// toggles the value and `content` is declared only while it is true. A leaf
/// passes `None` and no content.
///
/// The label carries the tree-item role and its selected state; branches also
/// report `expanded`. The arrow is a separate button ("Expand"/"Collapse"),
/// because Plico does not allow controls nested in a control, so a branch row
/// has two focus stops where MyGO's row has one. MyGO's Up/Down/Home/End,
/// Right/Left and type-selection traversal is not ported.
///
/// Returns the label button's response so the caller can choose on
/// [`crate::Response::clicked`].
pub fn tree_item(
    cx: &mut UiContext,
    label: &str,
    selected: bool,
    open: Option<&mut bool>,
    depth: usize,
    content: impl FnOnce(&mut UiContext),
) -> crate::Response {
    let m = cx.theme().metrics;
    let mut open = open;
    let mut response = None;
    let mut row = row(cx)
        .width_percent(100.)
        .align(Align::Center)
        .gap(m.space(1));
    row = row.padding_x(m.space(1));
    row.children(|cx| {
        let indent = depth as f32 * 16.;
        if indent > 0. {
            let _ = column(cx).width(indent).height(1.).children(|_cx| {});
        }
        match open.as_deref_mut() {
            Some(open) => {
                let arrow = button_base(cx, if *open { "Collapse" } else { "Expand" })
                    .width(18.)
                    .height(24.)
                    .min_width(0.)
                    .min_height(0.)
                    .padding(0.)
                    .radius(m.radius as f32)
                    .hover_background(Color::ControlHover)
                    .pressed_background(Color::ControlPressed)
                    .children(|cx| {
                        let _ = svg(
                            cx,
                            if *open {
                                &icons::CHEVRON_DOWN
                            } else {
                                &icons::CHEVRON_RIGHT
                            },
                        )
                        .size(14., 14.)
                        .color(Color::Muted);
                    })
                    .response;
                if arrow.clicked() {
                    *open = !*open;
                    cx.request_rebuild();
                }
            }
            None => {
                let _ = column(cx).width(18.).height(1.).children(|_cx| {});
            }
        }
        let expanded = open.as_deref().copied().unwrap_or(false);
        let mut item = button_base(cx, label)
            .grow(1.)
            .min_width(0.)
            .height(24.)
            .padding_x(m.space(2))
            .gap(m.space(1))
            .radius(m.radius as f32)
            .selected(selected)
            .color(if selected {
                Color::Accent
            } else {
                Color::Foreground
            })
            .hover_background(Color::ControlHover)
            .pressed_background(Color::ControlPressed)
            .semantic_role(WidgetRole::TreeItem);
        // Unstyled when unselected so the tree background shows through;
        // button bases paint no background of their own.
        if selected {
            item = item.background(Color::Selection);
        }
        if open.is_some() {
            item = item.expanded(expanded);
        }
        response = Some(
            item.children(|cx| {
                let _ = text(cx, label)
                    .font_size(m.font_size)
                    .weight(if selected { 600 } else { 400 })
                    .ellipsis(true)
                    .min_width(0.);
            })
            .response,
        );
    });
    if open.as_deref().copied().unwrap_or(false) {
        content(cx);
    }
    response.expect("tree item declaration")
}
