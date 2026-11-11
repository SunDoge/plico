use plico::prelude::*;

#[derive(Default)]
struct Application {
    local_a: i32,
    local_b: i32,
    global: i32,
}

impl plico_dev::App for Application {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext<'_>) {
        grid(cx).width_percent(100.).height_percent(100.).padding(24).gap(16).grid_columns([fr(1.), fr(1.)])
            .grid_rows([fr(1.), fr(1.)]) + |cx|
        {
            counter(cx, "Local A · application state", &mut self.local_a);
            counter(cx, "Local B · application state", &mut self.local_b);
            counter(cx, "Global A · shared state", &mut self.global);
            counter(cx, "Global B · shared state", &mut self.global);
        };
    }
}

#[component]
#[rustfmt::skip]
fn counter(cx: &mut UiContext, title: &str, count: &mut i32) {
    column(cx).min_width(0.).padding(16).gap(12).radius(12.).background(Color::Panel).border(1., Color::Border) + |cx| {
        text(cx, title).font_size(16).weight(600);
        text!(cx, "{}", *count).font_size(36);
        row(cx).gap(8).wrap(true) + |cx| {
            if button(cx).text("−").aria_label("Decrement").ghost().clicked() { *count -= 1; }
            if button(cx).text("+").aria_label("Increment").primary().clicked() { *count += 1; }
            if button(cx).text("Reset").clicked() { *count = 0; }
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
