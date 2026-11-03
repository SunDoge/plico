#![cfg(feature = "skia")]
use plico::{Theme, UiContext, View};
use plico_desktop::{NativeView, Source, capture};
use plico_renderer::{FrameSource, TextContext, layout::PlacedNode};
use std::{cell::Cell, rc::Rc};

struct TestView;
impl View for TestView {
    fn theme(&self) -> Theme {
        Theme::dark()
    }
    fn show(&mut self, cx: &mut UiContext<'_>) {
        plico::widgets::text(cx, "capture settles native layout");
    }
}
struct Settling {
    source: Source<NativeView<TestView>>,
    builds: usize,
    polls: usize,
    stats: Rc<Cell<(usize, usize)>>,
}
impl Settling {
    fn new() -> Self {
        Self {
            source: Source::new(NativeView(TestView)),
            builds: 0,
            polls: 0,
            stats: Rc::new(Cell::new((0, 0))),
        }
    }
}
impl FrameSource for Settling {
    fn build_frame(&mut self, text: &mut TextContext, width: u32, height: u32) -> Vec<PlacedNode> {
        self.builds += 1;
        self.stats.set((self.builds, self.polls));
        self.source.build_frame(text, width, height)
    }
    fn poll_async(&mut self) -> bool {
        let settle = self.polls < 2;
        self.polls += usize::from(settle);
        self.stats.set((self.builds, self.polls));
        settle
    }
    fn base_color(&self) -> peniko::Color {
        self.source.base_color()
    }
}

#[test]
fn capture_hides_settle_scene_and_pixel_setup() {
    let mut source = Settling::new();
    // Validate once through the ordinary source contract before capture borrows it.
    let mut text = TextContext::new();
    source.source.build_frame(&mut text, 100, 60);
    let stats = source.stats.clone();
    let frame = capture(&mut source)
        .size(160, 90)
        .settle_passes(4)
        .render()
        .unwrap();
    assert_eq!(stats.get(), (3, 2), "initial frame plus two settle passes");
    assert_eq!((frame.width(), frame.height()), (160, 90));
    assert_eq!(frame.rgba().len(), 160 * 90 * 4);
    assert!(
        frame
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| *pixel != [0, 0, 0, 0])
    );
}

#[test]
fn capture_writes_png_and_rejects_invalid_dimensions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture.png");
    capture(&mut Settling::new())
        .size(120, 80)
        .write_png(&path)
        .unwrap();
    let image = image::open(path).unwrap();
    assert_eq!((image.width(), image.height()), (120, 80));
    assert!(capture(&mut Settling::new()).size(0, 80).render().is_err());
}
