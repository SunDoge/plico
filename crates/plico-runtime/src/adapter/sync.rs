//! Retained node synchronization and paint construction.
use super::*;

impl Adapter {
    pub(super) fn sync(
        &mut self,
        engine: &Engine,
        e: Entity,
        live: &mut HashSet<Entity>,
        width: u32,
        height: u32,
        recursive: SyncMode,
    ) -> NodeId {
        if !live.insert(e) {
            return self.nodes[&e].id;
        }
        self.stats.sync_visits += 1;
        let n = engine.node(e).unwrap();
        self.selection_dirty |= if n.style.selectable {
            self.selectable_nodes.insert(e)
        } else {
            self.selectable_nodes.remove(&e)
        };
        for (set, member) in [
            (
                &mut self.control_align_nodes,
                matches!(n.kind, Kind::Button | Kind::Switch | Kind::Checkbox),
            ),
            (&mut self.input_align_nodes, n.kind == Kind::TextInput),
            (
                &mut self.slider_nodes,
                n.kind == Kind::Slider
                    || (n.kind == Kind::ValueControl
                        && n.numeric.is_some_and(|range| range.high.is_some())),
            ),
            (&mut self.shader_nodes, n.kind == Kind::Shader),
            (
                &mut self.tooltip_nodes,
                n.kind == Kind::TooltipAnchor && !n.text.is_empty(),
            ),
        ] {
            if member {
                set.insert(e);
            } else {
                set.remove(&e);
            }
        }
        let mut target = n.style;
        let styled_button = n.kind == Kind::Button || n.style.styled_button;
        if n.style.styled_button && n.style.disabled {
            // Core attenuates composed buttons for all backends. This backend
            // moves that attenuation into the host-owned interaction weights.
            target.opacity /= 0.45;
        }
        if n.kind == Kind::Slider {
            target.checked = self.pressed == Some(e);
        }
        if styled_button && !target.material_background {
            // Constructors give ordinary buttons their face. An explicitly
            // empty face (ghost / toolbar) must stay transparent at rest.
            let inferred = match target.background {
                Some(Token::Control) => (Some(Token::ControlHover), Some(Token::ControlPressed)),
                Some(Token::Accent) => (Some(Token::AccentHover), Some(Token::AccentPressed)),
                _ => (None, None),
            };
            target.hover_background = target.hover_background.or(inferred.0);
            target.pressed_background = target.pressed_background.or(inferred.1);
        }
        // Text inputs keep their fill under the pointer and report hover on
        // the border instead (MyGO `inputBorder`), so they never infer a
        // Control/Accent hover fill from their base color.
        if interactive(n.kind) && !styled_button && !target.disabled && n.kind != Kind::TextInput {
            let base = target
                .background
                .or((n.kind == Kind::Button).then_some(Token::Control));
            let inferred = match base {
                Some(Token::Control) => (Some(Token::ControlHover), Some(Token::ControlPressed)),
                Some(Token::Accent) => (Some(Token::AccentHover), Some(Token::AccentPressed)),
                _ => (None, None),
            };
            if self.pressed == Some(e) {
                target.background = target.pressed_background.or(inferred.1).or(base);
            } else if self.hovered == Some(e) {
                target.background = target.hover_background.or(inferred.0).or(base);
            } else {
                target.background = base;
            }
        }
        let visual_style =
            self.motions
                .style(e, target, engine.theme(), !self.nodes.contains_key(&e));
        let (visual_style, focus_progress) = if styled_button {
            self.motions.button_style(
                e,
                visual_style,
                engine.theme(),
                button_state::Interaction {
                    hovered: self.hovered == Some(e) && !target.material_background,
                    pressed: self.pressed == Some(e) && !target.material_background,
                    focused: self.focus_visible && self.focused == Some(e),
                    disabled: target.disabled,
                },
            )
        } else {
            self.motions.remove_button(e);
            (visual_style, 0.)
        };
        let checked_progress = self.motions.checked(e, n.style.checked);
        let raster_correction = self
            .motions
            .raster_scale(e)
            .filter(|_| {
                visual_style.scale[0] > 0. && visual_style.scale[0] == visual_style.scale[1]
            })
            .map(|scale| (scale / f64::from(visual_style.scale[0])) as f32)
            .filter(|correction| (*correction - 1.).abs() > 1e-6);
        let svg = if n.kind == Kind::Svg {
            cached_svg(
                &mut self.svg_cache,
                &mut self.stats,
                n.text,
                svg_color(n.text, visual_style, engine.theme()),
            )
        } else {
            None
        };
        let image = if n.kind == Kind::Image {
            cached_image(&mut self.image_cache, &mut self.stats, n.text)
        } else {
            None
        };
        let is_control = interactive(n.kind);
        if recursive != SyncMode::Node
            && is_control
            && n.kind != Kind::Overlay
            && !visual_style.disabled
            && visual_style.tab_stop
        {
            self.tab_order.push(e);
        }
        let children: Vec<_> = if recursive != SyncMode::Node {
            n.children
                .iter()
                .map(|c| {
                    if recursive != SyncMode::Full && self.nodes.contains_key(c) {
                        self.nodes[c].id
                    } else {
                        self.sync(engine, *c, live, width, height, recursive)
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        // Reuse the retained layout for paint/text/transform-only updates.
        // Grid templates remain conservative: variable-size grid data lives
        // outside Copy Style, so layout_eq alone cannot validate them.
        let rebuild_layout = self.nodes.get(&e).is_none_or(|previous| {
            n.grid.is_some()
                || previous.had_grid
                || previous.paint_key.kind != n.kind
                || (matches!(n.kind, Kind::Switch | Kind::Checkbox)
                    && previous.paint_key.theme.metrics != engine.theme().metrics)
                || !visual_style.layout_eq(previous.paint_key.style)
                || (n.kind == Kind::Root
                    && previous.style.size
                        != Size {
                            width: length(width as f32),
                            height: length(height as f32),
                        })
        });
        let style = rebuild_layout.then(|| {
            self.stats.layout_style_builds += 1;
            let mut style = taffy::Style {
                display: Display::Flex,
                overflow: taffy::geometry::Point {
                    x: if visual_style.clip || visual_style.scroll_y {
                        taffy::Overflow::Hidden
                    } else {
                        taffy::Overflow::Visible
                    },
                    y: if visual_style.scroll_y {
                        taffy::Overflow::Scroll
                    } else if visual_style.clip {
                        taffy::Overflow::Hidden
                    } else {
                        taffy::Overflow::Visible
                    },
                },
                flex_direction: if matches!(n.kind, Kind::Row | Kind::ButtonBase) {
                    FlexDirection::Row
                } else {
                    FlexDirection::Column
                },
                align_items: visual_style.align.map_or_else(
                    || {
                        if matches!(
                            n.kind,
                            Kind::Root | Kind::Column | Kind::Dialog | Kind::TooltipAnchor
                        ) {
                            AlignItems::STRETCH
                        } else {
                            AlignItems::CENTER
                        }
                    },
                    |align| match align {
                        plico::Align::Start => AlignItems::START,
                        plico::Align::Center => AlignItems::CENTER,
                        plico::Align::End => AlignItems::END,
                        plico::Align::Stretch => AlignItems::STRETCH,
                        plico::Align::Baseline => AlignItems::BASELINE,
                    },
                ),
                justify_content: visual_style
                    .justify
                    .map_or(JustifyContent::NORMAL, |justify| match justify {
                        plico::Justify::Start => JustifyContent::START,
                        plico::Justify::Center => JustifyContent::CENTER,
                        plico::Justify::End => JustifyContent::END,
                        plico::Justify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
                        plico::Justify::SpaceAround => JustifyContent::SPACE_AROUND,
                        plico::Justify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
                        plico::Justify::Stretch => JustifyContent::STRETCH,
                    }),
                flex_wrap: if visual_style.wrap {
                    FlexWrap::Wrap
                } else {
                    FlexWrap::NoWrap
                },
                flex_grow: visual_style.grow,
                min_size: Size {
                    width: visual_style.min_width.map_or(length(0.), length),
                    height: visual_style.min_height.map_or(auto(), length),
                },
                max_size: Size {
                    width: visual_style.max_width.map_or(auto(), length),
                    height: visual_style.max_height.map_or(auto(), length),
                },
                gap: Size {
                    width: length(visual_style.gap),
                    height: length(visual_style.gap),
                },
                border: Rect {
                    left: length(visual_style.border_width),
                    right: length(visual_style.border_width),
                    top: length(visual_style.border_width),
                    bottom: length(visual_style.border_width),
                },
                padding: Rect {
                    left: length(visual_style.horizontal_padding()),
                    right: length(visual_style.horizontal_padding()),
                    top: length(visual_style.vertical_padding()),
                    bottom: length(visual_style.vertical_padding()),
                },
                size: Size {
                    width: visual_style.width_percent.map_or_else(
                        || visual_style.width.map_or(auto(), length),
                        |v| percent(v / 100.),
                    ),
                    height: visual_style.height_percent.map_or_else(
                        || visual_style.height.map_or(auto(), length),
                        |v| percent(v / 100.),
                    ),
                },
                flex_shrink: if is_control { 0. } else { visual_style.shrink },
                ..Default::default()
            };
            if let Some([left, top]) = visual_style.absolute {
                style.position = Position::Absolute;
                style.inset.left = length(left);
                style.inset.top = length(top);
            }
            if visual_style.overlay {
                style.position = Position::Absolute;
                style.inset = Rect {
                    left: length(0.),
                    right: length(0.),
                    top: length(0.),
                    bottom: length(0.),
                };
                style.size = Size {
                    width: percent(1.),
                    height: percent(1.),
                };
            }
            if n.kind == Kind::Root {
                style.size = Size {
                    width: length(width as f32),
                    height: length(height as f32),
                };
            }
            if is_control && !matches!(n.kind, Kind::ButtonBase | Kind::Overlay) {
                style.min_size.height = visual_style.min_height.map_or(length(36.), length);
                style.padding = Rect {
                    left: length(visual_style.horizontal_padding()),
                    right: length(visual_style.horizontal_padding()),
                    top: length(visual_style.vertical_padding()),
                    bottom: length(visual_style.vertical_padding()),
                };
            }
            layout_style::apply(&mut style, visual_style, n.grid);
            if matches!(n.kind, Kind::Switch | Kind::Checkbox) {
                style.padding.left =
                    length(toggle::left_padding(n.kind, visual_style, engine.theme()));
            }
            style
        });
        let key = PaintKey {
            style: visual_style,
            kind: n.kind,
            theme: engine.theme(),
            hovered: self.hovered == Some(e),
            pressed: self.pressed == Some(e),
            focused: (self.focus_visible || n.kind == Kind::TextInput) && self.focused == Some(e),
            checked_progress,
            focus_progress,
            raster_correction,
        };
        let visual_style = key.style;
        let font_family = resolve_font_family(visual_style.font_family, &self.font_family);
        if !self.nodes.contains_key(&e) {
            let id = self
                .tree
                .new_leaf_with_context(
                    style.as_ref().expect("new node layout").clone(),
                    Paint::default(),
                )
                .unwrap();
            self.nodes.insert(
                e,
                Retained {
                    id,
                    style: style.as_ref().expect("new node layout").clone(),
                    paint_key: key.clone(),
                    spans: n.spans.to_vec(),
                    text: Arc::from(n.text),
                    children: Vec::new(),
                    layout: None,
                    toggle_line_height: None,
                    had_grid: n.grid.is_some(),
                },
            );
            // Force the initial paint write, even for an empty label.
            self.reverse.insert(id, e);
            self.geometry_dirty = true;
            self.stats.created += 1;
            let mut paint = make_paint(
                n.kind,
                visual_style,
                self.nodes[&e].text.clone(),
                engine.theme(),
                (key.hovered, key.pressed, key.focused, key.checked_progress),
                font_family.clone(),
            );
            apply_button_focus(
                &mut paint,
                styled_button,
                key.focus_progress,
                engine.theme(),
            );
            paint.text_runs = rich_runs(n.spans, visual_style, engine.theme());
            paint.raster_scale_correction = raster_correction.unwrap_or(1.);
            paint.svg = svg.clone();
            paint.image = image.clone();
            self.tree.set_node_context(id, Some(paint)).unwrap();
            self.stats.paint_writes += 1;
        }
        let retained = self.nodes.get_mut(&e).unwrap();
        retained.had_grid = n.grid.is_some();
        if let Some(style) = style
            && retained.style != style
        {
            self.tree.set_style(retained.id, style.clone()).unwrap();
            retained.style = style;
            self.stats.style_writes += 1;
            self.geometry_dirty = true;
            self.geometry_roots.insert(e);
        }
        let visual_changes = visual_style.changes_from(retained.paint_key.style);
        self.stats.changes |= visual_changes;
        if visual_changes.contains(plico::Changes::TRANSFORM) {
            fn has_anchor(engine: &Engine, entity: Entity) -> bool {
                engine.node(entity).is_some_and(|n| {
                    n.style.anchor_id.is_some() || n.children.iter().any(|e| has_anchor(engine, *e))
                })
            }
            self.anchor_motion_dirty |= has_anchor(engine, e);
        }
        if visual_changes.contains(plico::Changes::TRANSFORM)
            || visual_style.opacity != retained.paint_key.style.opacity
        {
            self.presentation_dirty = true;
            self.presentation_roots.insert(retained.id);
        }
        let paint_needed = (matches!(
            n.kind,
            Kind::Switch | Kind::Checkbox | Kind::Slider | Kind::TextInput
        ) && visual_changes.contains(plico::Changes::LAYOUT))
            || visual_changes.intersects(plico::Changes::PAINT | plico::Changes::MEASURE)
            || retained.paint_key.kind != key.kind
            || retained.paint_key.theme != key.theme
            || retained.paint_key.focus_progress != key.focus_progress
            || (
                retained.paint_key.hovered,
                retained.paint_key.pressed,
                retained.paint_key.focused,
                retained.paint_key.checked_progress,
            ) != (key.hovered, key.pressed, key.focused, key.checked_progress)
            || retained.text.as_ref() != n.text
            || retained.spans != n.spans
            || self
                .tree
                .get_node_context(retained.id)
                .unwrap()
                .font_family
                .as_deref()
                != Some(font_family.as_ref());
        if paint_needed || retained.paint_key.raster_correction != raster_correction {
            self.stats.changes.insert(plico::Changes::PAINT);
            // This is the single retained paint mutation path: invalidate all
            // text-affecting values conservatively, including rich runs/theme.
            self.control_metrics.remove(&e);
            if retained.text.as_ref() != n.text {
                retained.text = Arc::from(n.text);
            }
            let mut paint = make_paint(
                n.kind,
                key.style,
                retained.text.clone(),
                engine.theme(),
                (key.hovered, key.pressed, key.focused, key.checked_progress),
                font_family.clone(),
            );
            apply_button_focus(
                &mut paint,
                styled_button,
                key.focus_progress,
                engine.theme(),
            );
            paint.text_runs = rich_runs(n.spans, visual_style, engine.theme());
            retained.spans = n.spans.to_vec();
            paint.raster_scale_correction = raster_correction.unwrap_or(1.);
            paint.svg = svg;
            paint.image = image;
            // Paint-only changes must not invalidate Taffy's text measurement cache.
            let old = self.tree.get_node_context(retained.id).unwrap();
            let measurement_changed = old.font_family != paint.font_family
                || old.text != paint.text
                || old.font_size != paint.font_size
                || old.font_weight != paint.font_weight
                || old.text_runs.len() != paint.text_runs.len()
                || old
                    .text_runs
                    .iter()
                    .zip(paint.text_runs.iter())
                    .any(|(old, new)| {
                        let mut metrics = old.clone();
                        metrics.color = new.color;
                        metrics != *new
                    });
            *self.tree.get_node_context_mut(retained.id).unwrap() = paint;
            self.paint_dirty.push(retained.id);
            if measurement_changed {
                self.stats.text_measure_invalidations += 1;
                self.geometry_dirty = true;
                self.geometry_roots.insert(e);
                self.tree.mark_dirty(retained.id).unwrap();
            }
            self.stats.paint_writes += 1;
        }
        retained.paint_key = key;
        if recursive != SyncMode::Node && retained.children != children {
            self.tree.set_children(retained.id, &children).unwrap();
            retained.children = children;
            self.stats.child_writes += 1;
            self.geometry_dirty = true;
            self.geometry_roots.insert(e);
            self.topology_roots.insert(e);
        }
        retained.id
    }
}
fn resolve_font_family(family: plico::FontFamily, default: &Arc<str>) -> Arc<str> {
    use plico::FontFamily;
    use std::sync::LazyLock;
    static MONO: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("monospace"));
    static SANS: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("sans-serif"));
    static SERIF: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("serif"));
    match family {
        FontFamily::Default => default.clone(),
        FontFamily::Monospace => MONO.clone(),
        FontFamily::SansSerif => SANS.clone(),
        FontFamily::Serif => SERIF.clone(),
    }
}

fn rich_runs(
    spans: &[plico::TextSpan],
    style: plico::Style,
    theme: Theme,
) -> Arc<[plico_renderer::text::TextRun]> {
    spans
        .iter()
        .map(|span| plico_renderer::text::TextRun {
            range: span.range.clone(),
            font_size: style.font_size as f32,
            font_weight: span.weight as f32,
            font_italic: span.italic,
            line_height: None,
            color: plico_renderer::text::brush_for_color(resolve(
                span.color.unwrap_or(style.foreground),
                theme,
            )),
        })
        .collect()
}
fn apply_button_focus(paint: &mut Paint, styled: bool, progress: f32, theme: Theme) {
    if styled {
        paint.outline_width = if progress > 0. { 2. } else { 0. };
        paint.outline_offset = 1.;
        paint.outline_color =
            (progress > 0.).then(|| resolve(Token::Focus, theme).with_alpha(progress));
    }
}

pub(crate) fn make_paint(
    kind: Kind,
    style: plico::Style,
    text: Arc<str>,
    theme: Theme,
    interaction: (bool, bool, bool, f32),
    font_family: Arc<str>,
) -> Paint {
    let (hovered, _, focus, _) = interaction;
    let mut p = Paint {
        text: (!text.is_empty()
            && !matches!(
                kind,
                Kind::Svg
                    | Kind::Shader
                    | Kind::Image
                    | Kind::Slider
                    | Kind::ValueControl
                    | Kind::ButtonBase
                    | Kind::TextInput
                    | Kind::Dialog
                    | Kind::Overlay
                    | Kind::TooltipAnchor
            ))
        .then_some(text),
        font_size: style.font_size as f32,
        font_weight: style.font_weight as f32,
        font_family: Some(font_family),
        text_color: resolve(style.foreground, theme),
        backdrop_blur: style.backdrop_blur,
        foreground_blur: style.foreground_blur,
        corner_smoothing: style.corner_smoothing,
        sticky_top: style.sticky_top,
        sticky_push: style.sticky_push,
        background: style
            .background
            .map(|c| resolve(c, theme).with_alpha(style.background_alpha)),
        opacity: style.opacity,
        runtime_transform: Some([
            style.scale[0],
            0.,
            0.,
            style.scale[1],
            style.translate[0],
            style.translate[1],
        ]),
        border_radii: CornerRadii::uniform(style.radius),
        border: (style.border_width > 0.).then(|| {
            // Explicit invalid or custom borders (Danger, literals) stay
            // exactly as declared; the default border follows the recipe.
            let color = match kind {
                Kind::TextInput if style.border_color == Token::Border => {
                    rgb(crate::recipe::input_border(theme, focus, hovered))
                }
                _ => resolve(style.border_color, theme),
            };
            (style.border_width, color)
        }),
        wrap_text: !style.text_ellipsis,
        text_ellipsis: style.text_ellipsis,
        ..Default::default()
    };
    for (shadow, inset) in [
        (style.ambient_shadow, false),
        (style.shadow, false),
        (style.inset_shadow, true),
    ]
    .into_iter()
    .filter_map(|(shadow, inset)| shadow.map(|shadow| (shadow, inset)))
    .filter(|(s, _)| s.opacity > 0.)
    {
        p.shadows.push(plico_renderer::style::Shadow {
            inset,
            offset_x: shadow.offset_x,
            offset_y: shadow.offset_y,
            spread: shadow.spread,
            std_dev: shadow.blur / 2.,
            color: resolve(shadow.color, theme).with_alpha(shadow.opacity),
            radius: None,
        });
    }
    if style.scroll_y {
        let muted = theme.muted;
        p.scrollbar = Some(Arc::new(plico_renderer::style::ScrollbarStyle {
            thickness: 6.,
            // Match MyGO's 12-DIP hit lane with a 6-DIP thumb centered in it:
            // the visible thumb rests three DIPs inside the scroll-port edge.
            margin: 3.,
            min_thumb_length: 24.,
            thumb_color: Color::from_rgba8(
                (muted >> 16) as u8,
                (muted >> 8) as u8,
                muted as u8,
                150,
            ),
            hover_color: resolve(Token::Muted, theme),
            active_color: resolve(Token::Muted, theme),
            ..Default::default()
        }));
    }
    if interactive(kind) {
        if style.disabled && !matches!(kind, Kind::ButtonBase | Kind::Button) {
            p.opacity *= 0.45;
        }
        // MyGO paint.go explicitly excludes kindInput from the generic
        // keyboard ring: inputBorder already provides its focus indicator.
        if focus && style.control_chrome && kind != Kind::TextInput {
            p.outline_width = 2.;
            p.outline_offset = 1.;
            p.outline_color = Some(resolve(Token::Focus, theme));
        }
    }

    p
}
