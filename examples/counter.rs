use plico::prelude::*;

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Counters {
    global: i32,
}

impl plico_dev::App for Counters {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        grid(cx).width_percent(100.).height_percent(100.).padding(24).gap(16).grid_columns([fr(1.), fr(1.)])
            .grid_rows([fr(1.), fr(1.)]) + |cx|
        {
            local_counter(cx, "Local A · cx.state");
            local_counter(cx, "Local B · cx.state");
            counter(cx, "Global A · App state", &mut self.global);
            counter(cx, "Global B · App state", &mut self.global);
        };
    }
}

// Each component call site owns an independent local slot.
#[component]
fn local_counter(cx: &mut UiContext, title: &str) {
    let count = cx.state(|| 0);
    count.update(|count| counter(cx, title, count));
}

// A reusable component is just a function borrowing its caller's data.
#[component]
#[rustfmt::skip]
fn counter(cx: &mut UiContext, title: &str, count: &mut i32) {
    column(cx).min_width(0.).padding(16).gap(12).radius(12.).background(Color::Panel).border(1., Color::Border) + |cx| {
        text(cx, title).font_size(16).weight(600);
        text!(cx, "{count}").font_size(36);
        row(cx).gap(8).wrap(true) + |cx| {
            if button(cx).text("−").aria_label("Decrement").ghost().clicked() {
                *count -= 1;
            }
            if button(cx).text("+").aria_label("Increment").primary().clicked() {
                *count += 1;
            }
            if button(cx).text("Reset").clicked() {
                *count = 0;
            }
        };
    };
}

fn main() -> Result<(), plico_desktop::DesktopError> {
    plico_desktop::run_with_options(
        Counters::default(),
        plico_desktop::WindowOptions::new()
            .title("Local and global counters")
            .initial_inner_size(640, 480)
            .min_inner_size(560, 440),
    )
}
