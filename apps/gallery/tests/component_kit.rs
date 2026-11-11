use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, Driver};
#[test]
fn rust_gallery_kit_routes_actions_and_exposes_the_corner_study() {
    let mut ui = Driver::new(
        Gallery {
            router: Router::new(Page::ComponentKit),
            ..Default::default()
        },
        1280,
        1000,
    )
    .unwrap();
    for label in [
        "Next page",
        "Pinned",
        "Remove Rust",
        "Dismiss notification",
        "Clear filters",
    ] {
        ui.execute(Command::Click {
            role: "button".into(),
            label: label.into(),
            occurrence: None,
        })
        .unwrap();
    }
    assert_eq!(ui.view().component_kit.page, 2);
    assert!(ui.view().component_kit.pinned && ui.view().component_kit.cleared);
    assert!(!ui.view().component_kit.tag && !ui.view().component_kit.notice);
    ui.execute(Command::Click {
        role: "tab".into(),
        label: "Corners".into(),
        occurrence: None,
    })
    .unwrap();
    ui.execute(Command::SetValue {
        label: "Corner smoothing".into(),
        value: 0.65,
    })
    .unwrap();
    assert!((ui.view().component_kit.smoothing - 0.65).abs() < 1e-6);
    let mut amounts = Vec::new();
    fn walk(nodes: &[plico::ViewNode], amounts: &mut Vec<f32>) {
        for node in nodes {
            if node.style.radius == 28. {
                amounts.push(node.style.corner_smoothing);
            }
            walk(&node.children, amounts);
        }
    }
    walk(&ui.declarations(), &mut amounts);
    assert!(amounts.contains(&0.) && amounts.iter().any(|n| (*n - 0.65).abs() < 1e-6));
}
