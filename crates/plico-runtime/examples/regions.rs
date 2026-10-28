use plico::prelude::*;
use plico_runtime::regions::{Access, OnClick, Regions, WatchUi};

#[component]
#[rustfmt::skip]
fn counters(cx: &mut UiContext, (count, expanded): (Access<'_, i32>, Access<'_, bool>)) {
    column(cx).width_percent(100.).padding(24).gap(16) + |cx| {
        text(cx, "ID-driven regions").font_size(26).weight(600);
        text(cx, "Native widgets · explicit dependencies · no captured callbacks");
        row(cx).gap(8) + |cx| {
            button(cx).text("Increment").primary().on_click(
                (count.handle(),),
                |(count,)| {
                    count.update(|v| *v += 1);
                },
            );
            button(cx).text("Reset").ghost().on_click((count.handle(),), |(count,)| {
                count.update(|v| *v = 0);
            });
            button(cx).text("Toggle details").on_click(
                (expanded.handle(),),
                |(expanded,)| {
                    expanded.update(|v| *v = !*v);
                },
            );
        };
        cx.watch((count.handle(),), |cx, (count,)| {
            text!(cx, "Count: {}", *count.read()).font_size(32);
        });
        cx.watch(
            (expanded.handle(), count.handle()),
            |cx, (expanded, count)| {
                if *expanded.read() {
                    column(cx).padding(16).gap(8).radius(12.).background(Color::Panel) + |cx| {
                        text(cx, "Nested region").weight(600);
                        cx.watch((count.handle(),), |cx, (count,)| {
                            text!(cx, "Double: {}", *count.read() * 2);
                        });
                    };
                }
            },
        );
    };
}
fn main() -> Result<(), plico_shell::Error> {
    let app = Regions::new(|app| {
        let count = app.state(|| 0_i32);
        let expanded = app.state(|| true);
        app.root((count, expanded), counters);
    });
    plico_shell::run_window_with_options(
        Box::new(plico_runtime::Source::new(app)),
        plico_runtime::WindowOptions::new()
            .title("Plico · native regions")
            .initial_inner_size(720, 480)
            .min_inner_size(560, 360)
            .renderer(plico_renderer::RenderBackend::Skia),
    )
}
