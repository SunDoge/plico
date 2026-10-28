use plico::{Changes, Engine, UiContext, component, prelude::*};

#[component]
fn counter(cx: &mut UiContext, click: bool, output: &mut i32) {
    let count = cx.state(|| 0);
    if click {
        count.update(|n| *n += 1);
    }
    *output = count.get();
    text(cx, "counter");
}
fn page(cx: &mut UiContext, first: bool, click: bool, out: &mut [i32; 2]) {
    if first {
        cx.keyed("a", |cx| counter(cx, click, &mut out[0]));
    }
    cx.keyed("b", |cx| counter(cx, click, &mut out[1]));
}
#[test]
fn local_state_survives_frames_is_keyed_and_unmount_resets_it() {
    let mut engine = Engine::default();
    let mut out = [0; 2];
    engine.frame(|cx| page(cx, true, true, &mut out)).unwrap();
    assert_eq!(out, [1, 1]);
    assert!(engine.changes().contains(Changes::VIEW));
    engine.frame(|cx| page(cx, false, true, &mut out)).unwrap();
    assert_eq!(out, [1, 2]);
    engine.frame(|cx| page(cx, true, false, &mut out)).unwrap();
    assert_eq!(out, [0, 2]);
    assert!(!engine.changes().contains(Changes::VIEW));
}
#[test]
fn conditional_state_call_does_not_shift_following_slot() {
    fn show(cx: &mut UiContext, extra: bool, out: &mut i32) {
        if extra {
            let _ = cx.state(String::new);
        }
        let state = cx.state(|| 42);
        *out = state.get();
        state.set(7);
    }
    let mut engine = Engine::default();
    let mut out = 0;
    engine.frame(|cx| show(cx, true, &mut out)).unwrap();
    assert_eq!(out, 42);
    engine.frame(|cx| show(cx, false, &mut out)).unwrap();
    assert_eq!(out, 7);
    assert!(!engine.changes().contains(Changes::VIEW));
}
#[test]
fn non_clone_values_can_be_borrowed_and_drop_after_unmount() {
    use std::{cell::Cell, rc::Rc};
    struct Value(Rc<Cell<usize>>);
    impl Drop for Value {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let mut engine = Engine::default();
    engine
        .frame(|cx| {
            let state = cx.state(|| Value(drops.clone()));
            assert_eq!(state.read().0.get(), 0);
        })
        .unwrap();
    assert_eq!(drops.get(), 0);
    engine.frame(|_| {}).unwrap();
    assert_eq!(drops.get(), 1);
}
