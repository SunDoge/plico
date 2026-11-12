#![cfg(feature = "host")]

use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, Driver};
use plico_renderer::SemanticRole;

fn click(driver: &mut Driver<Gallery>, label: &str) {
    driver
        .execute(Command::Click {
            role: "button".into(),
            label: label.into(),
            occurrence: None,
        })
        .unwrap();
}

#[test]
fn ports_connect_and_node_deletion_cascades_edges() {
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::NodeEditor),
            ..Default::default()
        },
        1120,
        820,
    )
    .unwrap();
    assert_eq!(driver.view().node_editor.graph.edges.len(), 2);
    click(&mut driver, "Output Image source: Image");
    click(&mut driver, "Input Preview: Mask");
    assert_eq!(driver.view().node_editor.graph.edges.len(), 3);
    assert!(
        driver
            .snapshot()
            .node_by_role(SemanticRole::Label, "3 nodes · 3 edges", None)
            .is_some()
    );

    click(&mut driver, "Move Image source");
    click(&mut driver, "Delete");
    assert_eq!(driver.view().node_editor.graph.nodes.len(), 2);
    assert_eq!(driver.view().node_editor.graph.edges.len(), 1);
}

#[test]
fn dragging_a_header_moves_the_graph_node_in_graph_coordinates() {
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::NodeEditor),
            ..Default::default()
        },
        1120,
        820,
    )
    .unwrap();
    let before = driver.view().node_editor.graph.nodes[0].position;
    let bounds = driver
        .snapshot()
        .node_by_role(SemanticRole::Button, "Move Image source", None)
        .unwrap()
        .bounds;
    let x = f64::from((bounds[0] + bounds[2]) * 0.5);
    let y = f64::from((bounds[1] + bounds[3]) * 0.5);
    for command in [
        Command::PointerMove { x, y },
        Command::PointerDown { x, y },
        Command::PointerMove {
            x: x + 36.,
            y: y + 24.,
        },
        Command::PointerUp {
            x: x + 36.,
            y: y + 24.,
        },
    ] {
        driver.execute(command).unwrap();
    }
    assert_eq!(
        driver.view().node_editor.graph.nodes[0].position,
        [before[0] + 36., before[1] + 24.]
    );
}
