use plico_core::prelude::*;
#[component]
#[rustfmt::skip]
pub fn show(cx: &mut UiContext, expanded: &mut bool) {
    row(cx).gap(12).align(Align::Center).wrap(true) + |cx| {
        text(cx, "Layout playground").font_size(28).weight(750).grow(1.);
        if button(cx).text("Animate layout").primary().clicked() {
            *expanded = !*expanded;
        }
    };
    text(cx, "Tracks, constraints and alignment — with real layout.").font_size(13).color(Color::Muted);
    column(cx).height(0.).grow(1.).scroll_y(true).smooth_scroll(true).gap(16) + |cx| {
        column(cx).padding(18).gap(12).radius(14.).background(Color::Panel) + |cx| {
            row(cx).gap(12).justify(Justify::SpaceBetween) + |cx| {
                text(cx, "A workspace in three areas").font_size(16).weight(650);
                badge(cx, "CSS Grid");
            };
            grid(cx).width(percent(100.)).height(238.).column_gap(10).row_gap(8)
                .grid_columns([Track::Px(if *expanded { 170. } else { 110. }), fr(1.)])
                .grid_rows([Track::Px(36.), fr(1.), Track::Px(28.)])
                .grid_areas(["header header", "nav main", "footer footer"]).children(|cx| {
                row(cx).grid_area("header").padding_x(12).gap(8).radius(8.).background(Color::Selection) + |cx| {
                    icon(cx, &crate::icons::LAYOUT_GRID).size(15., 15.);
                    text(cx, "Project atlas").font_size(13).weight(650);
                    text(cx, "header / spans both columns").font_size(11).color(Color::Muted)
                        .margin_left(AUTO);
                };
                column(cx).grid_area("nav").padding(12).gap(10).radius(8.).background(Color::Control) + |cx| {
                    text(cx, "LIBRARY").font_size(10).weight(700).color(Color::Muted);
                    text(cx, "Overview").font_size(12).color(Color::Accent);
                    text(cx, "Collections").font_size(12);
                    text!(cx, "{:.0} px", if *expanded { 170. } else { 110. }).font_size(11)
                        .color(Color::Muted);
                };
                column(cx).grid_area("main").padding_edges(Edges::new(px(12.), px(16.), px(12.), px(16.)))
                    .gap(10).radius(8.).background(Color::Surface) + |cx|
                {
                    text(cx, "The remaining space").font_size(17).weight(650);
                    text(
                        cx,
                        "One fractional track absorbs changes in the sidebar and window width.",
                    ).width_percent(100.).font_size(12).color(Color::Muted);
                    grid(cx).grid_columns([fr(1.), fr(2.), fr(1.)]).column_gap(6).height(32.)
                        .margin_top(AUTO).children(|cx| {
                        for i in 0..3usize {
                            cx.keyed(i, |cx| {
                                column(cx).radius(5.).padding(6).background(Color::Selection)
                                    .children(|cx| {
                                    text(cx, ["1fr", "2fr", "1fr"][i]).font_size(11)
                                        .color(Color::Accent);
                                })
                            });
                        }
                    })
                };
                row(cx).grid_area("footer").padding_x(10).justify(Justify::SpaceBetween).radius(6.)
                    .background(Color::Control).children(|cx| {
                    text(cx, "Ready").font_size(11).color(Color::Muted);
                    text(cx, "named areas · independent row / column gaps").font_size(10)
                        .color(Color::Muted);
                })
            })
        };
        column(cx).padding(18).gap(12).radius(14.).background(Color::Panel) + |cx| {
            row(cx).justify(Justify::SpaceBetween) + |cx| {
                text(cx, "A flexible toolbar").font_size(16).weight(650);
                badge(cx, "Flexbox");
            };
            row(cx).width_percent(100.).height(84.)
                .padding_edges(Edges::new(px(10.), px(16.), px(10.), px(16.))).column_gap(10)
                .align(Align::Start).radius(9.).background(Color::Control) + |cx|
            {
                column(cx).flex_basis(if *expanded { 180. } else { 100. }).height(40.).padding(10)
                    .radius(7.).background(Color::Selection).motion(Motion::Smooth) + |cx|
                {
                    text(cx, "flex-basis").font_size(12).color(Color::Accent);
                };
                column(cx).flex_basis(90.).height(32.).align_self(Align::End).margin_left(AUTO).padding(8)
                    .radius(7.).background(Color::Selection).children(|cx| {
                    text(cx, "auto margin").font_size(11).color(Color::Accent);
                })
            };
            text(
                cx,
                "The leading item animates its basis; the trailing item stays at the far edge.",
            ).width_percent(100.).font_size(12).color(Color::Muted);
        };
        column(cx).padding(18).gap(12).radius(14.).background(Color::Panel).children(|cx| {
            text(cx, "One typographic baseline").font_size(16).weight(650);
            row(cx).gap(14).align(Align::Baseline).children(|cx| {
                text(cx, "Small").font_size(12).color(Color::Muted);
                text(cx, "Baseline").font_size(30).weight(700);
                text(cx, "aligned").font_size(18).color(Color::Accent);
            })
        })
    };
}
