# Node editor

`plico-node-editor` is a reusable, serializable node-graph model plus an
immediate Plico editor surface. It does not introduce a renderer widget or a
second state system.

```rust
let mut graph = plico_node_editor::Graph::default();
let source = graph.add_node("Source", [40., 80.], [200., 120.]);
let sink = graph.add_node("Sink", [360., 80.], [200., 120.]);
let output = graph.add_output(source, "Value", "number")?;
let input = graph.add_input(sink, "Value", "number")?;
graph.connect(
    plico_node_editor::Endpoint { node: source, port: output },
    plico_node_editor::Endpoint { node: sink, port: input },
)?;

plico_node_editor::node_editor(
    cx,
    &mut graph,
    &mut editor_state,
    Default::default(),
);
```

## Current contract

- Stable `NodeId`, `PortId`, and `EdgeId`; imported snapshots repair stale ID
  allocators before adding data.
- Directed output-to-input connections validate endpoints, application-defined
  port kinds, and duplicates.
- Removing a node cascades to its edges. Edges can also be removed directly.
- Graph and view state serialize independently. In-flight drag anchors remain
  transient Plico component state.
- Node headers drag in graph coordinates at every zoom level. Empty-canvas drag
  pans the graph. The toolbar zooms, resets, and deletes selected nodes.
- Connections use one generated SVG layer, rather than one layout node per
  curve. This is deliberately replaceable with a native WGPU path batch without
  changing the graph API.
- Ports connect by choosing an output and then an input. Wheel zoom, drag-wire
  preview, marquee/multi-selection, clipboard and undo commands are follow-up
  interaction layers, not silently claimed as complete.

The Gallery `Node editor` route and `scenarios/node-editor.json` provide the
visual and recipe entry points. Model tests cover validation, cascade removal,
serialization and edge generation; Gallery tests exercise real semantic port
activation, deletion and pointer dragging.

The pointer behavior and transformed-coordinate test strategy are informed by
MIT-licensed MyGO primitives. Attribution is recorded in the crate's
`THIRD_PARTY.md`; MyGO does not itself provide a complete node editor.

