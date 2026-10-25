use plico_core::{Engine, LocalState, UiContext, prelude::*};
fn slot<T: serde::Serialize + serde::de::DeserializeOwned + 'static>(
    cx: &mut UiContext,
    init: impl FnOnce() -> T,
) -> LocalState<T> {
    cx.persisted_state("draft", init)
}
#[test]
fn unmount_and_parent_recreation_preserve_state_but_keys_isolate_it() {
    let mut engine = Engine::default();
    engine
        .frame(|cx| {
            column(cx).key("page").children(|cx| {
                cx.keyed(1_u64, |cx| slot(cx, || 0).set(42));
                cx.keyed(2_u64, |cx| slot(cx, || 0).set(7));
            });
        })
        .unwrap();
    engine.frame(|_| {}).unwrap();
    engine
        .frame(|cx| {
            column(cx).key("page").children(|cx| {
                // Reverse order and rebuild the parent with a fresh arena generation.
                cx.keyed(2_u64, |cx| assert_eq!(slot(cx, || 0).get(), 7));
                cx.keyed(1_u64, |cx| assert_eq!(slot(cx, || 0).get(), 42));
            });
        })
        .unwrap();
}
#[test]
fn disk_roundtrip_preserves_unopened_slots_and_resets_only_incompatible_value() {
    let mut engine = Engine::default();
    engine
        .frame(|cx| {
            slot(cx, || 0).set(42);
            cx.persisted_state("other", String::new).set("saved".into());
        })
        .unwrap();
    let bytes = engine.dump_persisted().unwrap();
    let mut restored = Engine::default();
    restored.load_persisted(&bytes).unwrap();
    restored
        .frame(|cx| assert_eq!(slot(cx, || 0).get(), 42))
        .unwrap();
    // Taking a checkpoint must preserve fields not declared by this UI version.
    let mut next = Engine::default();
    next.load_persisted(&restored.dump_persisted().unwrap())
        .unwrap();
    next.frame(|cx| {
        assert_eq!(slot(cx, || "new type".to_owned()).get(), "new type");
        assert_eq!(&*cx.persisted_state("other", String::new).read(), "saved");
    })
    .unwrap();
    assert_eq!(next.take_persisted_resets().len(), 1);
    assert!(next.take_persisted_resets().is_empty());
}
#[test]
fn removal_scope_clear_and_invalid_import_are_explicit() {
    let mut engine = Engine::default();
    engine
        .frame(|cx| {
            cx.keyed("a", |cx| {
                slot(cx, || 0).set(8);
            });
            cx.keyed("b", |cx| {
                slot(cx, || 0).set(9);
            });
        })
        .unwrap();
    assert!(engine.load_persisted(b"invalid msgpack").is_err());
    engine
        .frame(|cx| {
            cx.keyed("a", |cx| {
                assert_eq!(slot(cx, || 0).get(), 8);
                assert!(cx.remove_persisted_state("draft"));
                assert_eq!(slot(cx, || 0).get(), 0);
                cx.clear_persisted_scope();
            });
            cx.keyed("b", |cx| assert_eq!(slot(cx, || 0).get(), 9));
        })
        .unwrap();
    engine.clear_persisted();
    assert!(engine.persisted_snapshot().unwrap().is_empty());
}
#[test]
fn values_are_not_serialized_during_declaration_or_writes() {
    use std::{cell::Cell, rc::Rc};
    #[derive(serde::Deserialize)]
    struct Counted {
        #[serde(skip)]
        calls: Rc<Cell<usize>>,
        value: i32,
    }
    impl serde::Serialize for Counted {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.calls.set(self.calls.get() + 1);
            self.value.serialize(serializer)
        }
    }
    let calls = Rc::new(Cell::new(0));
    let mut engine = Engine::default();
    for _ in 0..3 {
        engine
            .frame(|cx| {
                let _ = cx.persisted_state("counted", || Counted {
                    calls: calls.clone(),
                    value: 1,
                });
            })
            .unwrap();
    }
    assert_eq!(calls.get(), 0);
    engine.dump_persisted().unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn invalid_version_and_duplicate_keys_leave_live_store_unchanged() {
    let mut engine = Engine::default();
    engine
        .frame(|cx| {
            slot(cx, || 0).set(42);
        })
        .unwrap();
    let original = serde_json::to_value(engine.persisted_snapshot().unwrap()).unwrap();
    let mut invalid = original.clone();
    invalid["version"] = 9.into();
    let snapshot = serde_json::from_value(invalid).unwrap();
    assert!(engine.restore_persisted(snapshot).is_err());
    let mut duplicate = original;
    let entry = duplicate["entries"][0].clone();
    duplicate["entries"].as_array_mut().unwrap().push(entry);
    assert!(
        engine
            .restore_persisted(serde_json::from_value(duplicate).unwrap())
            .is_err()
    );
    engine
        .frame(|cx| assert_eq!(slot(cx, || 0).get(), 42))
        .unwrap();
}
