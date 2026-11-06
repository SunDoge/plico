#![cfg(feature = "host")]

use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, Driver};
use plico_renderer::SemanticRole;

#[test]
fn patterns_repeated_rows_and_radios_keep_distinct_stable_identity() {
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::Patterns),
            ..Default::default()
        },
        1120,
        2600,
    )
    .unwrap();
    let labels = ["Small", "Medium", "Large"];
    let ids: Vec<_> = labels
        .iter()
        .map(|label| {
            driver
                .snapshot()
                .node_by_role(SemanticRole::RadioButton, label, None)
                .unwrap()
                .id
        })
        .collect();
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]);
    driver
        .execute(Command::Click {
            role: "switch".into(),
            label: "Dark theme".into(),
            occurrence: None,
        })
        .unwrap();
    for (label, id) in labels.iter().zip(ids) {
        assert_eq!(
            driver
                .snapshot()
                .node_by_role(SemanticRole::RadioButton, label, None)
                .unwrap()
                .id,
            id
        );
    }
}
