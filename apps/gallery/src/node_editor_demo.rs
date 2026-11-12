use plico::prelude::*;
use plico_node_editor::{EditorOptions, EditorState, Endpoint, Graph};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct NodeEditorDemo {
    pub graph: Graph,
    pub editor: EditorState,
    pub status: String,
}

impl Default for NodeEditorDemo {
    fn default() -> Self {
        let mut graph = Graph::default();
        let source = graph.add_node("Image source", [40., 90.], [210., 142.]);
        let blur = graph.add_node("Gaussian blur", [350., 60.], [220., 176.]);
        let output = graph.add_node("Preview", [590., 130.], [200., 150.]);
        let source_image = graph.add_output(source, "Image", "rgba").unwrap();
        let blur_image = graph.add_input(blur, "Image", "rgba").unwrap();
        let _blur_radius = graph.add_input(blur, "Radius", "number").unwrap();
        let blurred = graph.add_output(blur, "Result", "rgba").unwrap();
        let preview_image = graph.add_input(output, "Image", "rgba").unwrap();
        let _preview_mask = graph.add_input(output, "Mask", "rgba").unwrap();
        graph
            .connect(
                Endpoint {
                    node: source,
                    port: source_image,
                },
                Endpoint {
                    node: blur,
                    port: blur_image,
                },
            )
            .unwrap();
        graph
            .connect(
                Endpoint {
                    node: blur,
                    port: blurred,
                },
                Endpoint {
                    node: output,
                    port: preview_image,
                },
            )
            .unwrap();
        Self {
            graph,
            editor: EditorState::default(),
            status: "Click an output, then an input, to connect ports.".into(),
        }
    }
}

#[component]
#[rustfmt::skip]
pub fn page(cx: &mut UiContext, demo: &mut NodeEditorDemo) {
    crate::heading(cx, "Node editor",
        "Reusable Rust graph state with ordinary Plico nodes and one batched Bézier edge layer.");
    column(cx).height(0.).grow(1.).padding_right(24).gap(10) + |cx| {
        let response = plico_node_editor::node_editor(cx, &mut demo.graph, &mut demo.editor,
            EditorOptions { height: 590., ..Default::default() });
        if let Some(edge) = response.connection_created {
            demo.status = format!("Created connection {}.", edge.0);
        } else if let Some(error) = response.connection_error {
            demo.status = error.to_string();
        } else if response.graph_changed {
            demo.status = "Graph updated.".into();
        } else if response.selection_changed {
            demo.status = "Selection updated.".into();
        }
        row(cx).justify(Justify::SpaceBetween).gap(12) + |cx| {
            text(cx, &demo.status).font_size(12).color(Color::Muted);
            text!(cx, "{} nodes · {} edges", demo.graph.nodes.len(), demo.graph.edges.len())
                .font_size(12).color(Color::Muted);
        };
    };
}
