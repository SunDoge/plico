use plico::prelude::*;
use plico_runtime::regions::{
    OnClick, Reactive, RegionApp, RegionProgram, Regions, State, RegionUi,
};

#[component]
#[rustfmt::skip]
fn counters(cx: &mut UiContext, global: State<i32>) {
    grid(cx).width_percent(100.).height_percent(100.).padding(24).gap(16).grid_columns([fr(1.), fr(1.)])
        .grid_rows([fr(1.), fr(1.)]) + |cx|
    {
        cx.component((), |cx, ()| {
            let count = cx.local_state(|| 0_i32);
            counter(cx, "Local A · component state", count);
        });
        cx.component((), |cx, ()| {
            let count = cx.local_state(|| 0_i32);
            counter(cx, "Local B · component state", count);
        });
        counter(cx, "Global A · shared state", global);
        counter(cx, "Global B · shared state", global);
    };
}

struct Program;

impl RegionProgram for Program {
    fn build() -> Regions {
        Regions::new(|app| {
            let global = app.persisted_state(1, || 0_i32);
            app.root((global,), |cx, (global,)| counters(cx, global.handle()));
        })
    }
}

type Application = RegionApp<Program>;

#[component]
#[rustfmt::skip]
fn counter(cx: &mut UiContext, title: &str, count: State<i32>) {
    column(cx).min_width(0.).padding(16).gap(12).radius(12.).background(Color::Panel).border(1., Color::Border) + |cx| {
        text(cx, title).font_size(16).weight(600);
        text_empty(cx).font_size(36).rx(count, |node, count| {
            node.text(count.read().to_string());
        });
        row(cx).gap(8).wrap(true) + |cx| {
            button(cx).text("−").aria_label("Decrement").ghost()
                .on_click(count, |count| count.update(|value| *value -= 1));
            button(cx).text("+").aria_label("Increment").primary()
                .on_click(count, |count| count.update(|value| *value += 1));
            button(cx).text("Reset").on_click(count, |count| count.set(0));
        };
    };
}

fn main() -> Result<(), plico_desktop::DesktopError> {
    plico_desktop::run_with_options(
        Application::default(),
        plico_desktop::WindowOptions::new()
            .title("Local and global counters")
            .initial_inner_size(640, 480)
            .min_inner_size(560, 440),
    )
}
