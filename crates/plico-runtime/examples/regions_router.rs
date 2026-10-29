//! Page ownership through a reactive outlet; navigation history remains plain data.
use plico::{Router, Theme, prelude::*};
use plico_runtime::{
    Source,
    regions::{
        Access, BindState, OnChange, OnClick, OnClickWith, Reactive, RegionApp, RegionProgram,
        Regions, WatchUi, outlet,
    },
};

#[derive(
    Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, plico::Route,
)]
enum Page {
    #[default]
    #[route(id = "counter", label = "Counter")]
    Counter,
    #[route(id = "editor", label = "Editor")]
    Editor,
}
struct Program;
impl RegionProgram for Program {
    fn build() -> Regions {
        Regions::new(|app| {
            let router = app.persisted_state(1, Router::<Page>::default);
            let theme = app.persisted_state(2, Theme::light);
            app.theme_state(theme);
            app.root((router, theme), shell);
        })
    }
}
#[component]
#[rustfmt::skip]
fn shell(cx: &mut UiContext, (router, theme): (Access<'_, Router<Page>>, Access<'_, Theme>)) {
    column(cx).width_percent(100.).height_percent(100.).padding(24).gap(16).background(Color::Surface) + |cx| {
        text(cx, "Reactive router").font_size(28).weight(700);
        text(cx, "Route keys own page state. Leaving a page destroys its local state.").color(Color::Muted);
        cx.watch((router.handle(), theme.handle()), |cx, (router, theme)| {
            row(cx).gap(8).wrap(true).align(Align::Center) + |cx| {
                button(cx).text("Back").disabled(!router.read().can_back())
                    .on_click((router.handle(),), |(router,)| {
                    router.update(|r| {
                        r.back();
                    });
                });
                button(cx).text("Forward").disabled(!router.read().can_forward())
                    .on_click((router.handle(),), |(router,)| {
                    router.update(|r| {
                        r.forward();
                    });
                });
                for &page in Page::ALL {
                    let entry = button(cx).key(page.id()).text(page.label());
                    let entry = if page == router.read().current() { entry.primary() } else { entry.ghost() };
                    entry.on_click_with((router.handle(),), page, |(router,), page| {
                        router.update(|r| r.go(page))
                    });
                }
                let mut dark = *theme.read() == Theme::dark();
                checkbox(cx, "Dark theme", &mut dark).on_change(
                    (theme.handle(),),
                    |(theme,), dark| {
                        theme.set(if dark { Theme::dark() } else { Theme::light() });
                    },
                );
            };
        });
        text_empty(cx).color(Color::Muted).reactive((router.handle(),), |node, (router,)| {
            node.text(format!("Current page: {}", router.read().current().label()));
        });
        column(cx).height(0.).grow(1.).scroll_y(true).padding(20).background(Color::Panel).border(1., Color::Border)
            .radius(12.) + |cx|
        {
            outlet(cx, router.handle(), (), |cx, page, ()| match page {
                Page::Counter => counter(cx),
                Page::Editor => editor(cx),
            });
        };
    };
}
#[component]
#[rustfmt::skip]
fn counter(cx: &mut UiContext) {
    let count = cx.local_persisted_state(1, || 0_i32);
    column(cx).gap(16) + |cx| {
        text(cx, "Counter page").font_size(22).weight(600);
        text_empty(cx).font_size(48).reactive((count,), |node, (count,)| {
            node.text(format!("Count: {}", *count.read()));
        });
        row(cx).gap(8) + |cx| {
            button(cx).text("Increment").primary().on_click((count,), |(count,)| count.update(|v| *v += 1));
            button(cx).text("Reset").ghost().on_click((count,), |(count,)| count.set(0));
        };
        text(cx, "Click Counter again: keep the count. Visit Editor and go Back: start at zero.").color(Color::Muted);
    };
}
fn save_draft((draft, saved): (Access<'_, TextBuffer>, Access<'_, String>)) {
    saved.set(draft.read().text().unwrap_or_default());
}
#[component]
#[rustfmt::skip]
fn editor(cx: &mut UiContext) {
    let draft = cx.local_state(TextBuffer::new);
    let saved = cx.local_persisted_state(1, String::new);
    column(cx).gap(16) + |cx| {
        text(cx, "Editor page").font_size(22).weight(600);
        cx.watch((draft,), |cx, (draft,)| {
            text_input(cx, "Page draft", &draft.read()).placeholder("A page-local draft").width_percent(100.)
                .height(40.).bind(draft.handle());
        });
        button(cx).text("Save draft").primary().on_click((draft, saved), save_draft);
        text(cx, "No saved draft").color(Color::Muted).reactive((saved,), |node, (saved,)| {
            if !saved.read().is_empty() {
                node.text(format!("Saved: {}", *saved.read()));
            }
        });
        text(cx,"The draft and saved message reset when this page unmounts. Marked values can survive reload while mounted.")
            .color(Color::Muted);
    };
}
fn main() -> Result<(), plico_shell::Error> {
    plico_shell::run_window_with_options(
        Box::new(Source::new(RegionApp::<Program>::default())),
        plico_runtime::WindowOptions::new()
            .title("Plico · reactive router")
            .initial_inner_size(900, 700)
            .min_inner_size(520, 600)
            .renderer(plico_renderer::RenderBackend::Skia),
    )
}
