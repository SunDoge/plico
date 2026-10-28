use plico::prelude::*;
use plico_desktop::{Source, View};
use plico_renderer::{FrameSource, TextContext};
use std::time::Duration;
struct Demo {
    case: u8,
    expanded: bool,
}
#[component]
fn cell(cx: &mut UiContext, label: &str) {
    column(cx)
        .height(30.)
        .background(Color::Accent)
        .children(|cx| {
            text(cx, label);
        })
}
impl View for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        match self.case {
            0 => {
                grid(cx)
                    .width_percent(100.)
                    .column_gap(10)
                    .row_gap(8)
                    .grid_columns([
                        Track::Px(if self.expanded { 120. } else { 80. }),
                        fr(1.),
                        fr(2.),
                    ])
                    .children(|cx| {
                        cell(cx, "A");
                        cell(cx, "B");
                        cell(cx, "C");
                        column(cx)
                            .height(24.)
                            .column_span(2)
                            .background(Color::Selection)
                            .children(|cx| {
                                text(cx, "Span");
                            })
                    });
            }
            1 => {
                grid(cx)
                    .width(400.)
                    .height(200.)
                    .gap(10)
                    .grid_columns([Track::Px(80.), fr(1.)])
                    .grid_rows([Track::Px(40.), fr(1.)])
                    .grid_areas(["header header", "side main"])
                    .children(|cx| {
                        column(cx).grid_area("header").children(|cx| {
                            text(cx, "Header");
                        });
                        column(cx).grid_area("side").children(|cx| {
                            text(cx, "Side");
                        });
                        column(cx).grid_area("main").children(|cx| {
                            text(cx, "Main");
                        })
                    });
            }
            2 => {
                row(cx)
                    .width(400.)
                    .height(120.)
                    .align(Align::Start)
                    .padding_edges(Edges::new(px(10.), px(20.), px(30.), px(40.)))
                    .gap(20)
                    .children(|cx| {
                        column(cx).flex_basis(80).height(20.).children(|cx| {
                            text(cx, "First");
                        });
                        column(cx)
                            .flex_basis(60)
                            .height(20.)
                            .margin_left(AUTO)
                            .align_self(Align::End)
                            .children(|cx| {
                                text(cx, "Last");
                            })
                    });
            }
            3 => {
                column(cx)
                    .width(400.)
                    .height(200.)
                    .position(Position::Relative)
                    .children(|cx| {
                        column(cx)
                            .position(Position::Absolute)
                            .inset_right(10)
                            .inset_bottom(20)
                            .width(40.)
                            .height(30.)
                            .children(|cx| {
                                text(cx, "Pinned");
                            });
                        column(cx)
                            .width(percent(50.))
                            .aspect_ratio(2.)
                            .children(|cx| {
                                text(cx, "Ratio");
                            })
                    });
            }
            4 => {
                row(cx)
                    .width(200.)
                    .flex_direction(FlexDirection::RowReverse)
                    .gap(10)
                    .children(|cx| {
                        cell(cx, "Right");
                        cell(cx, "Left");
                    });
            }
            5 => {
                column(cx)
                    .width(100.)
                    .padding(10)
                    .border(2., Color::Border)
                    .box_sizing(BoxSizing::ContentBox)
                    .children(|cx| {
                        text(cx, "Content box");
                    });
            }
            6 => {
                column(cx).width(400.).height(160.).children(|cx| {
                    column(cx)
                        .width(percent(if self.expanded { 75. } else { 25. }))
                        .height(100.)
                        .padding_edges(Edges::all(px(if self.expanded { 30. } else { 10. })))
                        .motion(Motion::Smooth)
                        .children(|cx| {
                            text(cx, "Animated");
                        })
                });
            }
            7 => {
                row(cx).width(400.).children(|cx| {
                    column(cx)
                        .width(percent(90.))
                        .max_width(percent(60.))
                        .height(30.)
                        .children(|cx| {
                            text(cx, "Bounded");
                        });
                    column(cx).display(Display::None).children(|cx| {
                        text(cx, "Hidden");
                    })
                });
            }
            8 => {
                row(cx)
                    .width(400.)
                    .height(80.)
                    .align(Align::Baseline)
                    .gap(12)
                    .children(|cx| {
                        text(cx, "Small baseline").font_size(12).padding_top(5);
                        text(cx, "Large baseline").font_size(28);
                    });
            }
            9 => {
                grid(cx)
                    .width(200.)
                    .height(100.)
                    .grid_columns([fr(1.); 2])
                    .grid_rows([Track::Px(40.); 2])
                    .grid_flow(GridFlow::Column)
                    .children(|cx| {
                        cell(cx, "A");
                        cell(cx, "B");
                        cell(cx, "C");
                    });
            }
            10 => {
                grid(cx)
                    .width(300.)
                    .grid_columns([fr(1.); 3])
                    .grid_auto_rows([Track::Px(30.)])
                    .row_gap(8)
                    .grid_flow(GridFlow::RowDense)
                    .children(|cx| {
                        column(cx).column_span(2).children(|cx| {
                            text(cx, "Wide A");
                        });
                        column(cx).column_span(2).children(|cx| {
                            text(cx, "Wide B");
                        });
                        cell(cx, "Fill hole");
                    });
            }
            11 => {
                grid(cx)
                    .width(300.)
                    .grid_columns([
                        minmax(TrackSize::Px(100.), TrackSize::Fr(1.)),
                        Track::FitContent(px(80.)),
                    ])
                    .direction(Direction::Rtl)
                    .children(|cx| {
                        cell(cx, "Right");
                        cell(cx, "Left");
                    });
            }
            _ => {}
        }
    }
}
fn frame(s: &mut Source<Demo>, tcx: &mut TextContext, w: u32) {
    s.build_frame(tcx, w, 300);
    assert!(s.last_error.is_none(), "{:?}", s.last_error);
}
fn parent_rect(s: &Source<Demo>, label: &str) -> [f32; 4] {
    let p = s
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some(label))
        .unwrap();
    s.adapter
        .placed
        .iter()
        .find(|n| Some(n.node_id) == p.parent_node_id)
        .unwrap()
        .rect
}
fn setup(case: u8) -> (Source<Demo>, TextContext) {
    let mut s = Source::new(Demo {
        case,
        expanded: false,
    });
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 400);
    (s, tcx)
}
#[test]
fn grid_fraction_tracks_span_resize_and_template_invalidation() {
    let (mut s, mut tcx) = setup(0);
    assert_eq!(parent_rect(&s, "A"), [0., 0., 80., 30.]);
    assert_eq!(parent_rect(&s, "B"), [90., 0., 190., 30.]);
    assert_eq!(parent_rect(&s, "C"), [200., 0., 400., 30.]);
    assert_eq!(parent_rect(&s, "Span"), [0., 38., 190., 62.]);
    frame(&mut s, &mut tcx, 400);
    assert_eq!(s.adapter.stats.layout_runs, 0);
    s.view.expanded = true;
    frame(&mut s, &mut tcx, 400);
    assert_eq!(parent_rect(&s, "A")[2], 120.);
    assert_eq!(s.adapter.stats.created, 0);
    frame(&mut s, &mut tcx, 700);
    assert_eq!(parent_rect(&s, "C")[2], 700.);
}
#[test]
fn named_areas_map_to_rectangular_regions() {
    let (s, _) = setup(1);
    assert_eq!(parent_rect(&s, "Header"), [0., 0., 400., 40.]);
    assert_eq!(parent_rect(&s, "Side"), [0., 50., 80., 200.]);
    assert_eq!(parent_rect(&s, "Main"), [90., 50., 400., 200.]);
}
#[test]
fn flex_basis_auto_margin_self_alignment_and_four_edge_padding() {
    let (s, _) = setup(2);
    assert_eq!(parent_rect(&s, "First"), [40., 10., 120., 30.]);
    assert_eq!(parent_rect(&s, "Last"), [320., 70., 380., 90.]);
}
#[test]
fn right_bottom_insets_and_aspect_ratio() {
    let (s, _) = setup(3);
    assert_eq!(parent_rect(&s, "Pinned"), [350., 150., 390., 180.]);
    assert_eq!(parent_rect(&s, "Ratio"), [0., 0., 200., 100.]);
}
#[test]
fn reverse_flow_content_box_percentage_bounds_and_hidden_nodes() {
    let (s, _) = setup(4);
    assert!(parent_rect(&s, "Right")[0] > parent_rect(&s, "Left")[0]);
    let (s, _) = setup(5);
    assert_eq!(parent_rect(&s, "Content box")[2], 124.);
    let (s, _) = setup(7);
    assert_eq!(parent_rect(&s, "Bounded")[2], 240.);
    assert!(
        !s.adapter
            .placed
            .iter()
            .any(|p| p.paint.text.as_deref() == Some("Hidden"))
    );
}
#[test]
fn typed_percentage_and_edge_motion_use_virtual_clock() {
    let clock = plico::VirtualClock::default();
    let mut s = Source::with_clock(
        Demo {
            case: 6,
            expanded: false,
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    frame(&mut s, &mut tcx, 400);
    s.view.expanded = true;
    frame(&mut s, &mut tcx, 400);
    clock.advance(Duration::from_millis(120)).unwrap();
    frame(&mut s, &mut tcx, 400);
    let r = parent_rect(&s, "Animated");
    assert!((r[2] - 200.).abs() < 1., "{r:?}");
    let p = s
        .adapter
        .placed
        .iter()
        .find(|p| p.paint.text.as_deref() == Some("Animated"))
        .unwrap();
    assert!((p.content_origin[0] - 20.).abs() < 1.);
    clock.advance(Duration::from_millis(120)).unwrap();
    frame(&mut s, &mut tcx, 400);
    assert_eq!(parent_rect(&s, "Animated")[2], 300.);
    assert!(!s.has_anim());
}
#[test]
fn grid_payload_roundtrips_and_invalid_areas_are_rejected() {
    let (s, _) = setup(1);
    let nodes = s.engine.snapshot();
    let json = serde_json::to_string(&nodes).unwrap();
    let nodes: Vec<plico::ViewNode> = serde_json::from_str(&json).unwrap();
    plico_dev::validate_nodes(&nodes).unwrap();
    assert_eq!(nodes[0].grid.as_ref().unwrap().areas.len(), 2);
    let mut bad = nodes;
    bad[0].grid.as_mut().unwrap().areas = vec!["a b".into(), "b a".into()];
    assert!(plico_dev::validate_nodes(&bad).is_err());
    let mut engine = plico::Engine::default();
    assert!(
        engine
            .frame_declarations(|cx| {
                (grid(cx).grid_areas(["a b", "b a"])).children(|_cx| {});
            })
            .is_err()
    );
}

#[test]
fn shaped_text_baselines_align_across_sizes_and_padding() {
    let (mut s, mut tcx) = setup(8);
    let mut baselines = Vec::new();
    for p in &s.adapter.placed {
        if p.paint
            .text
            .as_deref()
            .is_some_and(|t| t.ends_with("baseline"))
        {
            let l = plico_renderer::scene::layout_node_text(&mut tcx, p).unwrap();
            baselines.push(p.content_origin[1] + l.lines().next().unwrap().metrics().baseline);
        }
    }
    assert_eq!(baselines.len(), 2);
    assert!((baselines[0] - baselines[1]).abs() < 1., "{baselines:?}");
    frame(&mut s, &mut tcx, 400);
    assert_eq!(s.adapter.stats.layout_runs, 0);
}
#[test]
fn column_flow_dense_packing_minmax_fit_content_and_rtl() {
    let (s, _) = setup(9);
    assert_eq!(parent_rect(&s, "B")[0], 0.);
    assert_eq!(parent_rect(&s, "B")[1], 40.);
    assert_eq!(parent_rect(&s, "C")[0], 100.);
    let (s, _) = setup(10);
    assert_eq!(parent_rect(&s, "Wide B")[1], 38.);
    assert_eq!(parent_rect(&s, "Fill hole")[0], 200.);
    assert_eq!(parent_rect(&s, "Fill hole")[1], 0.);
    let (s, _) = setup(11);
    let right = parent_rect(&s, "Right");
    let left = parent_rect(&s, "Left");
    assert!(right[0] > left[0]);
    assert!(right[2] - right[0] >= 100.);
    assert!(left[2] - left[0] <= 80.);
}
