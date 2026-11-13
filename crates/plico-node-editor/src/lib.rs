//! A reusable node-graph model and immediate Plico editor surface.
//!
//! The graph is ordinary serializable Rust data. Transient pointer anchors
//! stay in Plico component state, so snapshots never restore a half-finished
//! drag. Edges are emitted as one SVG layer; nodes remain ordinary controls.

use plico::prelude::*;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::collections::BTreeSet;

/// Stable node identity, independent of declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// Stable port identity within its graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PortId(pub u64);

/// Stable edge identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub u64);

/// One typed connection point on a node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    pub id: PortId,
    pub label: String,
    /// Application-defined compatibility class. Empty accepts any peer.
    #[serde(default)]
    pub kind: String,
}

/// One graph node. Positions and sizes are in graph logical pixels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub title: String,
    pub position: [f32; 2],
    pub size: [f32; 2],
    #[serde(default)]
    pub inputs: Vec<Port>,
    #[serde(default)]
    pub outputs: Vec<Port>,
}

/// One end of an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub node: NodeId,
    pub port: PortId,
}

/// Directed output-to-input connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub from: Endpoint,
    pub to: Endpoint,
}

/// Serializable node graph with internal stable-ID allocation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    next_id: u64,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            next_id: 1,
        }
    }
}

#[derive(Clone, Debug, Snafu, PartialEq, Eq)]
pub enum GraphError {
    #[snafu(display("node {id:?} does not exist"))]
    MissingNode { id: NodeId },
    #[snafu(display("port {port:?} is not an output of node {node:?}"))]
    MissingOutput { node: NodeId, port: PortId },
    #[snafu(display("port {port:?} is not an input of node {node:?}"))]
    MissingInput { node: NodeId, port: PortId },
    #[snafu(display("cannot connect incompatible port kinds {from:?} and {to:?}"))]
    Incompatible { from: String, to: String },
    #[snafu(display("the connection already exists"))]
    DuplicateEdge,
}

impl Graph {
    fn id(&mut self) -> u64 {
        // Imported/older snapshots may omit or carry a stale allocator. Never
        // let that turn a valid graph into duplicate application identities.
        let used = self
            .nodes
            .iter()
            .flat_map(|node| {
                std::iter::once(node.id.0)
                    .chain(node.inputs.iter().map(|port| port.id.0))
                    .chain(node.outputs.iter().map(|port| port.id.0))
            })
            .chain(self.edges.iter().map(|edge| edge.id.0))
            .max()
            .unwrap_or(0);
        let id = self.next_id.max(used.saturating_add(1)).max(1);
        self.next_id = id.saturating_add(1);
        id
    }

    /// Add a node and return its stable identity.
    pub fn add_node(
        &mut self,
        title: impl Into<String>,
        position: [f32; 2],
        size: [f32; 2],
    ) -> NodeId {
        let id = NodeId(self.id());
        self.nodes.push(Node {
            id,
            title: title.into(),
            position,
            size: [size[0].max(120.), size[1].max(72.)],
            inputs: Vec::new(),
            outputs: Vec::new(),
        });
        id
    }

    /// Add an input to an existing node.
    pub fn add_input(
        &mut self,
        node: NodeId,
        label: impl Into<String>,
        kind: impl Into<String>,
    ) -> Result<PortId, GraphError> {
        let id = PortId(self.id());
        let target = self
            .nodes
            .iter_mut()
            .find(|candidate| candidate.id == node)
            .ok_or(GraphError::MissingNode { id: node })?;
        target.inputs.push(Port {
            id,
            label: label.into(),
            kind: kind.into(),
        });
        Ok(id)
    }

    /// Add an output to an existing node.
    pub fn add_output(
        &mut self,
        node: NodeId,
        label: impl Into<String>,
        kind: impl Into<String>,
    ) -> Result<PortId, GraphError> {
        let id = PortId(self.id());
        let target = self
            .nodes
            .iter_mut()
            .find(|candidate| candidate.id == node)
            .ok_or(GraphError::MissingNode { id: node })?;
        target.outputs.push(Port {
            id,
            label: label.into(),
            kind: kind.into(),
        });
        Ok(id)
    }

    /// Connect an output to an input after validating identity and kind.
    pub fn connect(&mut self, from: Endpoint, to: Endpoint) -> Result<EdgeId, GraphError> {
        let from_node = self
            .nodes
            .iter()
            .find(|node| node.id == from.node)
            .ok_or(GraphError::MissingNode { id: from.node })?;
        let output = from_node
            .outputs
            .iter()
            .find(|port| port.id == from.port)
            .ok_or(GraphError::MissingOutput {
                node: from.node,
                port: from.port,
            })?;
        let to_node = self
            .nodes
            .iter()
            .find(|node| node.id == to.node)
            .ok_or(GraphError::MissingNode { id: to.node })?;
        let input = to_node
            .inputs
            .iter()
            .find(|port| port.id == to.port)
            .ok_or(GraphError::MissingInput {
                node: to.node,
                port: to.port,
            })?;
        if !output.kind.is_empty() && !input.kind.is_empty() && output.kind != input.kind {
            return Err(GraphError::Incompatible {
                from: output.kind.clone(),
                to: input.kind.clone(),
            });
        }
        if self
            .edges
            .iter()
            .any(|edge| edge.from == from && edge.to == to)
        {
            return Err(GraphError::DuplicateEdge);
        }
        let id = EdgeId(self.id());
        self.edges.push(Edge { id, from, to });
        Ok(id)
    }

    /// Remove a node and every edge attached to it.
    pub fn remove_node(&mut self, id: NodeId) -> bool {
        let before = self.nodes.len();
        self.nodes.retain(|node| node.id != id);
        self.edges
            .retain(|edge| edge.from.node != id && edge.to.node != id);
        self.nodes.len() != before
    }

    /// Remove one connection without changing either endpoint.
    pub fn remove_edge(&mut self, id: EdgeId) -> bool {
        let before = self.edges.len();
        self.edges.retain(|edge| edge.id != id);
        self.edges.len() != before
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }
}

/// Serializable view and selection state. Pointer anchors are intentionally
/// absent and remain transient component state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorState {
    pub pan: [f32; 2],
    pub zoom: f32,
    pub selected: BTreeSet<NodeId>,
    pub connecting: Option<Endpoint>,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            pan: [28., 28.],
            zoom: 1.,
            selected: BTreeSet::new(),
            connecting: None,
        }
    }
}

impl EditorState {
    pub fn set_zoom(&mut self, zoom: f32) {
        if zoom.is_finite() {
            self.zoom = zoom.clamp(0.4, 2.5);
        }
    }

    pub fn reset_view(&mut self) {
        self.pan = [28., 28.];
        self.zoom = 1.;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorResponse {
    pub graph_changed: bool,
    pub selection_changed: bool,
    pub connection_created: Option<EdgeId>,
    pub connection_error: Option<GraphError>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorOptions {
    pub height: f32,
    pub surface_size: [f32; 2],
}

impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            height: 560.,
            surface_size: [2400., 1600.],
        }
    }
}

#[derive(Clone, Default, PartialEq)]
struct DragAnchor(Option<(f64, f64)>);

fn screen(point: [f32; 2], state: &EditorState) -> [f32; 2] {
    [
        point[0] * state.zoom + state.pan[0],
        point[1] * state.zoom + state.pan[1],
    ]
}

fn port_point(graph: &Graph, endpoint: Endpoint, output: bool) -> Option<[f32; 2]> {
    let node = graph.node(endpoint.node)?;
    let ports = if output { &node.outputs } else { &node.inputs };
    let index = ports.iter().position(|port| port.id == endpoint.port)?;
    let x = if output {
        node.position[0] + node.size[0]
    } else {
        node.position[0]
    };
    Some([x, node.position[1] + 58. + index as f32 * 28.])
}

fn compatible_input(graph: &Graph, from: Endpoint, to: Endpoint) -> bool {
    let Some(source) = graph
        .node(from.node)
        .and_then(|node| node.outputs.iter().find(|port| port.id == from.port))
    else {
        return false;
    };
    let Some(input) = graph
        .node(to.node)
        .and_then(|node| node.inputs.iter().find(|port| port.id == to.port))
    else {
        return false;
    };
    source.kind.is_empty() || input.kind.is_empty() || source.kind == input.kind
}

fn edges_svg(graph: &Graph, state: &EditorState, size: [f32; 2]) -> String {
    let mut body = String::new();
    for edge in &graph.edges {
        let (Some(from), Some(to)) = (
            port_point(graph, edge.from, true),
            port_point(graph, edge.to, false),
        ) else {
            continue;
        };
        let from = screen(from, state);
        let to = screen(to, state);
        let dx = to[0] - from[0];
        let reach = if dx >= 0. {
            // Nearby forward ports should not overshoot and fold back on
            // themselves. Backward edges keep a larger shoulder so their
            // direction remains legible around adjacent nodes.
            (dx * 0.5).max(8. * state.zoom)
        } else {
            (dx.abs() * 0.5).max(48. * state.zoom)
        };
        body.push_str(&format!(
            r#"<path d="M {:.2} {:.2} C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}"/>"#,
            from[0],
            from[1],
            from[0] + reach,
            from[1],
            to[0] - reach,
            to[1],
            to[0],
            to[1]
        ));
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{:.0}" height="{:.0}" viewBox="0 0 {:.0} {:.0}"><g fill="none" stroke="#64748b" stroke-opacity="0.78" stroke-width="2.25" stroke-linecap="round">{body}</g></svg>"##,
        size[0], size[1], size[0], size[1]
    )
}

/// Show a reusable graph editor.
///
/// Click an output and then a compatible input to connect them. Drag node
/// headers to move nodes, drag empty canvas space to pan, and use the compact
/// toolbar for zoom/reset/delete operations.
#[component]
#[rustfmt::skip]
pub fn node_editor(
    cx: &mut UiContext,
    graph: &mut Graph,
    state: &mut EditorState,
    options: EditorOptions,
) -> EditorResponse {
    let mut response = EditorResponse::default();
    let snapshot = graph.nodes.clone();
    let mut movement = Vec::<(NodeId, f32, f32)>::new();
    let mut selected = None;
    let mut port_click = None::<(Endpoint, bool)>;
    let mut pan_delta = [0f32; 2];

    column(cx).position(Position::Relative).width_percent(100.).height(options.height).clip(true).radius(20.)
        .background(Color::Surface).border(1., Color::Border) + |cx|
    {
        button_base(cx, "Pan node canvas").absolute(0., 0.).width_percent(100.).height_percent(100.)
            .background(Color::Surface).children(|cx| {
            let drag = cx.state(DragAnchor::default);
            if cx.pressed() {
                if let Some((x, y)) = cx.pointer_position() {
                    let previous = drag.read().0;
                    drag.update(|anchor| anchor.0 = Some((x, y)));
                    if let Some((px, py)) = previous {
                        pan_delta = [(x - px) as f32, (y - py) as f32];
                    }
                }
            } else {
                drag.update(|anchor| anchor.0 = None);
            }
        });

        let svg = edges_svg(graph, state, options.surface_size);
        column(cx).absolute(0., 0.).width(options.surface_size[0]).height(options.surface_size[1]) + |cx| {
            svg_source(cx, svg).size(options.surface_size[0], options.surface_size[1]);
        };

        for node in &snapshot {
            cx.keyed(node.id.0, |cx| {
                let at = screen(node.position, state);
                let width = node.size[0] * state.zoom;
                let rows = node.inputs.len().max(node.outputs.len());
                let height = node.size[1].max(66. + rows as f32 * 28.) * state.zoom;
                let is_selected = state.selected.contains(&node.id);
                column(cx).absolute(at[0], at[1]).width(width).height(height).radius(14.).background(Color::Panel)
                    .border(
                    if is_selected { 2. } else { 1. },
                    if is_selected { Color::Accent } else { Color::Border },
                ).elevation(if is_selected { Elevation::CONTROL } else { Elevation::SUBTLE }) + |cx|
                {
                    let header = button_base(cx, format!("Move {}", node.title)).width_percent(100.)
                        .height(42. * state.zoom).padding_x(14)
                        .background(if is_selected { Color::Selection } else { Color::Control }).children(|cx| {
                        text(cx, &node.title).weight(650)
                            .font_size((14. * state.zoom).clamp(11., 18.) as u16).ellipsis(true).width(0.)
                            .grow(1.);
                        let drag = cx.state(DragAnchor::default);
                        if cx.pressed() {
                            if let Some((x, y)) = cx.pointer_position() {
                                let previous = drag.read().0;
                                drag.update(|anchor| anchor.0 = Some((x, y)));
                                if let Some((px, py)) = previous {
                                    movement.push((
                                        node.id,
                                        (x - px) as f32 / state.zoom,
                                        (y - py) as f32 / state.zoom,
                                    ));
                                }
                            }
                        } else {
                            drag.update(|anchor| anchor.0 = None);
                        }
                    });
                    if header.clicked() {
                        selected = Some(node.id);
                    }
                    for index in 0..rows {
                        let y = 48. * state.zoom + index as f32 * 28. * state.zoom;
                        if let Some(port) = node.inputs.get(index) {
                            cx.keyed(port.id.0, |cx| {
                                if button_base(
                                    cx,
                                    format!("Input {}: {}", node.title, port.label),
                                ).absolute(-6. * state.zoom, y).width(18. * state.zoom).height(18. * state.zoom)
                                    .padding(0).radius(9. * state.zoom).background(
                                    if state.connecting.is_some_and(|from| {
                                        compatible_input(
                                            graph,
                                            from,
                                            Endpoint {
                                                node: node.id,
                                                port: port.id,
                                            },
                                        )
                                    }) {
                                        Color::Success
                                    } else {
                                        Color::Accent
                                    },
                                ).children(|_| {}).clicked()
                                {
                                    port_click = Some((
                                        Endpoint {
                                            node: node.id,
                                            port: port.id,
                                        },
                                        false,
                                    ));
                                }
                                text(cx, &port.label).absolute(18. * state.zoom, y - 1.)
                                    .font_size((12. * state.zoom).clamp(10., 16.) as u16).color(Color::Muted);
                            });
                        }
                        if let Some(port) = node.outputs.get(index) {
                            cx.keyed(port.id.0, |cx| {
                                if button_base(cx, format!("Output {}: {}", node.title, port.label)).absolute(width - 12. * state.zoom, y).width(18. * state.zoom)
                                    .height(18. * state.zoom).padding(0).radius(9. * state.zoom).background(
                                    if state.connecting
                                        == Some(Endpoint {
                                            node: node.id,
                                            port: port.id,
                                        })
                                    {
                                        Color::Success
                                    } else {
                                        Color::Accent
                                    },
                                ).children(|_| {}).clicked()
                                {
                                    port_click = Some((
                                        Endpoint {
                                            node: node.id,
                                            port: port.id,
                                        },
                                        true,
                                    ));
                                }
                                text(cx, &port.label).absolute(
                                    width
                                        - (18. + port.label.len() as f32 * 7.)
                                            * state.zoom,
                                    y - 1.,
                                ).font_size((12. * state.zoom).clamp(10., 16.) as u16).color(Color::Muted);
                            });
                        }
                    }
                };
            });
        }

        row(cx).position(Position::Absolute).inset_top(12.).inset_right(12.).padding(4).gap(4).radius(12.)
            .background(Color::Panel).elevation(Elevation::CONTROL) + |cx|
        {
            if button(cx).text("−").aria_label("Zoom out").ghost().width(32.).padding(0).font_size(18).clicked() {
                state.set_zoom(state.zoom - 0.1);
            }
            text!(cx, "{:.0}%", state.zoom * 100.).font_size(12).width(44.);
            if button(cx).text("+").aria_label("Zoom in").ghost().width(32.).padding(0).font_size(18).clicked() {
                state.set_zoom(state.zoom + 0.1);
            }
            if button(cx).text("Reset").ghost().clicked() {
                state.reset_view();
            }
            if button(cx).text("Delete").ghost().disabled(state.selected.is_empty()).clicked() {
                for id in std::mem::take(&mut state.selected) {
                    response.graph_changed |= graph.remove_node(id);
                }
            }
        };
        if state.connecting.is_some() {
            text(cx, "Choose a compatible input").absolute(14., 14.).padding_x(10).padding_y(6).radius(10.)
                .background(Color::Panel).color(Color::Muted).font_size(12);
        }
    };

    if pan_delta != [0., 0.] {
        state.pan[0] += pan_delta[0];
        state.pan[1] += pan_delta[1];
    }
    for (id, dx, dy) in movement {
        if let Some(node) = graph.node_mut(id) {
            node.position[0] += dx;
            node.position[1] += dy;
            response.graph_changed = true;
        }
    }
    if let Some(id) = selected {
        let changed = state.selected.len() != 1 || !state.selected.contains(&id);
        state.selected.clear();
        state.selected.insert(id);
        response.selection_changed |= changed;
    }
    if let Some((endpoint, output)) = port_click {
        match (state.connecting, output) {
            (_, true) => state.connecting = Some(endpoint),
            (Some(from), false) => {
                match graph.connect(from, endpoint) {
                    Ok(id) => {
                        response.connection_created = Some(id);
                        response.graph_changed = true;
                    }
                    Err(error) => response.connection_error = Some(error),
                }
                state.connecting = None;
            }
            (None, false) => {}
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_validates_ports_kinds_duplicates_and_cascade_removal() {
        let mut graph = Graph::default();
        let source = graph.add_node("Source", [0., 0.], [180., 100.]);
        let sink = graph.add_node("Sink", [300., 0.], [180., 100.]);
        let out = graph.add_output(source, "Image", "rgba").unwrap();
        let input = graph.add_input(sink, "Image", "rgba").unwrap();
        let edge = graph
            .connect(
                Endpoint {
                    node: source,
                    port: out,
                },
                Endpoint {
                    node: sink,
                    port: input,
                },
            )
            .unwrap();
        assert_eq!(graph.edges[0].id, edge);
        assert_eq!(
            graph.connect(
                Endpoint {
                    node: source,
                    port: out
                },
                Endpoint {
                    node: sink,
                    port: input
                },
            ),
            Err(GraphError::DuplicateEdge)
        );
        assert!(graph.remove_node(source));
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn graph_round_trip_keeps_ids_and_allocator_monotonic() {
        let mut graph = Graph::default();
        let first = graph.add_node("One", [10., 20.], [180., 100.]);
        let mut encoded = serde_json::to_value(&graph).unwrap();
        encoded["next_id"] = serde_json::json!(0);
        let mut decoded: Graph = serde_json::from_value(encoded).unwrap();
        let second = decoded.add_node("Two", [20., 30.], [180., 100.]);
        assert!(second.0 > first.0);
    }

    #[test]
    fn edge_svg_batches_cubic_connections() {
        let mut graph = Graph::default();
        let a = graph.add_node("A", [0., 0.], [180., 100.]);
        let b = graph.add_node("B", [300., 100.], [180., 100.]);
        let output = graph.add_output(a, "Out", "number").unwrap();
        let input = graph.add_input(b, "In", "number").unwrap();
        graph
            .connect(
                Endpoint {
                    node: a,
                    port: output,
                },
                Endpoint {
                    node: b,
                    port: input,
                },
            )
            .unwrap();
        let svg = edges_svg(&graph, &EditorState::default(), [800., 600.]);
        assert!(svg.contains("<path d=\"M"));
        assert!(svg.contains(" C "));
    }
}
