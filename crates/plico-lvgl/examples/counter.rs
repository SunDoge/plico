//! Minimal native-widget proof, not a second gallery.
use plico_core::{DeclareView, Engine, Entity, FrameError, Kind, Theme, prelude::*};
use plico_lvgl::{Adapter, oxivgl};
use snafu::{OptionExt, ResultExt, Snafu};
use std::path::PathBuf;

#[derive(usage_rs::Cli)]
#[usage(about = "Plico immediate API on native oxivgl widgets")]
struct Options {
    #[usage(long)]
    headless: bool,
    #[usage(long)]
    out: Option<PathBuf>,
}
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("declaration failed: {source}"))]
    Declaration { source: FrameError },
    #[snafu(display("backend failed: {source}"))]
    Backend { source: plico_lvgl::Error },
    #[snafu(display("LVGL snapshot allocation failed"))]
    Snapshot,
    #[snafu(display("snapshot write failed: {source}"))]
    Io { source: std::io::Error },
}
struct Counter {
    count: u32,
    extra: bool,
    disabled: bool,
    rows: Vec<u64>,
}
impl DeclareView for Counter {
    fn theme(&self) -> Theme {
        Theme::default()
    }

    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).padding(24).gap(14).width_percent(100.) + |cx| {
            text(cx, "Plico / LVGL").font_size(24);
            text!(cx, "Count: {}", self.count);
            row(cx).gap(12) + |cx| {
                if button(cx).text("Increment").disabled(self.disabled).primary().clicked()
                {
                    self.count += 1;
                }
                if button(cx).text("Toggle extra").clicked() {
                    self.extra = !self.extra;
                }
            };
            if self.extra && button(cx).text("Extra action").clicked() {
                self.count += 10;
            }
            row(cx).gap(12) + |cx| {
                for id in &self.rows {
                    text!(cx, "Item {id}").key(*id);
                }
            };
        };
    }
}
fn find(engine: &Engine, kind: Kind, text: &str) -> Entity {
    fn visit(engine: &Engine, id: Entity, kind: Kind, text: &str) -> Option<Entity> {
        let node = engine.node(id)?;
        if node.kind == kind && node.text == text {
            return Some(id);
        }
        node.children
            .iter()
            .find_map(|id| visit(engine, *id, kind, text))
    }
    visit(engine, engine.root(), kind, text).expect("declared widget")
}
fn frame(engine: &mut Engine, app: &mut Counter, adapter: &mut Adapter<'_>) -> Result<(), Error> {
    engine
        .declare_until_settled(app)
        .context(DeclarationSnafu)?;
    let delta = engine.take_frame_delta();
    adapter
        .sync_delta(engine, Some(delta))
        .context(BackendSnafu)
}
fn main() -> Result<(), Error> {
    let options = Options::parse();
    // Upstream permits one LVGL initialization per process; all calls stay on
    // this thread. There is no Plico renderer, Taffy, Parley or Vello here.
    let driver = if options.headless {
        oxivgl::driver::LvglDriver::init(480, 360)
    } else {
        oxivgl::driver::LvglDriver::sdl(480, 360)
            .title(c"Plico / oxivgl")
            .build()
    };
    let mut engine = Engine::default();
    engine.set_viewport(480., 360.).context(DeclarationSnafu)?;
    let mut app = Counter {
        count: 0,
        extra: false,
        disabled: false,
        rows: vec![1, 2],
    };
    let mut adapter = Adapter::new(&driver).context(BackendSnafu)?;
    frame(&mut engine, &mut app, &mut adapter)?;
    if options.headless {
        let increment = find(&engine, Kind::ButtonBase, "Increment");
        let original = adapter.object_id(increment);
        // Actual LVGL events run the registered oxivgl callback, which queues
        // only an Entity. App state changes in the next ordinary declaration.
        for _ in 0..3 {
            assert!(adapter.send_click(increment));
            assert_eq!(adapter.dispatch(&mut engine), 1);
            frame(&mut engine, &mut app, &mut adapter)?;
            assert_eq!(adapter.object_id(increment), original);
            assert_eq!(adapter.stats().created, 0);
            // Increment frames are paint-only: the incremental path must not
            // walk the whole tree (the structural pass counts every surviving
            // node as reused).
            assert_eq!(adapter.stats().reused, 0);
            assert_eq!(adapter.stats().removed, 0);
        }
        assert_eq!(app.count, 3);
        // A no-op delta skips the structural walk entirely: nothing is
        // created, removed, or updated, and stats stay at zero.
        let noop = plico_core::FrameDelta::default();
        adapter
            .sync_delta(&engine, Some(noop))
            .context(BackendSnafu)?;
        assert_eq!(adapter.stats().style_updates, 0);
        assert_eq!(adapter.stats().text_updates, 0);
        assert_eq!(adapter.stats().created, 0);
        assert_eq!(adapter.stats().removed, 0);
        // Increment clicks are paint-only frames: the incremental path handles
        // them without a full walk (reused stays zero because it is counted
        // only by the structural walk).
        assert_eq!(adapter.stats().reused, 0);

        let item = find(&engine, Kind::Text, "Item 1");
        let item_object = adapter.object_id(item);
        app.extra = true;
        app.rows.reverse();
        frame(&mut engine, &mut app, &mut adapter)?;
        assert_eq!(adapter.object_id(item), item_object);
        let second = find(&engine, Kind::Text, "Item 2");
        assert!(adapter.bounds(item).unwrap()[0] > adapter.bounds(second).unwrap()[0]);
        let old_extra = find(&engine, Kind::ButtonBase, "Extra action");
        adapter.send_click(old_extra);
        app.extra = false;
        frame(&mut engine, &mut app, &mut adapter)?;
        assert!(adapter.object_id(old_extra).is_none());
        app.extra = true;
        frame(&mut engine, &mut app, &mut adapter)?;
        assert_ne!(find(&engine, Kind::ButtonBase, "Extra action"), old_extra);
        assert_eq!(adapter.dispatch(&mut engine), 0); // stale click cannot hit a remount

        app.disabled = true;
        frame(&mut engine, &mut app, &mut adapter)?;
        adapter.send_click(increment);
        assert_eq!(adapter.dispatch(&mut engine), 0);
        app.disabled = false;
        frame(&mut engine, &mut app, &mut adapter)?;
        let before = adapter.object_id(increment);
        // Kinds without a native widget no longer fail the sync: a checkbox
        // degrades along KindFamily into a labelled container without
        // interaction. A second adapter proves the projection mounts and
        // reports bounds; the primary engine's widgets stay untouched.
        let mut degraded = Engine::default();
        degraded
            .frame_declarations(|cx| {
                checkbox(cx, "Degraded", &mut false);
            })
            .context(DeclarationSnafu)?;
        let mut degraded_adapter = Adapter::new(&driver).context(BackendSnafu)?;
        degraded_adapter.sync(&degraded).context(BackendSnafu)?;
        // Whatever node count the composite checkbox declares, the projection
        // mounts them all and the root reports bounds.
        assert!(degraded_adapter.bounds(degraded.root()).is_some());
        assert!(degraded_adapter.stats().created > 0);
        assert_eq!(adapter.object_id(increment), before);
        println!(
            "PASS: native click round-trip, object reuse, no-op sync, keyed reorder, conditional unmount, stale events and disabled input"
        );
    } else {
        loop {
            let delay = driver.timer_handler();
            if adapter.dispatch(&mut engine) > 0 {
                frame(&mut engine, &mut app, &mut adapter)?;
            }
            std::thread::sleep(std::time::Duration::from_millis(u64::from(
                delay.clamp(1, 16),
            )));
        }
    }
    if let Some(path) = options.out {
        let snapshot = oxivgl::snapshot::Snapshot::take(&driver).context(SnapshotSnafu)?;
        snapshot.write_png(&path).context(IoSnafu)?;
        println!("snapshot: {}", path.display());
    }
    Ok(())
}
