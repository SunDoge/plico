//! Native controls: Taffy supplies rectangles; egui owns measurement and interaction.
use egui::{Color32, Context, FontId, Pos2, Rect, RichText, Ui, Vec2};
use plico_core::{Engine, Entity, Kind, Style, Theme};
use snafu::Snafu;
use std::collections::HashMap;
use taffy::{NodeId, TaffyTree, prelude::*};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("egui backend does not support node {kind:?}"))]
    Unsupported { kind: Kind },
    #[snafu(display("secret inputs are not yet supported by the egui backend"))]
    SecretInput,
    #[snafu(display("egui window initialization failed: {message}"))]
    Window { message: String },
    #[snafu(display("Taffy layout failed: {source}"))]
    Layout { source: taffy::TaffyError },
}
#[derive(Clone)]
struct Measure {
    text: String,
    style: Style,
    kind: Kind,
}
struct Asset {
    uri: String,
    bytes: std::sync::Arc<[u8]>,
    source: std::sync::Arc<str>,
    color: u32,
}
pub struct Adapter {
    tree: TaffyTree<Measure>,
    nodes: HashMap<Entity, NodeId>,
    hovered: Option<Entity>,
    scroll_views: HashMap<Entity, [f32; 2]>,
    assets: HashMap<Entity, Asset>,
}
impl Default for Adapter {
    fn default() -> Self {
        Self {
            tree: TaffyTree::new(),
            nodes: HashMap::new(),
            hovered: None,
            scroll_views: HashMap::new(),
            assets: HashMap::new(),
        }
    }
}
fn color(token: plico_core::Color, theme: Theme) -> Color32 {
    let c = theme.resolve(token);
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}
fn font(style: Style) -> FontId {
    FontId::new(
        style.font_size as f32,
        if style.font_family == plico_core::FontFamily::Monospace {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        },
    )
}
fn rich(text: &str, style: Style, theme: Theme) -> RichText {
    let text = RichText::new(text)
        .font(font(style))
        .color(color(style.foreground, theme));
    if style.font_weight >= 600 {
        text.strong()
    } else {
        text
    }
}
fn put(ui: &mut Ui, rect: Rect, widget: impl egui::Widget) -> egui::Response {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min).with_cross_justify(true)),
        |ui| ui.add(widget),
    )
    .inner
}
impl Adapter {
    /// Number of live layout nodes retained by the adapter.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn new_context() -> Context {
        let ctx = Context::default();
        egui_extras::install_image_loaders(&ctx);
        ctx.add_font_provider(std::sync::Arc::new(
            egui_system_font_provider::SystemFontProvider::new(),
        ));
        ctx
    }
    pub(crate) fn validate(engine: &Engine, entity: Entity) -> Result<(), Error> {
        let Some(node) = engine.node(entity) else {
            return Ok(());
        };
        if matches!(node.kind, Kind::Shader | Kind::Image) {
            return Err(Error::Unsupported { kind: node.kind });
        }
        if engine
            .input(entity)
            .is_some_and(|(buffer, _)| buffer.is_secret())
        {
            return Err(Error::SecretInput);
        }
        for child in node.children {
            Self::validate(engine, *child)?;
        }
        Ok(())
    }
    fn sync(
        &mut self,
        engine: &Engine,
        entity: Entity,
        live: &mut std::collections::HashSet<Entity>,
    ) -> Result<NodeId, Error> {
        let node = engine.node(entity).expect("live declaration");
        live.insert(entity);
        let children = node
            .children
            .iter()
            .map(|child| self.sync(engine, *child, live))
            .collect::<Result<Vec<_>, _>>()?;
        let mut style = plico_core::taffy_layout::style(node.kind, node.style, node.grid);
        // Native scrollbars live in egui's viewport, not Taffy's allocation.
        style.scrollbar_width = 0.;
        let measure = Measure {
            text: node.text.into(),
            style: node.style,
            kind: node.kind,
        };
        let id = if let Some(id) = self.nodes.get(&entity).copied() {
            self.tree
                .set_style(id, style)
                .map_err(|source| Error::Layout { source })?;
            self.tree
                .set_node_context(id, Some(measure))
                .map_err(|source| Error::Layout { source })?;
            self.tree
                .set_children(id, &children)
                .map_err(|source| Error::Layout { source })?;
            id
        } else {
            let id = self
                .tree
                .new_with_children(style, &children)
                .map_err(|source| Error::Layout { source })?;
            self.tree
                .set_node_context(id, Some(measure))
                .map_err(|source| Error::Layout { source })?;
            self.nodes.insert(entity, id);
            id
        };
        Ok(id)
    }
    pub fn layout(&mut self, ctx: &Context, engine: &Engine, size: [f32; 2]) -> Result<(), Error> {
        Self::validate(engine, engine.root())?;
        let mut live = std::collections::HashSet::new();
        let root = self.sync(engine, engine.root(), &mut live)?;
        self.scroll_views.retain(|entity, _| live.contains(entity));
        self.assets.retain(|entity, asset| {
            if !live.contains(entity) || engine.node(*entity).is_none_or(|n| n.kind != Kind::Svg) {
                ctx.forget_image(&asset.uri);
                false
            } else {
                true
            }
        });
        for entity in &live {
            let node = engine.node(*entity).unwrap();
            if node.kind != Kind::Svg {
                continue;
            }
            let color = engine.theme().resolve(node.style.foreground);
            if self
                .assets
                .get(entity)
                .is_some_and(|old| old.source.as_ref() == node.text && old.color == color)
            {
                continue;
            }
            if let Some(old) = self.assets.remove(entity) {
                ctx.forget_image(&old.uri);
            }
            let source = node.text.replace("currentColor", &format!("#{color:06x}"));
            use std::hash::{Hash, Hasher};
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            source.hash(&mut hash);
            self.assets.insert(
                *entity,
                Asset {
                    uri: format!("bytes://plico-{entity:?}-{}.svg", hash.finish()),
                    bytes: std::sync::Arc::from(source.as_bytes()),
                    source: std::sync::Arc::from(node.text),
                    color,
                },
            );
        }
        let mut root_style = self
            .tree
            .style(root)
            .map_err(|source| Error::Layout { source })?
            .clone();
        root_style.size = Size {
            width: taffy::style_helpers::length(size[0]),
            height: taffy::style_helpers::length(size[1]),
        };
        self.tree
            .set_style(root, root_style)
            .map_err(|source| Error::Layout { source })?;
        let stale = self
            .nodes
            .keys()
            .filter(|e| !live.contains(e))
            .copied()
            .collect::<Vec<_>>();
        // Taffy removes links, not descendant objects: stale IDs can be removed
        // independently after every live parent's children have been updated.
        for entity in stale {
            self.tree
                .remove(self.nodes.remove(&entity).unwrap())
                .map_err(|source| Error::Layout { source })?;
        }
        self.tree
            .compute_layout_with_measure(
                root,
                Size {
                    width: AvailableSpace::Definite(size[0]),
                    height: AvailableSpace::Definite(size[1]),
                },
                |input, _, measure, style| {
                    taffy::compute_leaf_layout(
                        input,
                        style,
                        |_, _| 0.,
                        |known, available| {
                            let Some(m) = measure else {
                                return Size::ZERO;
                            };
                            if m.kind.family() == plico_core::KindFamily::Container {
                                return Size {
                                    width: known.width.unwrap_or(0.),
                                    height: known.height.unwrap_or(0.),
                                };
                            }
                            if m.kind == Kind::Svg {
                                return Size {
                                    width: known.width.unwrap_or(16.),
                                    height: known.height.unwrap_or(16.),
                                };
                            }
                            if m.kind == Kind::Slider {
                                return Size {
                                    width: known.width.unwrap_or(150.),
                                    height: known.height.unwrap_or(24.),
                                };
                            }
                            let width = known
                                .width
                                .or(match available.width {
                                    AvailableSpace::Definite(v) => Some(v),
                                    _ => None,
                                })
                                .unwrap_or(f32::INFINITY);
                            let extra = if matches!(m.kind, Kind::Checkbox | Kind::Switch) {
                                26.
                            } else {
                                0.
                            };
                            let text_size = ctx.fonts_mut(|f| {
                                let galley = if m.kind == Kind::Text && !m.style.text_ellipsis {
                                    f.layout(
                                        m.text.clone(),
                                        font(m.style),
                                        Color32::WHITE,
                                        (width - extra).max(0.),
                                    )
                                } else {
                                    f.layout_no_wrap(m.text.clone(), font(m.style), Color32::WHITE)
                                };
                                galley.size()
                            });
                            Size {
                                width: known.width.unwrap_or(text_size.x + extra),
                                height: known
                                    .height
                                    .unwrap_or(text_size.y.max(m.style.font_size as f32)),
                            }
                        },
                    )
                },
            )
            .map_err(|source| Error::Layout { source })?;
        Ok(())
    }
    pub fn bounds(&self, entity: Entity) -> Option<[f32; 4]> {
        let l = self.tree.layout(*self.nodes.get(&entity)?).ok()?;
        Some([l.location.x, l.location.y, l.size.width, l.size.height])
    }
    /// The native scroll offset and viewport height, in logical pixels.
    pub fn scroll_viewport(&self, entity: Entity) -> Option<[f32; 2]> {
        self.scroll_views.get(&entity).copied()
    }
    /// Window-local rectangle, for host diagnostics and deterministic tests.
    pub fn rect(&self, engine: &Engine, entity: Entity) -> Option<Rect> {
        fn locate(
            adapter: &Adapter,
            engine: &Engine,
            parent: Entity,
            target: Entity,
            origin: Pos2,
        ) -> Option<Rect> {
            let [x, y, w, h] = adapter.bounds(parent)?;
            let rect = Rect::from_min_size(origin + egui::vec2(x, y), egui::vec2(w, h));
            if parent == target {
                return Some(rect);
            }
            engine
                .node(parent)?
                .children
                .iter()
                .find_map(|child| locate(adapter, engine, *child, target, rect.min))
        }
        locate(self, engine, engine.root(), entity, Pos2::ZERO)
    }
    /// The same egui context measures and draws. No Scene, Parley or glyph cache.
    pub fn show(
        &mut self,
        ctx: &Context,
        engine: &mut Engine,
        size: [f32; 2],
    ) -> Result<bool, Error> {
        self.layout(ctx, engine, size)?;
        let mut events = Vec::new();
        ctx.root_ui(|ui| {
            self.draw(
                ui,
                engine,
                engine.root(),
                Pos2::ZERO,
                engine.theme(),
                &mut events,
            )
        });
        let hovered = events.iter().find_map(|e| {
            if let Event::Hover(entity) = e {
                Some(*entity)
            } else {
                None
            }
        });
        let mut changed = events
            .iter()
            .any(|e| !matches!(e, Event::Hover(_) | Event::Scroll(..)))
            || hovered != self.hovered;
        self.hovered = hovered;
        engine.set_hovered(hovered);
        for event in events {
            match event {
                Event::Hover(_) => {}
                Event::Scroll(entity, offset, height) => {
                    let previous = self.scroll_views.insert(entity, [offset, height]);
                    if engine.observes_scroll(entity) && previous != Some([offset, height]) {
                        changed = true;
                    }
                    engine.set_scroll_viewport(entity, offset, height);
                }
                Event::Click(e) => engine.queue_click(e),
                Event::Value(e, value) => engine.queue_value(e, value),
                Event::Text(e, text, submit) => {
                    if let Some((buffer, _)) = engine.input(e) {
                        if let Some(text) = text {
                            buffer.set(text.into());
                        }
                        if submit {
                            buffer.edit(plico_core::InputEdit::Submit);
                        }
                    }
                }
            }
        }
        Ok(changed)
    }
    fn draw(
        &self,
        ui: &mut Ui,
        engine: &Engine,
        entity: Entity,
        origin: Pos2,
        theme: Theme,
        events: &mut Vec<Event>,
    ) {
        let node = engine.node(entity).unwrap();
        let Some([x, y, w, h]) = self.bounds(entity) else {
            return;
        };
        if w <= 0. || h <= 0. || node.style.layout.display == plico_core::Display::None {
            return;
        }
        let rect = Rect::from_min_size(
            origin + egui::vec2(x, y) + Vec2::from(node.style.translate),
            egui::vec2(w, h),
        );
        ui.scope_builder(
            egui::UiBuilder::new().id_salt(entity).max_rect(rect),
            |ui| {
                if node.style.disabled {
                    ui.disable();
                }
                ui.multiply_opacity(node.style.opacity);
                if node.style.clip || node.style.scroll_y {
                    ui.set_clip_rect(ui.clip_rect().intersect(rect));
                }
                ui.visuals_mut().override_text_color = Some(color(node.style.foreground, theme));
                let style = node.style;
                if style.selectable {
                    ui.style_mut().interaction.selectable_labels = true;
                }
                if node.kind == Kind::ButtonBase {
                    ui.style_mut().interaction.selectable_labels = false;
                }
                let fill = style.background.map(|c| color(c, theme));
                let corner = egui::CornerRadius::same(style.radius.clamp(0., 255.) as u8);
                if !node.kind.interactive()
                    && let Some(fill) = fill
                {
                    ui.painter().rect_filled(rect, corner, fill);
                }
                if !node.kind.interactive() && style.border_width > 0. {
                    ui.painter().rect_stroke(
                        rect,
                        corner,
                        egui::Stroke::new(style.border_width, color(style.border_color, theme)),
                        egui::StrokeKind::Inside,
                    );
                }
                let response = match node.kind {
                    Kind::Text => {
                        let label = egui::Label::new(rich(node.text, style, theme))
                            .selectable(ui.style().interaction.selectable_labels)
                            .halign(egui::Align::Min);
                        Some(put(
                            ui,
                            rect,
                            if style.text_ellipsis {
                                label.truncate()
                            } else {
                                label.wrap()
                            },
                        ))
                    }
                    Kind::Button | Kind::ButtonBase | Kind::Overlay => {
                        let mut button = egui::Button::new(rich(
                            if node.kind == Kind::Button {
                                node.text
                            } else {
                                ""
                            },
                            style,
                            theme,
                        ))
                        .min_size(rect.size())
                        .corner_radius(corner);
                        if let Some(fill) = fill {
                            button = button.fill(fill);
                        }
                        Some(put(ui, rect, button))
                    }
                    Kind::Checkbox | Kind::Switch => {
                        let mut checked = style.checked;
                        Some(put(
                            ui,
                            rect,
                            egui::Checkbox::new(&mut checked, rich(node.text, style, theme)),
                        ))
                    }
                    Kind::Slider => node.numeric.map(|range| {
                        ui.spacing_mut().slider_width = rect.width();
                        let mut value = range.value;
                        let mut slider =
                            egui::Slider::new(&mut value, range.min..=range.max).show_value(false);
                        if range.step > 0. {
                            slider = slider.step_by(range.step);
                        }
                        if range.vertical {
                            slider = slider.vertical();
                        }
                        let r = put(ui, rect, slider);
                        if r.changed() {
                            events.push(Event::Value(entity, value));
                        }
                        r
                    }),
                    Kind::TextInput => engine.input(entity).map(|(buffer, hint)| {
                        let mut value = buffer.text().unwrap_or_default();
                        let r = put(
                            ui,
                            rect,
                            egui::TextEdit::singleline(&mut value)
                                .id(egui::Id::unique(entity))
                                .desired_width(rect.width())
                                .min_size(rect.size())
                                .font(font(style))
                                .hint_text(hint),
                        );
                        let submit =
                            r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if r.changed() || submit {
                            events.push(Event::Text(entity, r.changed().then_some(value), submit));
                        }
                        r
                    }),
                    Kind::Svg => self.assets.get(&entity).map(|asset| {
                        put(
                            ui,
                            rect,
                            egui::Image::from_bytes(asset.uri.clone(), asset.bytes.clone())
                                .fit_to_exact_size(rect.size()),
                        )
                    }),
                    _ => None,
                };
                if let Some(response) = response {
                    if response.hovered() && node.kind.interactive() {
                        events.push(Event::Hover(entity));
                    }
                    if response.clicked()
                        && matches!(
                            node.kind,
                            Kind::Button
                                | Kind::ButtonBase
                                | Kind::Checkbox
                                | Kind::Switch
                                | Kind::Overlay
                        )
                    {
                        events.push(Event::Click(entity));
                    }
                }
                let l = self.tree.layout(self.nodes[&entity]).unwrap();
                if style.scroll_y {
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(
                            l.border.left + l.padding.left,
                            l.border.top + l.padding.top,
                        ))),
                        |ui| {
                            let output = egui::ScrollArea::vertical()
                                .id_salt(entity)
                                .auto_shrink([false, false])
                                .show_viewport(ui, |ui, _| {
                                    let origin = ui.max_rect().min
                                        - egui::vec2(
                                            l.border.left + l.padding.left,
                                            l.border.top + l.padding.top,
                                        );
                                    for child in node.children {
                                        self.draw(ui, engine, *child, origin, theme, events);
                                    }
                                });
                            events.push(Event::Scroll(
                                entity,
                                output.state.clamped_offset().y,
                                output.inner_rect.height(),
                            ));
                        },
                    );
                } else {
                    for child in node.children {
                        self.draw(ui, engine, *child, rect.min, theme, events);
                    }
                }
            },
        );
    }
}
enum Event {
    Scroll(Entity, f32, f32),
    Hover(Entity),
    Click(Entity),
    Value(Entity, f64),
    Text(Entity, Option<String>, bool),
}
