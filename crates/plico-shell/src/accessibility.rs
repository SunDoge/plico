//! Window-level AccessKit adapter and retained accessibility tree publication.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use accesskit::{
    Action, ActionData, AriaCurrent, HasPopup, Node, NodeId, Rect, Role, Toggled, Tree, TreeId,
    TreeUpdate,
};
use accesskit_xplat::{Adapter, EventHandler, WindowEvent as AccessKitEvent};
use raw_window_handle::HasWindowHandle;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::{
    SemanticAction, SemanticCurrent, SemanticNode, SemanticPopup, SemanticRole, SemanticSnapshot,
    SemanticToggleState,
};

const ROOT_ID: NodeId = NodeId(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicationGeometry {
    width: u32,
    height: u32,
    scale_bits: u64,
    offset_x_bits: u64,
    offset_y_bits: u64,
}

fn publication_geometry(
    width: u32,
    height: u32,
    scale: f64,
    offset: [f64; 2],
) -> PublicationGeometry {
    PublicationGeometry {
        width,
        height,
        scale_bits: scale.to_bits(),
        offset_x_bits: offset[0].to_bits(),
        offset_y_bits: offset[1].to_bits(),
    }
}

#[cfg(test)]
fn accesskit_node(
    semantic: &SemanticNode,
    scale: f64,
    exposed: Option<&std::collections::HashSet<u64>>,
) -> Node {
    accesskit_node_with_offset(semantic, scale, [0., 0.], exposed)
}

fn accesskit_node_with_offset(
    semantic: &SemanticNode,
    scale: f64,
    offset: [f64; 2],
    exposed: Option<&std::collections::HashSet<u64>>,
) -> Node {
    let role = match semantic.role {
        SemanticRole::Generic => Role::GenericContainer,
        SemanticRole::Group => Role::Group,
        SemanticRole::Label => Role::Label,
        SemanticRole::Heading => Role::Heading,
        SemanticRole::Button => Role::Button,
        SemanticRole::TextInput => Role::TextInput,
        SemanticRole::Image => Role::Image,
        SemanticRole::RadioGroup => Role::RadioGroup,
        SemanticRole::Region => Role::Region,
        SemanticRole::Link => Role::Link,
        SemanticRole::List => Role::List,
        SemanticRole::ListItem => Role::ListItem,
        SemanticRole::Dialog => Role::Dialog,
        SemanticRole::AlertDialog => Role::AlertDialog,
        SemanticRole::Alert => Role::Alert,
        SemanticRole::Status => Role::Status,
        SemanticRole::Tooltip => Role::Tooltip,
        SemanticRole::CheckBox => Role::CheckBox,
        SemanticRole::RadioButton => Role::RadioButton,
        SemanticRole::Switch => Role::Switch,
        SemanticRole::ComboBox => Role::ComboBox,
        SemanticRole::ListBox => Role::ListBox,
        SemanticRole::Option => Role::ListBoxOption,
        SemanticRole::Menu => Role::Menu,
        SemanticRole::MenuBar => Role::MenuBar,
        SemanticRole::MenuItem => Role::MenuItem,
        SemanticRole::MenuItemCheckBox => Role::MenuItemCheckBox,
        SemanticRole::MenuItemRadio => Role::MenuItemRadio,
        SemanticRole::Tree => Role::Tree,
        SemanticRole::TreeItem => Role::TreeItem,
        SemanticRole::Toolbar => Role::Toolbar,
        SemanticRole::Table => Role::Table,
        SemanticRole::Row => Role::Row,
        SemanticRole::Cell => Role::Cell,
        SemanticRole::ColumnHeader => Role::ColumnHeader,
        SemanticRole::RowHeader => Role::RowHeader,
        SemanticRole::Separator => Role::Splitter,
        SemanticRole::Slider => Role::Slider,
        SemanticRole::SpinButton => Role::SpinButton,
        SemanticRole::ProgressBar => Role::ProgressIndicator,
        SemanticRole::TabList => Role::TabList,
        SemanticRole::Tab => Role::Tab,
        SemanticRole::TabPanel => Role::TabPanel,
        SemanticRole::Grid => Role::Grid,
        SemanticRole::GridCell => Role::GridCell,
        SemanticRole::Navigation => Role::Navigation,
        SemanticRole::Meter => Role::Meter,
        SemanticRole::ColorWell => Role::ColorWell,
    };
    let mut node = Node::new(role);
    if let Some(label) = &semantic.label {
        node.set_label(label.clone());
    }
    // AccessKit exposes the name of a static label through its value.
    let value = match role {
        Role::Label => semantic.value.as_ref().or(semantic.label.as_ref()),
        _ => semantic.value.as_ref(),
    };
    if let Some(value) = value {
        node.set_value(value.clone());
    }
    if let Some(value) = semantic.numeric_value {
        node.set_numeric_value(value);
    }
    if let Some(value) = semantic.min_numeric_value {
        node.set_min_numeric_value(value);
    }
    if let Some(value) = semantic.max_numeric_value {
        node.set_max_numeric_value(value);
    }
    let [x0, y0, x1, y1] = semantic.bounds.map(f64::from);
    let [offset_x, offset_y] = offset;
    let [x0, y0, x1, y1] = [
        (x0 + offset_x) * scale,
        (y0 + offset_y) * scale,
        (x1 + offset_x) * scale,
        (y1 + offset_y) * scale,
    ];
    node.set_bounds(Rect::new(x0, y0, x1, y1));
    let relation_is_exposed = |id: &u64| exposed.is_none_or(|ids| ids.contains(id));
    node.set_children(
        semantic
            .children
            .iter()
            .filter(|id| relation_is_exposed(id))
            .copied()
            .map(NodeId)
            .collect::<Vec<_>>(),
    );
    node.set_controls(
        semantic
            .controls
            .iter()
            .filter(|id| relation_is_exposed(id))
            .copied()
            .map(NodeId)
            .collect::<Vec<_>>(),
    );
    node.set_described_by(
        semantic
            .described_by
            .iter()
            .filter(|id| relation_is_exposed(id))
            .copied()
            .map(NodeId)
            .collect::<Vec<_>>(),
    );
    if let Some(active) = semantic.active_descendant.filter(relation_is_exposed) {
        node.set_active_descendant(NodeId(active));
    }
    if semantic.disabled {
        node.set_disabled();
    }
    if let Some(state) = semantic.states.checked.or(semantic.states.pressed) {
        node.set_toggled(match state {
            SemanticToggleState::Off => Toggled::False,
            SemanticToggleState::On => Toggled::True,
            SemanticToggleState::Mixed => Toggled::Mixed,
        });
    }
    if let Some(direction) = semantic.states.sort_direction {
        node.set_sort_direction(match direction {
            crate::SemanticSortDirection::Ascending => accesskit::SortDirection::Ascending,
            crate::SemanticSortDirection::Descending => accesskit::SortDirection::Descending,
            crate::SemanticSortDirection::Other => accesskit::SortDirection::Other,
        });
    }
    if let Some(selected) = semantic.states.selected {
        node.set_selected(selected);
    }
    if let Some(expanded) = semantic.states.expanded {
        node.set_expanded(expanded);
    }
    if let Some(current) = semantic.states.current {
        node.set_aria_current(match current {
            SemanticCurrent::True => AriaCurrent::True,
            SemanticCurrent::Page => AriaCurrent::Page,
            SemanticCurrent::Step => AriaCurrent::Step,
            SemanticCurrent::Location => AriaCurrent::Location,
            SemanticCurrent::Date => AriaCurrent::Date,
            SemanticCurrent::Time => AriaCurrent::Time,
        });
    }
    if let Some(popup) = semantic.states.popup {
        node.set_has_popup(match popup {
            SemanticPopup::Menu => HasPopup::Menu,
            SemanticPopup::ListBox => HasPopup::Listbox,
            SemanticPopup::Tree => HasPopup::Tree,
            SemanticPopup::Grid => HasPopup::Grid,
            SemanticPopup::Dialog => HasPopup::Dialog,
        });
    }
    if semantic.states.modal == Some(true) {
        node.set_modal();
    }
    if !semantic.disabled
        && matches!(
            semantic.role,
            SemanticRole::Slider | SemanticRole::SpinButton
        )
    {
        node.add_action(Action::SetValue);
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);
    }
    node.add_action(Action::ScrollIntoView);
    match semantic.role {
        SemanticRole::Button
        | SemanticRole::Link
        | SemanticRole::CheckBox
        | SemanticRole::RadioButton
        | SemanticRole::Switch
        | SemanticRole::ComboBox
        | SemanticRole::Option
        | SemanticRole::MenuItem
        | SemanticRole::MenuItemCheckBox
        | SemanticRole::MenuItemRadio
        | SemanticRole::TreeItem
        | SemanticRole::Tab
        | SemanticRole::GridCell
            if !semantic.disabled =>
        {
            node.add_action(Action::Click);
            node.add_action(Action::Focus);
        }
        SemanticRole::TextInput
        | SemanticRole::ListBox
        | SemanticRole::Slider
        | SemanticRole::SpinButton
            if !semantic.disabled =>
        {
            node.add_action(Action::Focus);
        }
        _ => {}
    }
    node
}

#[cfg(test)]
fn root_update(
    title: String,
    width: f64,
    height: f64,
    scale: f64,
    initialize: bool,
    snapshot: Option<&SemanticSnapshot>,
) -> TreeUpdate {
    root_update_with_offset(title, width, height, scale, [0., 0.], initialize, snapshot)
}

fn root_update_with_offset(
    title: String,
    width: f64,
    height: f64,
    scale: f64,
    offset: [f64; 2],
    initialize: bool,
    snapshot: Option<&SemanticSnapshot>,
) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_label(title);
    root.set_bounds(Rect::new(0.0, 0.0, width, height));
    // AccessKit requires every child ID in an update to resolve either in the
    // current tree or in the same update. A semantic snapshot can transiently
    // retain a child which is no longer projected (for example after
    // virtualization or conditional unmounting), so close the tree at this
    // platform boundary instead of allowing the consumer to panic.
    let exposed = snapshot
        .map(SemanticSnapshot::exposed_nodes)
        .unwrap_or_default();
    let exposed_ids = exposed.iter().map(|node| node.id).collect::<HashSet<_>>();
    let children = snapshot
        .map(SemanticSnapshot::exposed_root_children)
        .unwrap_or_default()
        .into_iter()
        .filter(|id| exposed_ids.contains(id))
        .collect::<Vec<_>>();
    root.set_children(children.iter().copied().map(NodeId).collect::<Vec<_>>());
    let mut tree = Tree::new(ROOT_ID);
    tree.toolkit_name = Some("Plico".into());
    let focus = snapshot
        .and_then(|snapshot| snapshot.focus)
        .filter(|id| exposed_ids.contains(id))
        .map(NodeId)
        .unwrap_or(ROOT_ID);
    let mut nodes = vec![(ROOT_ID, root)];
    nodes.extend(exposed.into_iter().map(|semantic| {
        (
            NodeId(semantic.id),
            accesskit_node_with_offset(semantic, scale, offset, Some(&exposed_ids)),
        )
    }));
    TreeUpdate {
        nodes,
        tree: initialize.then_some(tree),
        tree_id: TreeId::ROOT,
        focus,
    }
}

/// Reuse the platform tree when the exposed topology and window geometry
/// are stable. A structural change goes through the full, closed-tree builder.
fn update_stable_tree(
    previous: &SemanticSnapshot,
    current: &SemanticSnapshot,
    scale: f64,
    offset: [f64; 2],
) -> Option<TreeUpdate> {
    if previous.root_children != current.root_children
        || previous.modal_root != current.modal_root
        || previous.nodes.len() != current.nodes.len()
    {
        return None;
    }
    let mut changed = Vec::new();
    for (old, new) in previous.nodes.iter().zip(&current.nodes) {
        if old.id != new.id || old.children != new.children {
            return None;
        }
        if old != new {
            changed.push(new);
        }
    }
    let exposed: HashSet<_> = current
        .exposed_nodes()
        .into_iter()
        .map(|node| node.id)
        .collect();
    let nodes = changed
        .into_iter()
        .filter(|node| exposed.contains(&node.id))
        .map(|node| {
            (
                NodeId(node.id),
                accesskit_node_with_offset(node, scale, offset, Some(&exposed)),
            )
        })
        .collect();
    Some(TreeUpdate {
        nodes,
        tree: None,
        tree_id: TreeId::ROOT,
        focus: current
            .focus
            .filter(|id| exposed.contains(id))
            .map(NodeId)
            .unwrap_or(ROOT_ID),
    })
}

struct Handler {
    window: Arc<dyn Window>,
    events: Arc<Mutex<Vec<AccessKitEvent>>>,
}

impl EventHandler for Handler {
    fn handle_accesskit_event(&self, event: AccessKitEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
        self.window.request_redraw();
    }
}

/// Owns the platform adapter for Plico's semantic tree.
pub struct AccessibilityState {
    adapter: Adapter,
    events: Arc<Mutex<Vec<AccessKitEvent>>>,
    title: String,
    snapshot: Option<Arc<SemanticSnapshot>>,
    content_offset: [f64; 2],
    active: bool,
    initialize_pending: bool,
    last_published: Option<(PublicationGeometry, Option<Arc<SemanticSnapshot>>)>,
}

impl AccessibilityState {
    /// Attach an AccessKit adapter to `window` without publishing a tree yet.
    pub fn new(window: Arc<dyn Window>, title: String) -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        let handler = Arc::new(Handler {
            window: window.clone(),
            events: events.clone(),
        });
        let adapter = Adapter::with_combined_handler(
            window
                .window_handle()
                .expect("winit window must expose a raw window handle")
                .as_raw(),
            handler,
        );
        Self {
            adapter,
            events,
            title,
            snapshot: None,
            content_offset: [0., 0.],
            active: false,
            initialize_pending: false,
            last_published: None,
        }
    }

    /// Forward window focus, movement, and resize changes to AccessKit.
    pub fn process_window_event(&mut self, window: &dyn Window, event: &WindowEvent) {
        match event {
            WindowEvent::Focused(focused) => self.adapter.set_focus(*focused),
            WindowEvent::Moved(_) | WindowEvent::SurfaceResized(_) => {
                self.update_window_bounds(window)
            }
            _ => {}
        }
    }

    /// Replace the immutable semantic projection used by the next publication.
    pub fn set_snapshot(&mut self, snapshot: Option<Arc<SemanticSnapshot>>) {
        self.set_snapshot_with_offset(snapshot, [0., 0.]);
    }

    pub(crate) fn set_snapshot_with_offset(
        &mut self,
        snapshot: Option<Arc<SemanticSnapshot>>,
        content_offset: [f64; 2],
    ) {
        self.snapshot = snapshot;
        self.content_offset = content_offset;
    }

    /// Consume activation lifecycle events before a frame is built and report
    /// whether the frame source should produce semantic data.
    pub fn prepare_frame(&mut self) -> bool {
        if let Ok(mut events) = self.events.lock() {
            events.retain(|event| match event {
                AccessKitEvent::InitialTreeRequested => {
                    self.active = true;
                    self.initialize_pending = true;
                    false
                }
                AccessKitEvent::AccessibilityDeactivated => {
                    self.active = false;
                    self.initialize_pending = false;
                    self.snapshot = None;
                    self.last_published = None;
                    false
                }
                AccessKitEvent::ActionRequested(_) => true,
            });
        }
        self.active
    }

    /// Drain supported platform actions for routing back to the frame source.
    pub fn take_actions(&mut self) -> Vec<SemanticAction> {
        self.events
            .lock()
            .map(|mut events| {
                let mut actions = Vec::new();
                events.retain(|event| {
                    let AccessKitEvent::ActionRequested(request) = event else {
                        return true;
                    };
                    let target = request.target_node.0;
                    let action = match request.action {
                        Action::SetValue => match &request.data {
                            Some(ActionData::NumericValue(value)) if value.is_finite() => {
                                Some(SemanticAction::SetNumericValue {
                                    target,
                                    value: *value,
                                })
                            }
                            _ => None,
                        },
                        Action::Increment => Some(SemanticAction::Increment { target }),
                        Action::Decrement => Some(SemanticAction::Decrement { target }),
                        Action::Click => Some(SemanticAction::Click { target }),
                        Action::Focus => Some(SemanticAction::Focus { target }),
                        Action::Blur => Some(SemanticAction::Blur { target }),
                        Action::ScrollIntoView => Some(SemanticAction::ScrollIntoView { target }),
                        _ => None,
                    };
                    actions.extend(action);
                    false
                });
                actions
            })
            .unwrap_or_default()
    }

    /// Publish changes to the last active snapshot, including focus-only updates.
    pub fn publish_root(&mut self, window: &dyn Window) {
        self.update_window_bounds(window);
        if !self.active {
            return;
        }
        let requested = std::mem::take(&mut self.initialize_pending);
        let title = &self.title;
        let snapshot = self.snapshot.clone();
        let size = window.surface_size().cast::<f64>();
        let scale = window.scale_factor();
        let offset = self.content_offset;
        let key = publication_geometry(size.width as u32, size.height as u32, scale, offset);
        let published = &mut self.last_published;
        if !requested
            && let Some((last_key, previous)) = published.as_ref()
            && *last_key == key
            && match (previous, &snapshot) {
                (Some(old), Some(new)) => Arc::ptr_eq(old, new),
                (None, None) => true,
                _ => false,
            }
        {
            return;
        }
        self.adapter.update_if_active(move || {
            let incremental = published.as_ref().and_then(|(last_key, previous)| {
                if requested || *last_key != key {
                    return None;
                }
                update_stable_tree(previous.as_deref()?, snapshot.as_deref()?, scale, offset)
            });
            let update = incremental.unwrap_or_else(|| {
                root_update_with_offset(
                    title.clone(),
                    size.width,
                    size.height,
                    scale,
                    offset,
                    requested,
                    snapshot.as_deref(),
                )
            });
            *published = Some((key, snapshot));
            update
        });
    }

    fn update_window_bounds(&mut self, window: &dyn Window) {
        let outer_origin: (_, _) = window
            .outer_position()
            .unwrap_or_default()
            .cast::<f64>()
            .into();
        let outer_size: (_, _) = window.outer_size().cast::<f64>().into();
        let inner_origin: (_, _) = window.surface_position().cast::<f64>().into();
        let inner_size: (_, _) = window.surface_size().cast::<f64>().into();
        self.adapter.set_window_bounds(
            Rect::from_origin_size(outer_origin, outer_size),
            Rect::from_origin_size(inner_origin, inner_size),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_tree_identifies_the_plico_window() {
        let update = root_update("Gallery".into(), 800.0, 600.0, 1.0, true, None);
        assert_eq!(update.focus, ROOT_ID);
        assert_eq!(update.tree.as_ref().unwrap().root, ROOT_ID);
        assert_eq!(
            update.tree.as_ref().unwrap().toolkit_name.as_deref(),
            Some("Plico")
        );
        assert_eq!(update.nodes[0].1.role(), Role::Window);
        assert_eq!(update.nodes[0].1.label(), Some("Gallery"));
        assert_eq!(
            update.nodes[0].1.bounds(),
            Some(Rect::new(0.0, 0.0, 800.0, 600.0))
        );
    }

    #[test]
    fn live_region_roles_reach_accesskit() {
        let snapshot = SemanticSnapshot {
            revision: 1,
            nodes: vec![
                crate::SemanticNode {
                    id: 2,
                    role: SemanticRole::Status,
                    label: Some("Saved".into()),
                    value: Some("complete".into()),
                    numeric_value: None,
                    min_numeric_value: None,
                    max_numeric_value: None,
                    bounds: [0.0, 0.0, 100.0, 20.0],
                    children: vec![],
                    controls: vec![],
                    described_by: Vec::new(),
                    active_descendant: None,
                    disabled: false,
                    states: crate::SemanticStates::default(),
                },
                crate::SemanticNode {
                    id: 3,
                    role: SemanticRole::Alert,
                    label: Some("Connection lost".into()),
                    value: None,
                    numeric_value: None,
                    min_numeric_value: None,
                    max_numeric_value: None,
                    bounds: [0.0, 20.0, 100.0, 40.0],
                    children: vec![],
                    controls: vec![],
                    described_by: Vec::new(),
                    active_descendant: None,
                    disabled: false,
                    states: crate::SemanticStates::default(),
                },
            ],
            root_children: vec![2, 3],
            ..SemanticSnapshot::default()
        };
        let update = root_update("Gallery".into(), 200.0, 100.0, 1.0, true, Some(&snapshot));
        assert_eq!(update.nodes[1].1.role(), Role::Status);
        assert_eq!(update.nodes[1].1.label(), Some("Saved"));
        assert_eq!(update.nodes[1].1.value(), Some("complete"));
        assert_eq!(update.nodes[2].1.role(), Role::Alert);
        assert_eq!(update.nodes[2].1.label(), Some("Connection lost"));
    }

    #[test]
    fn static_label_text_reaches_the_accesskit_value() {
        let mut label = SemanticNode {
            id: 2,
            role: SemanticRole::Label,
            label: Some("最后一行".into()),
            value: None,
            numeric_value: None,
            min_numeric_value: None,
            max_numeric_value: None,
            bounds: [0.; 4],
            children: vec![],
            controls: vec![],
            described_by: vec![],
            active_descendant: None,
            disabled: false,
            states: Default::default(),
        };
        assert_eq!(accesskit_node(&label, 1., None).value(), Some("最后一行"));
        label.value = Some("explicit value".into());
        assert_eq!(
            accesskit_node(&label, 1., None).value(),
            Some("explicit value")
        );
        label.role = SemanticRole::Button;
        label.value = None;
        assert_eq!(accesskit_node(&label, 1., None).value(), None);
    }

    #[test]
    fn linux_csd_offsets_semantic_bounds_but_not_the_window_root() {
        let snapshot = SemanticSnapshot {
            revision: 1,
            nodes: vec![crate::SemanticNode {
                id: 2,
                role: SemanticRole::Button,
                label: Some("Save".into()),
                value: None,
                numeric_value: None,
                min_numeric_value: None,
                max_numeric_value: None,
                bounds: [2., 3., 42., 23.],
                children: Vec::new(),
                controls: Vec::new(),
                described_by: Vec::new(),
                active_descendant: None,
                disabled: false,
                states: crate::SemanticStates::default(),
            }],
            root_children: vec![2],
            ..SemanticSnapshot::default()
        };
        let update = root_update_with_offset(
            "Gallery".into(),
            200.,
            100.,
            2.,
            [14., 14.],
            true,
            Some(&snapshot),
        );
        assert_eq!(
            update.nodes[0].1.bounds(),
            Some(Rect::new(0., 0., 200., 100.))
        );
        assert_eq!(
            update.nodes[1].1.bounds(),
            Some(Rect::new(32., 34., 112., 74.))
        );
    }

    #[test]
    fn interactive_grid_cells_expose_actions_but_static_cells_do_not() {
        let mut semantic = crate::SemanticNode {
            id: 2,
            role: SemanticRole::GridCell,
            label: Some("B1 6".into()),
            value: None,
            numeric_value: None,
            min_numeric_value: None,
            max_numeric_value: None,
            bounds: [0.0, 0.0, 120.0, 44.0],
            children: vec![],
            controls: vec![],
            described_by: Vec::new(),
            active_descendant: None,
            disabled: false,
            states: crate::SemanticStates::default(),
        };
        for (direction, expected) in [
            (
                crate::SemanticSortDirection::Ascending,
                accesskit::SortDirection::Ascending,
            ),
            (
                crate::SemanticSortDirection::Descending,
                accesskit::SortDirection::Descending,
            ),
            (
                crate::SemanticSortDirection::Other,
                accesskit::SortDirection::Other,
            ),
        ] {
            let mut header = semantic.clone();
            header.role = SemanticRole::ColumnHeader;
            header.states.sort_direction = Some(direction);
            assert_eq!(
                accesskit_node(&header, 1.0, None).sort_direction(),
                Some(expected)
            );
            header.states.sort_direction = None;
            assert_eq!(accesskit_node(&header, 1.0, None).sort_direction(), None);
        }
        let node = accesskit_node(&semantic, 1.0, None);
        assert!(node.supports_action(Action::Click));
        assert!(node.supports_action(Action::Focus));
        semantic.disabled = true;
        let disabled = accesskit_node(&semantic, 1.0, None);
        assert!(!disabled.supports_action(Action::Click));
        assert!(!disabled.supports_action(Action::Focus));
        semantic.disabled = false;
        semantic.role = SemanticRole::Cell;
        let static_cell = accesskit_node(&semantic, 1.0, None);
        assert!(!static_cell.supports_action(Action::Click));
        assert!(!static_cell.supports_action(Action::Focus));
    }

    #[test]
    fn interaction_states_reach_accesskit() {
        let semantic = crate::SemanticNode {
            id: 2,
            role: SemanticRole::CheckBox,
            label: Some("Partial selection".into()),
            value: None,
            numeric_value: None,
            min_numeric_value: None,
            max_numeric_value: None,
            bounds: [0.0, 0.0, 100.0, 20.0],
            children: vec![],
            controls: vec![],
            described_by: Vec::new(),
            active_descendant: None,
            disabled: false,
            states: crate::SemanticStates {
                checked: Some(SemanticToggleState::Mixed),
                selected: Some(true),
                expanded: Some(false),
                current: Some(SemanticCurrent::Date),
                popup: Some(SemanticPopup::ListBox),
                modal: Some(true),
                ..crate::SemanticStates::default()
            },
        };

        let node = accesskit_node(&semantic, 1.0, None);
        assert_eq!(node.toggled(), Some(Toggled::Mixed));
        assert_eq!(node.is_selected(), Some(true));
        assert_eq!(node.is_expanded(), Some(false));
        assert_eq!(node.aria_current(), Some(AriaCurrent::Date));
        assert_eq!(node.has_popup(), Some(HasPopup::Listbox));
        assert!(node.is_modal());
    }

    #[test]
    fn numeric_range_reaches_accesskit() {
        let semantic = crate::SemanticNode {
            id: 2,
            role: SemanticRole::ProgressBar,
            label: Some("Build progress".into()),
            value: Some("64 percent".into()),
            numeric_value: Some(64.0),
            min_numeric_value: Some(0.0),
            max_numeric_value: Some(100.0),
            bounds: [0.0, 0.0, 100.0, 8.0],
            children: vec![],
            controls: vec![],
            described_by: Vec::new(),
            active_descendant: None,
            disabled: false,
            states: crate::SemanticStates::default(),
        };

        let node = accesskit_node(&semantic, 1.0, None);
        assert_eq!(node.role(), Role::ProgressIndicator);
        assert_eq!(node.value(), Some("64 percent"));
        assert_eq!(node.numeric_value(), Some(64.0));
        assert_eq!(node.min_numeric_value(), Some(0.0));
        assert_eq!(node.max_numeric_value(), Some(100.0));
        let mut slider = semantic.clone();
        slider.role = SemanticRole::Slider;
        let node = accesskit_node(&slider, 1., None);
        assert!(node.supports_action(Action::SetValue));
        assert!(node.supports_action(Action::Increment));
        assert!(node.supports_action(Action::Decrement));
        assert!(!node.supports_action(Action::Click));
        slider.disabled = true;
        let node = accesskit_node(&slider, 1., None);
        assert!(!node.supports_action(Action::SetValue));
        assert!(!node.supports_action(Action::Increment));
    }

    #[test]
    fn composite_widget_relations_reach_accesskit_and_filter_hidden_targets() {
        let semantic = crate::SemanticNode {
            id: 2,
            role: SemanticRole::ComboBox,
            label: Some("Workspace".into()),
            value: None,
            numeric_value: None,
            min_numeric_value: None,
            max_numeric_value: None,
            bounds: [0.0, 0.0, 100.0, 20.0],
            children: vec![],
            controls: vec![3, 9],
            described_by: vec![3, 9],
            active_descendant: Some(4),
            disabled: false,
            states: crate::SemanticStates::default(),
        };

        let node = accesskit_node(&semantic, 1.0, None);
        assert_eq!(node.controls(), &[NodeId(3), NodeId(9)]);
        assert_eq!(node.described_by(), &[NodeId(3), NodeId(9)]);
        assert_eq!(node.active_descendant(), Some(NodeId(4)));

        let exposed = [2, 3].into_iter().collect();
        let filtered = accesskit_node(&semantic, 1.0, Some(&exposed));
        assert_eq!(filtered.controls(), &[NodeId(3)]);
        assert_eq!(filtered.described_by(), &[NodeId(3)]);
        assert_eq!(filtered.active_descendant(), None);
    }

    #[test]
    fn tooltip_description_tree_updates_keep_names_and_remove_closed_links() {
        let semantic = |id, role, label: &str| crate::SemanticNode {
            id,
            role,
            label: Some(label.into()),
            value: None,
            numeric_value: None,
            min_numeric_value: None,
            max_numeric_value: None,
            bounds: [0.0, 0.0, 100.0, 20.0],
            children: Vec::new(),
            controls: Vec::new(),
            described_by: Vec::new(),
            active_descendant: None,
            disabled: false,
            states: Default::default(),
        };
        let mut snapshot = SemanticSnapshot {
            revision: 1,
            nodes: vec![
                semantic(2, SemanticRole::Button, "Invite"),
                semantic(3, SemanticRole::Tooltip, "Invite a teammate"),
            ],
            root_children: vec![2, 3],
            focus: Some(2),
            modal_root: None,
        };
        snapshot.nodes[0].described_by = vec![3];
        let opened = root_update("Gallery".into(), 600.0, 400.0, 2.0, true, Some(&snapshot));
        let control = &opened
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(2))
            .unwrap()
            .1;
        assert_eq!(control.label(), Some("Invite"));
        assert_eq!(control.described_by(), &[NodeId(3)]);
        assert_eq!(opened.focus, NodeId(2));
        let hint = &opened
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(3))
            .unwrap()
            .1;
        assert_eq!(hint.role(), Role::Tooltip);
        assert_eq!(hint.label(), Some("Invite a teammate"));
        assert!(!hint.supports_action(Action::Focus));
        snapshot.revision += 1;
        snapshot.nodes[1].label = Some("邀请团队成员 🦀".into());
        let updated = root_update("Gallery".into(), 600.0, 400.0, 2.0, false, Some(&snapshot));
        assert_eq!(
            updated
                .nodes
                .iter()
                .find(|(id, _)| *id == NodeId(3))
                .unwrap()
                .1
                .label(),
            Some("邀请团队成员 🦀")
        );
        // Even an author retaining a stale link cannot publish a missing target.
        snapshot.nodes.pop();
        snapshot.root_children.pop();
        snapshot.revision += 1;
        let closed = root_update("Gallery".into(), 600.0, 400.0, 2.0, false, Some(&snapshot));
        let control = &closed
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(2))
            .unwrap()
            .1;
        assert_eq!(control.label(), Some("Invite"));
        assert!(control.described_by().is_empty());
        assert!(!closed.nodes.iter().any(|(id, _)| *id == NodeId(3)));
        assert_eq!(closed.focus, NodeId(2));
    }

    struct IgnoreChanges;
    impl accesskit_consumer::TreeChangeHandler for IgnoreChanges {
        fn node_added(&mut self, _: &accesskit_consumer::Node<'_>) {}
        fn node_updated(
            &mut self,
            _: &accesskit_consumer::Node<'_>,
            _: &accesskit_consumer::Node<'_>,
        ) {
        }
        fn focus_moved(
            &mut self,
            _: Option<&accesskit_consumer::Node<'_>>,
            _: Option<&accesskit_consumer::Node<'_>>,
        ) {
        }
        fn node_removed(&mut self, _: &accesskit_consumer::Node<'_>) {}
    }

    #[test]
    fn focus_and_caption_updates_only_send_changed_nodes() {
        let nodes = (1..=1024)
            .map(|id| SemanticNode {
                id,
                role: SemanticRole::Button,
                label: Some(format!("Row {id}")),
                value: None,
                numeric_value: None,
                min_numeric_value: None,
                max_numeric_value: None,
                bounds: [0., id as f32, 100., id as f32 + 1.],
                children: vec![],
                controls: vec![],
                described_by: vec![],
                active_descendant: None,
                disabled: false,
                states: Default::default(),
            })
            .collect();
        let before = SemanticSnapshot {
            nodes,
            root_children: (1..=1024).collect(),
            ..Default::default()
        };
        let initial = root_update("Rows".into(), 100., 600., 1., true, Some(&before));
        assert_eq!(initial.nodes.len(), 1025);
        let mut consumer = accesskit_consumer::Tree::new(initial, true);
        let mut focused = before.clone();
        focused.focus = Some(800);
        let update = update_stable_tree(&before, &focused, 1., [0.; 2]).unwrap();
        assert!(update.nodes.is_empty());
        consumer.update_and_process_changes(update, &mut IgnoreChanges);
        assert_eq!(
            consumer.state().focus().unwrap().label().as_deref(),
            Some("Row 800")
        );

        let mut edited = focused.clone();
        edited.nodes[799].label = Some("Changed".into());
        let update = update_stable_tree(&focused, &edited, 1., [0.; 2]).unwrap();
        assert_eq!(update.nodes.len(), 1);
        assert_eq!(update.nodes[0].0, NodeId(800));
        consumer.update_and_process_changes(update, &mut IgnoreChanges);
        assert_eq!(
            consumer.state().focus().unwrap().label().as_deref(),
            Some("Changed")
        );
        edited.modal_root = Some(1);
        assert!(update_stable_tree(&focused, &edited, 1., [0.; 2]).is_none());
    }

    #[test]
    fn publication_filters_dangling_structural_ids_before_accesskit() {
        let semantic = |id, children| crate::SemanticNode {
            id,
            role: SemanticRole::Group,
            label: Some(format!("node {id}")),
            value: None,
            numeric_value: None,
            min_numeric_value: None,
            max_numeric_value: None,
            bounds: [0.0, 0.0, 100.0, 20.0],
            children,
            controls: Vec::new(),
            described_by: Vec::new(),
            active_descendant: None,
            disabled: false,
            states: Default::default(),
        };
        let stale = 4_294_967_682;
        let initial = SemanticSnapshot {
            revision: 1,
            nodes: vec![semantic(2, vec![3]), semantic(3, Vec::new())],
            root_children: vec![2],
            focus: Some(3),
            modal_root: None,
        };
        let mut consumer = accesskit_consumer::Tree::new(
            root_update("Plico".into(), 600.0, 400.0, 1.0, true, Some(&initial)),
            true,
        );

        let snapshot = SemanticSnapshot {
            revision: 2,
            nodes: vec![semantic(2, vec![stale])],
            root_children: vec![2, stale],
            focus: Some(stale),
            modal_root: None,
        };

        let update = root_update("Plico".into(), 600.0, 400.0, 1.0, false, Some(&snapshot));
        assert_eq!(update.focus, ROOT_ID);
        assert_eq!(update.nodes[0].1.children(), &[NodeId(2)]);
        assert!(update.nodes[1].1.children().is_empty());
        assert!(update.nodes.iter().all(|(id, _)| *id != NodeId(stale)));

        consumer.update_and_process_changes(update, &mut IgnoreChanges);
    }

    #[test]
    fn structural_roles_reach_accesskit_without_becoming_generic_containers() {
        let roles = [
            (SemanticRole::Group, Role::Group),
            (SemanticRole::Dialog, Role::Dialog),
            (SemanticRole::AlertDialog, Role::AlertDialog),
            (SemanticRole::Image, Role::Image),
            (SemanticRole::RadioGroup, Role::RadioGroup),
            (SemanticRole::Region, Role::Region),
            (SemanticRole::List, Role::List),
            (SemanticRole::ListItem, Role::ListItem),
            (SemanticRole::Menu, Role::Menu),
            (SemanticRole::MenuBar, Role::MenuBar),
            (SemanticRole::MenuItem, Role::MenuItem),
            (SemanticRole::MenuItemCheckBox, Role::MenuItemCheckBox),
            (SemanticRole::MenuItemRadio, Role::MenuItemRadio),
            (SemanticRole::Tree, Role::Tree),
            (SemanticRole::TreeItem, Role::TreeItem),
            (SemanticRole::Toolbar, Role::Toolbar),
            (SemanticRole::Table, Role::Table),
            (SemanticRole::Row, Role::Row),
            (SemanticRole::Cell, Role::Cell),
            (SemanticRole::ColumnHeader, Role::ColumnHeader),
            (SemanticRole::RowHeader, Role::RowHeader),
            (SemanticRole::Separator, Role::Splitter),
            (SemanticRole::Slider, Role::Slider),
            (SemanticRole::SpinButton, Role::SpinButton),
            (SemanticRole::ProgressBar, Role::ProgressIndicator),
            (SemanticRole::Heading, Role::Heading),
            (SemanticRole::TabList, Role::TabList),
            (SemanticRole::Tab, Role::Tab),
            (SemanticRole::TabPanel, Role::TabPanel),
            (SemanticRole::Grid, Role::Grid),
            (SemanticRole::GridCell, Role::GridCell),
        ];
        for (index, (semantic, _)) in roles.iter().enumerate() {
            let node = crate::SemanticNode {
                id: index as u64 + 2,
                role: *semantic,
                label: Some(format!("table node {index}")),
                value: None,
                numeric_value: None,
                min_numeric_value: None,
                max_numeric_value: None,
                bounds: [0.0, index as f32 * 20.0, 100.0, index as f32 * 20.0 + 20.0],
                children: vec![],
                controls: vec![],
                described_by: Vec::new(),
                active_descendant: None,
                disabled: false,
                states: crate::SemanticStates::default(),
            };
            assert_eq!(accesskit_node(&node, 1.0, None).role(), roles[index].1);
        }
    }

    #[test]
    fn modal_snapshot_exposes_only_the_modal_at_the_window_root() {
        let snapshot = SemanticSnapshot {
            revision: 1,
            nodes: vec![
                crate::SemanticNode {
                    id: 2,
                    role: SemanticRole::Button,
                    label: Some("Background".into()),
                    value: None,
                    numeric_value: None,
                    min_numeric_value: None,
                    max_numeric_value: None,
                    bounds: [0.0, 0.0, 50.0, 20.0],
                    children: vec![],
                    controls: vec![],
                    described_by: Vec::new(),
                    active_descendant: None,
                    disabled: false,
                    states: crate::SemanticStates::default(),
                },
                crate::SemanticNode {
                    id: 3,
                    role: SemanticRole::Dialog,
                    label: Some("Settings".into()),
                    value: None,
                    numeric_value: None,
                    min_numeric_value: None,
                    max_numeric_value: None,
                    bounds: [10.0, 10.0, 90.0, 90.0],
                    children: vec![4],
                    controls: vec![],
                    described_by: Vec::new(),
                    active_descendant: None,
                    disabled: false,
                    states: crate::SemanticStates::default(),
                },
                crate::SemanticNode {
                    id: 4,
                    role: SemanticRole::Button,
                    label: Some("Save".into()),
                    value: None,
                    numeric_value: None,
                    min_numeric_value: None,
                    max_numeric_value: None,
                    bounds: [60.0, 60.0, 80.0, 75.0],
                    children: vec![],
                    controls: vec![],
                    described_by: Vec::new(),
                    active_descendant: None,
                    disabled: false,
                    states: crate::SemanticStates::default(),
                },
            ],
            root_children: vec![2, 3],
            focus: Some(3),
            modal_root: Some(3),
        };
        let update = root_update("Gallery".into(), 200.0, 200.0, 2.0, true, Some(&snapshot));
        assert_eq!(update.focus, NodeId(3));
        assert_eq!(update.nodes[0].1.children(), &[NodeId(3)]);
        assert_eq!(update.nodes.len(), 3, "background subtree must be absent");
        assert_eq!(update.nodes[1].1.role(), Role::Dialog);
        assert_eq!(update.nodes[1].1.label(), Some("Settings"));
        assert_eq!(
            update.nodes[1].1.bounds(),
            Some(Rect::new(20.0, 20.0, 180.0, 180.0))
        );
        assert!(update.nodes[2].1.supports_action(Action::Click));
        assert!(update.nodes[2].1.supports_action(Action::Focus));
        assert!(update.nodes[2].1.supports_action(Action::ScrollIntoView));
    }

    #[test]
    fn publication_tracks_window_geometry() {
        let base = publication_geometry(800, 600, 2., [0., 0.]);
        assert_ne!(base, publication_geometry(801, 600, 2., [0., 0.]));
        assert_ne!(base, publication_geometry(800, 600, 1., [0., 0.]));
        assert_ne!(base, publication_geometry(800, 600, 2., [0., 32.]));
    }
}
