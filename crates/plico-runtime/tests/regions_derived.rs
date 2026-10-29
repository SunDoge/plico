use plico::prelude::*;
use plico_renderer::{FrameSource, TextContext};
use plico_runtime::{
    Source, View,
    regions::{OnClick, RegionUi, Regions, State},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Model {
    selected: u32,
    category: u8,
    feedback: String,
}
struct Filter {
    source: State<Model>,
    category: u8,
}
impl PartialEq for Filter {
    fn eq(&self, other: &Self) -> bool {
        self.category == other.category
    }
}
fn app(persisted: bool) -> (Regions, State<Model>, State<Filter>, State<u8>) {
    let mut handles = None;
    let regions = Regions::new(|app| {
        let model = if persisted {
            app.persisted_state(70, Model::default)
        } else {
            app.state(Model::default)
        };
        let filter = app.derived(model, |model| Filter {
            source: model.handle(),
            category: model.read().category,
        });
        let feedback = app.derived(model, |model| model.read().feedback.clone());
        let doubled = app.derived(filter, |filter| filter.read().category * 2);
        handles = Some((model, filter, doubled));
        app.root(
            (model, filter, feedback, doubled),
            |cx, (model, filter, feedback, doubled)| {
                cx.region_keyed(1, model.handle(), |cx, model| {
                    text!(cx, "Selected {}", model.read().selected);
                    button(cx)
                        .text("Category")
                        .on_click(model.handle(), |model| {
                            model.update(|model| model.category += 1);
                        });
                });
                cx.region_keyed(2, filter.handle(), |cx, filter| {
                    text!(cx, "Category {}", filter.read().category);
                    button(cx)
                        .text("Filtered category")
                        .on_click(filter.read().source, |model| {
                            model.update(|model| model.category += 1)
                        });
                });
                cx.region_keyed(3, feedback.handle(), |cx, feedback| {
                    text(cx, &*feedback.read());
                });
                cx.region_keyed(4, doubled.handle(), |cx, doubled| {
                    text!(cx, "Doubled {}", *doubled.read());
                });
            },
        );
    });
    let (model, filter, doubled) = handles.unwrap();
    (regions, model, filter, doubled)
}
fn frame(source: &mut Source<Regions>, text: &mut TextContext) {
    source.experiment_prepare_without_scene(text, 600, 400);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
}
fn runs(source: &Source<Regions>) -> Vec<usize> {
    let mut runs = source.view.region_runs();
    runs.sort_by_key(|(entity, _)| entity.raw());
    runs.into_iter().map(|(_, runs)| runs).collect()
}
fn texts(source: &Source<Regions>) -> Vec<String> {
    source
        .adapter
        .placed
        .iter()
        .filter_map(|placed| placed.paint.text.clone())
        .map(|text| text.to_string())
        .collect()
}
fn contains(source: &Source<Regions>, label: &str) -> bool {
    texts(source).iter().any(|text| text == label)
}

#[test]
fn equal_outputs_do_not_notify_and_batches_replay_each_reader_once() {
    let (regions, model, filter, doubled) = app(false);
    let mut source = Source::new(regions);
    let mut text = TextContext::new();
    frame(&mut source, &mut text);
    let before = runs(&source);
    source.view.access(model).update(|model| model.selected = 1);
    frame(&mut source, &mut text);
    let after = runs(&source);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], before[1] + 1);
    assert_eq!(&after[2..], &before[2..]);

    let before = after;
    for selected in 2..=5 {
        source.view.access(model).update(|model| {
            model.selected = selected;
            model.category = 0;
        });
    }
    frame(&mut source, &mut text);
    let after = runs(&source);
    assert_eq!(after[1], before[1] + 1);
    assert_eq!(&after[2..], &before[2..]);

    source.view.access(model).update(|model| model.category = 3);
    frame(&mut source, &mut text);
    assert!(contains(&source, "Category 3"));
    assert!(contains(&source, "Doubled 6"));
    assert_eq!(source.view.access(filter).read().category, 3);
    assert_eq!(*source.view.access(doubled).read(), 6);

    let before = runs(&source);
    assert!(!source.view.access(model).update_if(|model| {
        model.category = 4;
        false
    }));
    assert_eq!(source.view.access(filter).read().category, 3);
    frame(&mut source, &mut text);
    assert_eq!(runs(&source), before);
    source.view.access(model).notify();
    frame(&mut source, &mut text);
    assert!(contains(&source, "Category 4"));
    assert!(contains(&source, "Doubled 8"));

    let local = texts(&source);
    source.build_frame(&mut text, 600, 400);
    assert!(source.last_error.is_none(), "{:?}", source.last_error);
    assert_eq!(
        texts(&source),
        local,
        "full declaration changed derived output"
    );
}

#[test]
fn unmounted_readers_rebind_to_the_latest_derived_value() {
    let mut handles = None;
    let regions = Regions::new(|app| {
        let model = app.state(Model::default);
        let mounted = app.state(|| true);
        let filter = app.derived(model, |model| Filter {
            source: model.handle(),
            category: model.read().category,
        });
        handles = Some((model, mounted));
        app.root((mounted, filter), |cx, (mounted, filter)| {
            cx.when(
                (mounted.handle(), filter.handle()),
                |(mounted, _)| *mounted.read(),
                |cx, (_, filter)| {
                    cx.region_keyed(1, filter.handle(), |cx, filter| {
                        text!(cx, "Category {}", filter.read().category);
                    });
                },
            );
        });
    });
    let (model, mounted) = handles.unwrap();
    let mut source = Source::new(regions);
    let mut text = TextContext::new();
    frame(&mut source, &mut text);
    assert!(contains(&source, "Category 0"));
    source.view.access(mounted).set(false);
    frame(&mut source, &mut text);
    source.view.access(model).update(|model| model.category = 7);
    frame(&mut source, &mut text);
    assert!(!contains(&source, "Category 7"));
    source.view.access(mounted).set(true);
    frame(&mut source, &mut text);
    assert!(contains(&source, "Category 7"));
}

#[test]
fn async_completion_uses_the_same_equality_and_destroyed_mailbox_rules() {
    let (regions, model, _, _) = app(false);
    let mut source = Source::new(regions);
    let mut text = TextContext::new();
    frame(&mut source, &mut text);
    let sender = source.view.access(model).sender();
    let before = runs(&source);
    sender
        .send(Model {
            selected: 9,
            category: 0,
            feedback: String::new(),
        })
        .unwrap();
    assert!(source.view.poll_reload());
    frame(&mut source, &mut text);
    assert!(contains(&source, "Selected 9"));
    let after = runs(&source);
    assert_eq!(after[1], before[1] + 1);
    assert_eq!(&after[2..], &before[2..]);
    drop(source);
    assert!(sender.send(Model::default()).is_err());
}

#[test]
fn checkpoint_recomputes_transient_derived_chains_from_restored_sources() {
    let (regions, model, _, _) = app(true);
    regions.access(model).update(|model| {
        model.selected = 11;
        model.category = 7;
        model.feedback = "restored".into();
    });
    let checkpoint = regions.checkpoint().unwrap();
    assert_eq!(
        serde_json::to_value(&checkpoint).unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "derived outputs entered the checkpoint"
    );
    let (mut restored, _, filter, doubled) = app(true);
    restored.restore_checkpoint(checkpoint);
    assert_eq!(restored.access(filter).read().category, 7);
    assert_eq!(*restored.access(doubled).read(), 14);
    let mut source = Source::new(restored);
    let mut text = TextContext::new();
    frame(&mut source, &mut text);
    assert!(contains(&source, "Selected 11"));
    assert!(contains(&source, "Category 7"));
    assert!(contains(&source, "Doubled 14"));
    assert!(contains(&source, "restored"));
}

#[test]
#[should_panic(expected = "derived state is read-only")]
fn derived_outputs_cannot_be_written_directly() {
    let (regions, _, filter, _) = app(false);
    regions.access(filter).update(|filter| filter.category = 9);
}

#[test]
#[should_panic(expected = "foreign or expired derived source")]
fn derived_rejects_foreign_source_handles() {
    let mut foreign = None;
    let _other = Regions::new(|app| {
        let model = app.state(Model::default);
        foreign = Some(model);
        app.root((), |_, _| {});
    });
    let _regions = Regions::new(|app| {
        app.derived(foreign.unwrap(), |model| model.read().category);
        app.root((), |_, _| {});
    });
}
