#![cfg(feature = "host")]
use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, Driver};
use plico_renderer::{SemanticRole, SemanticToggleState};

fn driver(page: Page) -> Driver<Gallery> {
    Driver::new(
        Gallery {
            router: Router::new(page),
            ..Default::default()
        },
        1120,
        820,
    )
    .unwrap()
}
fn click(driver: &mut Driver<Gallery>, role: &str, label: &str) {
    driver
        .execute(Command::Click {
            role: role.into(),
            label: label.into(),
            occurrence: None,
        })
        .unwrap();
}
#[test]
fn table_selection_survives_sort_filter_navigation_and_empty_results() {
    let mut d = driver(Page::DataTable);
    let snapshot = d.snapshot();
    let header = snapshot
        .node_by_role(SemanticRole::Button, "Owner", None)
        .unwrap();
    let cell = snapshot
        .node_by_role(SemanticRole::Label, "Mina", Some(0))
        .unwrap();
    assert!(
        (header.bounds[0] - cell.bounds[0]).abs() < 0.5,
        "headers and rows share tracks"
    );
    let id = d
        .snapshot()
        .node_by_role(SemanticRole::CheckBox, "Router", None)
        .unwrap()
        .id;
    click(&mut d, "checkbox", "Router");
    click(&mut d, "button", "Project");
    let snapshot = d.snapshot();
    let item = snapshot
        .node_by_role(SemanticRole::CheckBox, "Router", None)
        .unwrap();
    assert_eq!(item.id, id, "sorting must retain the same row");
    assert_eq!(item.states.checked, Some(SemanticToggleState::On));
    d.execute(Command::Focus {
        role: "textbox".into(),
        label: "Filter projects".into(),
        occurrence: None,
    })
    .unwrap();
    d.execute(Command::Text {
        text: "Arun".into(),
    })
    .unwrap();
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::CheckBox, "Router", None)
            .is_none()
    );
    click(&mut d, "checkbox", "Select visible");
    click(&mut d, "button", "Clear filter");
    assert_eq!(
        d.snapshot()
            .node_by_role(SemanticRole::CheckBox, "Router", None)
            .unwrap()
            .states
            .checked,
        Some(SemanticToggleState::On)
    );
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Label, "3 selected", None)
            .is_some()
    );
    click(&mut d, "button", "Layout");
    d.execute(Command::Key {
        key: "Alt+ArrowLeft".into(),
    })
    .unwrap();
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Label, "3 selected", None)
            .is_some()
    );
    d.execute(Command::Focus {
        role: "textbox".into(),
        label: "Filter projects".into(),
        occurrence: None,
    })
    .unwrap();
    d.execute(Command::Text {
        text: "does not exist".into(),
    })
    .unwrap();
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Label, "No matching projects", None)
            .is_some()
    );
    click(&mut d, "button", "Clear filter");
    click(&mut d, "button", "Clear selection");
    assert_eq!(
        d.snapshot()
            .node_by_role(SemanticRole::CheckBox, "Router", None)
            .unwrap()
            .states
            .checked,
        Some(SemanticToggleState::Off)
    );
}
#[test]
fn tree_scopes_keep_independent_selection_and_restore_after_navigation() {
    let mut d = driver(Page::TreeView);
    click(&mut d, "button", "Collapse crates");
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Button, "plico", None)
            .is_none()
    );
    click(&mut d, "button", "Expand crates");
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Button, "plico", None)
            .unwrap()
            .states
            .selected
            == Some(true)
    );
    d.execute(Command::Focus {
        role: "button".into(),
        label: "Expand apps".into(),
        occurrence: None,
    })
    .unwrap();
    d.execute(Command::Key {
        key: "Enter".into(),
    })
    .unwrap();
    click(&mut d, "button", "gallery");
    click(&mut d, "button", "Background worker");
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Button, "gallery", None)
            .unwrap()
            .states
            .selected
            == Some(true)
    );
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Button, "Background worker", None)
            .unwrap()
            .states
            .selected
            == Some(true)
    );
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Button, "target (ignored)", None)
            .unwrap()
            .disabled
    );
    click(&mut d, "button", "Layout");
    d.execute(Command::Key {
        key: "Alt+ArrowLeft".into(),
    })
    .unwrap();
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Button, "gallery", None)
            .unwrap()
            .states
            .selected
            == Some(true)
    );
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Button, "Background worker", None)
            .unwrap()
            .states
            .selected
            == Some(true)
    );
}

#[test]
fn chinese_status_filter_and_select_visible_use_the_same_rows() {
    let mut d = Driver::new(
        Gallery {
            language: gallery::i18n::Language::Chinese,
            router: Router::new(Page::DataTable),
            ..Default::default()
        },
        640,
        740,
    )
    .unwrap();
    d.execute(Command::Focus {
        role: "textbox".into(),
        label: "筛选项目".into(),
        occurrence: None,
    })
    .unwrap();
    d.execute(Command::Text {
        text: "审核中".into(),
    })
    .unwrap();
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::CheckBox, "Resource cache", None)
            .is_some()
    );
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::CheckBox, "Router", None)
            .is_none()
    );
    click(&mut d, "checkbox", "选择当前结果");
    click(&mut d, "button", "清除筛选");
    assert!(
        d.snapshot()
            .node_by_role(SemanticRole::Label, "2 项已选", None)
            .is_some()
    );
}
