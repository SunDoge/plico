#![cfg(feature = "widgets")]
use egui::{Context, Pos2, RawInput, Rect, Vec2};
use plico_core::{Engine, Entity, prelude::*};
use plico_egui::{Adapter, Source};
use plico_renderer::FrameSource;
use plico_runtime::View;

struct Demo {
    count: usize,
    enabled: bool,
    name: TextBuffer,
    extra: bool,
    value: f64,
}
impl Default for Demo {
    fn default() -> Self {
        Self {
            count: 0,
            enabled: true,
            name: TextBuffer::new(),
            extra: false,
            value: 0.25,
        }
    }
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx)
            .key("panel")
            .width_percent(100.)
            .height_percent(100.)
            .gap(8)
            .padding(12)
            .children(|cx| {
                if self.extra {
                    text(cx, "Inserted heading");
                }
                text(cx, "Measured and drawn by egui")
                    .key("label")
                    .font_size(18);
                if button(cx)
                    .key("increment")
                    .text("Increment")
                    .height(36.)
                    .width(150.)
                    .disabled(!self.enabled)
                    .clicked()
                {
                    self.count += 1;
                }
                checkbox(cx, "Enabled", &mut self.enabled).key("enabled");
                text_input(cx, "Name", &self.name)
                    .key("input")
                    .width_percent(100.)
                    .height(36.);
                slider(cx, "Volume", &mut self.value, 0. ..=1.)
                    .key("slider")
                    .width(200.)
                    .height(32.);
            });
    }
}
fn find(engine: &Engine, parent: Entity, key: &'static str) -> Entity {
    fn search(engine: &Engine, parent: Entity, key: &'static str) -> Option<Entity> {
        engine.find(parent, key).or_else(|| {
            engine
                .node(parent)?
                .children
                .iter()
                .find_map(|child| search(engine, *child, key))
        })
    }
    search(engine, parent, key).unwrap()
}
fn frame(
    ctx: &Context,
    adapter: &mut Adapter,
    source: &mut Source<Demo>,
    size: [f32; 2],
    events: Vec<egui::Event>,
) {
    let engine = source.native_widget_frame(size).unwrap();
    ctx.begin_pass(RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::from(size))),
        events,
        ..Default::default()
    });
    let changed = adapter.show(ctx, engine, size).unwrap();
    let mut output = ctx.end_pass();
    output.textures_delta.clear();
    if changed {
        source.native_widget_input_changed();
    }
}
fn click(ctx: &Context, adapter: &mut Adapter, source: &mut Source<Demo>, target: Entity) {
    let pos = adapter.rect(&source.engine, target).unwrap().center();
    for pressed in [true, false] {
        frame(
            ctx,
            adapter,
            source,
            [480., 360.],
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    frame(ctx, adapter, source, [480., 360.], vec![]);
}
#[test]
fn native_buttons_disabled_state_and_text_edits_roundtrip_to_plico() {
    let ctx = Adapter::new_context();
    let mut adapter = Adapter::default();
    let mut source = Source::new(Demo::default());
    frame(&ctx, &mut adapter, &mut source, [480., 360.], vec![]);
    frame(&ctx, &mut adapter, &mut source, [480., 360.], vec![]);
    let button = find(&source.engine, source.engine.root(), "increment");
    click(&ctx, &mut adapter, &mut source, button);
    assert_eq!(source.view.count, 1);
    let enabled = find(&source.engine, source.engine.root(), "enabled");
    click(&ctx, &mut adapter, &mut source, enabled);
    assert!(!source.view.enabled);
    click(&ctx, &mut adapter, &mut source, button);
    assert_eq!(source.view.count, 1, "disabled button must not activate");
    let slider = find(&source.engine, source.engine.root(), "slider");
    let r = adapter.rect(&source.engine, slider).unwrap();
    let pos = egui::pos2(r.right() - 12., r.center().y);
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut adapter,
            &mut source,
            [480., 360.],
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    frame(&ctx, &mut adapter, &mut source, [480., 360.], vec![]);
    assert!(
        source.view.value > 0.7,
        "native slider must update controlled value"
    );
    let input = find(&source.engine, source.engine.root(), "input");
    click(&ctx, &mut adapter, &mut source, input);
    frame(
        &ctx,
        &mut adapter,
        &mut source,
        [480., 360.],
        vec![egui::Event::Text("hello".into())],
    );
    assert_eq!(source.view.name.text().as_deref(), Some("hello"));
    source.view.name.set("External value".to_owned().into());
    frame(&ctx, &mut adapter, &mut source, [480., 360.], vec![]);
    assert_eq!(source.view.name.text().as_deref(), Some("External value"));
    source.view.extra = true;
    frame(&ctx, &mut adapter, &mut source, [480., 360.], vec![]);
    assert_eq!(
        find(&source.engine, source.engine.root(), "input"),
        input,
        "native editor identity must survive sibling insertion"
    );
}
#[test]
fn taffy_reflows_and_egui_measures_without_a_parley_text_context() {
    let ctx = Adapter::new_context();
    let mut adapter = Adapter::default();
    let mut source = Source::new(Demo::default());
    for size in [[480., 360.], [320., 600.], [640., 240.]] {
        frame(&ctx, &mut adapter, &mut source, size, vec![]);
        let input = find(&source.engine, source.engine.root(), "input");
        let rect = adapter.rect(&source.engine, input).unwrap();
        assert!(rect.width() <= size[0] - 24. + 1.);
        assert!(rect.width() >= size[0] - 24. - 1.);
        let label = find(&source.engine, source.engine.root(), "label");
        assert!(adapter.rect(&source.engine, label).unwrap().height() >= 18.);
    }
}
#[test]
fn secret_inputs_are_rejected_before_painting_instead_of_becoming_plaintext() {
    let ctx = Adapter::new_context();
    let mut adapter = Adapter::default();
    let mut source = Source::new(Demo {
        name: TextBuffer::secret(),
        ..Default::default()
    });
    let engine = source.native_widget_frame([480., 360.]).unwrap();
    ctx.begin_pass(RawInput::default());
    assert!(matches!(
        adapter.show(&ctx, engine, [480., 360.]),
        Err(plico_egui::Error::SecretInput)
    ));
    let mut output = ctx.end_pass();
    assert!(output.shapes.is_empty());
    output.textures_delta.clear();
}

#[test]
fn scrolling_is_owned_by_egui_and_reported_to_the_engine() {
    struct ScrollDemo;
    impl View for ScrollDemo {
        #[component]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx)
                .width_percent(100.)
                .height_percent(100.)
                .scroll_y(true)
                .key("scroll")
                .children(|cx| {
                    for index in 0..50u64 {
                        cx.keyed(index, |cx| {
                            column(cx).height(28.).shrink(0.).children(|cx| {
                                text(cx, "Scrollable row");
                            });
                        });
                    }
                });
        }
    }
    let ctx = Adapter::new_context();
    let mut adapter = Adapter::default();
    let mut source = Source::new(ScrollDemo);
    for n in 0..4 {
        let engine = source.native_widget_frame([320., 240.]).unwrap();
        let events = if n == 2 {
            vec![
                egui::Event::PointerMoved(egui::pos2(100., 100.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -160.),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: Default::default(),
                },
            ]
        } else {
            vec![]
        };
        ctx.begin_pass(RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(320., 240.))),
            events,
            ..Default::default()
        });
        adapter.show(&ctx, engine, [320., 240.]).unwrap();
        let mut output = ctx.end_pass();
        output.textures_delta.clear();
    }
    let scroll = find(&source.engine, source.engine.root(), "scroll");
    let [offset, height] = adapter.scroll_viewport(scroll).unwrap();
    assert!(offset > 0., "egui ScrollArea must move after wheel input");
    assert!(height > 0. && height <= 240.);
}
