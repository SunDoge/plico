//! Retained validated GPU surfaces, driven by the injected UI clock.
use super::*;
use plico::ShaderParams;
use plico_renderer::PaintScene;
use plico_renderer::{ShaderEffectId, ShaderEffectSource, widget::PaintContext};
use std::time::Duration;

struct ScalarTransition {
    from: f32,
    to: f32,
    started: Duration,
    duration: Duration,
}
impl ScalarTransition {
    fn sample(&self, now: Duration) -> (f32, bool) {
        let t = (now.saturating_sub(self.started).as_secs_f32() / self.duration.as_secs_f32())
            .clamp(0., 1.);
        if t >= 1. {
            (self.to, false)
        } else {
            (
                self.from + (self.to - self.from) * (t * t * (3. - 2. * t)),
                true,
            )
        }
    }
}
/// Override a retained shader's canvas for this frame in window coordinates.
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct SurfacePose {
    pub bounds: Option<[f32; 4]>,
    pub popup_geometry: Option<[f32; 12]>,
    pub popup_optics: Option<PopupOptics>,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum PopupOptics {
    Morph { frost: f32, visible: bool },
    List { opening: f32, radius: f32 },
}

pub(super) struct Surface {
    id: ShaderEffectId,
    authored: String,
    source: Option<ShaderEffectSource>,
    error: Option<String>,
    params: ShaderParams,
    values: Arc<[f32]>,
    elapsed: f32,
    transitions: HashMap<usize, ScalarTransition>,
    sampled: Duration,
    painted: Option<([f32; 4], [f32; 2], f32)>,
    popup_pose: SurfacePose,
}
impl Surface {
    fn sample_values(&mut self, now: Duration, enabled: bool) {
        if self.transitions.is_empty() {
            return;
        }
        let mut values = self.values.to_vec();
        self.transitions.retain(|&index, transition| {
            let (value, active) = if enabled {
                transition.sample(now)
            } else {
                (transition.to, false)
            };
            if let Some(slot) = values.get_mut(index) {
                *slot = value;
            }
            active
        });
        if self.values.as_ref() != values.as_slice() {
            self.values = values.into();
            self.painted = None;
        }
    }
    fn running(&self) -> bool {
        self.source.is_some()
            && self.params.animated
            && !self.params.paused
            && self.params.speed > 0.
    }
}
impl Adapter {
    pub fn shader_diagnostics(&self) -> impl Iterator<Item = &str> {
        self.shaders
            .values()
            .filter_map(|surface| surface.error.as_deref())
    }
    pub(super) fn shader_active(&self) -> bool {
        self.shader_motion_enabled
            && self
                .shaders
                .values()
                .any(|s| s.running() || !s.transitions.is_empty())
    }
    pub(super) fn update_shaders(&mut self, engine: &Engine, tcx: &mut TextContext) -> bool {
        self.shaders
            .retain(|entity, _| engine.node(*entity).is_some_and(|n| n.kind == Kind::Shader));
        let now = engine.time().now;
        let mut changed = false;
        for &entity in &self.shader_nodes {
            let Some(index) = self
                .nodes
                .get(&entity)
                .and_then(|n| self.placed_indices.get(&n.id))
                .copied()
            else {
                continue;
            };
            self.stats.shader_visits += 1;
            let node = &mut self.placed[index];
            let logical = engine.node(entity).unwrap();
            if logical.kind != Kind::Shader {
                continue;
            }
            let Some(params) = logical.shader else {
                continue;
            };
            let surface = self.shaders.entry(entity).or_insert_with(|| Surface {
                id: ShaderEffectId::new(),
                authored: String::new(),
                source: None,
                error: None,
                params: params.clone(),
                values: params.values.clone().into(),
                elapsed: 0.,
                transitions: HashMap::new(),
                sampled: now,
                painted: None,
                popup_pose: SurfacePose::default(),
            });
            if self.shader_motion_enabled && !self.shader_reset_time && surface.running() {
                surface.elapsed +=
                    now.saturating_sub(surface.sampled).as_secs_f32() * surface.params.speed;
            }
            surface.sampled = now;
            surface.sample_values(now, self.shader_motion_enabled);
            let program_changed = surface.authored != logical.text
                || surface.params.source_path != params.source_path
                || surface.params.module != params.module
                || surface.params.reads_backdrop != params.reads_backdrop
                || surface.params.prefilter_backdrop != params.prefilter_backdrop
                || surface.params.builtin != params.builtin
                || surface.source.is_none() && surface.error.is_none();
            let values_changed = surface.params.values != params.values
                || surface.params.parameters != params.parameters
                || surface.params.transitions != params.transitions;
            if program_changed || values_changed {
                let result = if program_changed || surface.error.is_some() {
                    if params.module {
                        ShaderEffectSource::new_module(logical.text)
                    } else {
                        ShaderEffectSource::with_backdrop(logical.text, params.reads_backdrop)
                    }
                    .and_then(|source| source.with_backdrop_prefilter(params.prefilter_backdrop))
                    .and_then(|source| source.with_builtin(params.builtin))
                } else {
                    surface
                        .source
                        .clone()
                        .ok_or_else(|| "shader has no valid program".to_owned())
                };
                let result = result.and_then(|source| {
                    if !params.parameters.is_empty() && !params.values.is_empty() {
                        return Err(
                            "choose named shader parameters or raw values(), not both".into()
                        );
                    }
                    let values = if params.parameters.is_empty() {
                        params.values.clone()
                    } else {
                        source.encode_parameters(&params.parameters)?
                    };
                    if params.transitions.iter().any(|t| t.index >= values.len()) {
                        return Err("shader transition index is outside the uniform values".into());
                    }
                    Ok((source, values))
                });
                match result {
                    Ok((source, mut values)) => {
                        surface.source = Some(source);
                        let mut previous = std::mem::take(&mut surface.transitions);
                        if !program_changed && self.shader_motion_enabled {
                            for transition in &params.transitions {
                                if transition.milliseconds == 0 {
                                    continue;
                                }
                                let index = transition.index;
                                let duration =
                                    Duration::from_millis(u64::from(transition.milliseconds));
                                if let Some(active) = previous.remove(&index)
                                    && active.to == values[index]
                                    && active.duration == duration
                                {
                                    values[index] = surface.values[index];
                                    surface.transitions.insert(index, active);
                                    continue;
                                }
                                if let Some(&from) = surface.values.get(index)
                                    && from != values[index]
                                {
                                    surface.transitions.insert(
                                        index,
                                        ScalarTransition {
                                            from,
                                            to: values[index],
                                            started: now,
                                            duration: Duration::from_millis(u64::from(
                                                transition.milliseconds,
                                            )),
                                        },
                                    );
                                    values[index] = from;
                                }
                            }
                        }
                        surface.values = values.into();
                        surface.error = None;
                    }
                    Err(error) => {
                        surface.error = Some(match &params.source_path {
                            Some(path) => format!("{path}: {error}"),
                            None => error,
                        });
                    }
                }
                surface.authored.clear();
                surface.authored.push_str(logical.text);
                surface.painted = None;
            }
            if &surface.params != params {
                surface.params.clone_from(params);
                surface.painted = None;
            }
            // Resolve the animated effect canvas in presentation space.
            // Keep the node's layout bounds and the ancestor clip chain intact.
            let [outset_x, outset_y] = params.paint_outset;
            let pose = self.effect_poses.get(&entity).copied().unwrap_or_default();
            let rect = pose.bounds;
            let [x0, y0, x1, y1] = rect.unwrap_or(node.scroll.port);
            if logical.style.clip && node.own_clip.is_some() {
                node.own_clip = Some([x0 - outset_x, y0 - outset_y, x1 + outset_x, y1 + outset_y]);
            }
            let (size, origin) = rect.map_or((node.content_size, node.content_origin), |_| {
                ([x1 - x0, y1 - y0], [x0, y0])
            });
            let offset = [
                origin[0] - node.content_origin[0] - outset_x,
                origin[1] - node.content_origin[1] - outset_y,
            ];
            let time = params.time + surface.elapsed;
            let geometry = [
                size[0] + outset_x * 2.,
                size[1] + outset_y * 2.,
                logical.style.radius,
                self.shader_scale as f32,
            ];
            if pose != surface.popup_pose {
                surface.popup_pose = pose;
                surface.painted = None;
            }
            if surface.painted == Some((geometry, offset, time)) && node.paint.widget.is_some() {
                continue;
            }
            let mut paint = PaintContext::new_clipped(
                geometry[0],
                geometry[1],
                geometry[2] as f64,
                self.shader_scale,
                tcx,
            );
            if let Some(source) = &surface.source {
                let values = if pose.popup_geometry.is_some() || pose.popup_optics.is_some() {
                    let mut values = surface.values.to_vec();
                    if let Some(optics) = pose.popup_optics {
                        match optics {
                            PopupOptics::Morph { frost, visible } => {
                                values[14] = frost;
                                values[23] = f32::from(visible);
                            }
                            PopupOptics::List { opening, radius } => {
                                values[0] = radius;
                                values[14] = opening;
                            }
                        }
                    }
                    if let Some(geometry) = pose.popup_geometry {
                        values.resize(28, 0.);
                        values.extend_from_slice(&geometry);
                    }
                    values.into()
                } else {
                    surface.values.clone()
                };
                paint.draw_shader_effect(surface.id, source.clone(), time, values);
            }
            let painted = paint.finish();
            let fragment = if offset == [0.; 2] {
                Arc::new(painted)
            } else {
                let mut fragment = Scene::new();
                fragment.append_scene(
                    painted,
                    kurbo::Affine::translate((f64::from(offset[0]), f64::from(offset[1]))),
                );
                Arc::new(fragment)
            };
            node.paint.widget = Some(fragment.clone());
            self.tree.get_node_context_mut(node.node_id).unwrap().widget = Some(fragment);
            surface.painted = Some((geometry, offset, time));
            self.scene_dirty_nodes.insert(node.node_id);
            self.stats.paint_writes += 1;
            changed = true;
        }
        self.shader_reset_time = false;
        changed
    }
}
