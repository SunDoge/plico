use plico::prelude::*;

#[derive(Default, plico::serde::Serialize, plico::serde::Deserialize)]
#[serde(crate = "plico::serde")]
struct Counter {
    count: i32,
}

impl App for Counter {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).gap(12.).padding(24.)
            + |cx| {
                text!(cx, "Count: {}", self.count);
                if button(cx).text("Increment").primary().clicked() {
                    self.count += 1;
                }
            };
    }
}

fn main() -> Result<(), plico::DesktopError> {
    plico::run(Counter::default(), "Counter")
}
