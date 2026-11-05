//! Retained validated GPU surfaces, driven by the injected UI clock.
use super::*;
use plico::ShaderParams;
use plico_renderer::{ShaderEffectId, ShaderEffectSource, widget::PaintContext};
use std::time::Duration;

pub(super) struct Surface {
    id: ShaderEffectId,
    authored: String,
    source: Option<ShaderEffectSource>,
    error: Option<String>,
    params: ShaderParams,
    values: Arc<[f32]>,
    elapsed: f32,
    sampled: Duration,
    painted: Option<([f32; 4], f32)>,
}
impl Surface {
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
        self.shader_motion_enabled && self.shaders.values().any(Surface::running)
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
                sampled: now,
                painted: None,
            });
            if self.shader_motion_enabled && !self.shader_reset_time && surface.running() {
                surface.elapsed +=
                    now.saturating_sub(surface.sampled).as_secs_f32() * surface.params.speed;
            }
            surface.sampled = now;
            let program_changed = surface.authored != logical.text
                || surface.params.source_path != params.source_path
                || surface.params.module != params.module
                || surface.params.sksl != params.sksl
                || surface.source.is_none() && surface.error.is_none();
            let values_changed = surface.params.values != params.values
                || surface.params.parameters != params.parameters;
            if program_changed || values_changed {
                let result = if program_changed || surface.error.is_some() {
                    if params.module {
                        ShaderEffectSource::new_module(logical.text)
                    } else {
                        ShaderEffectSource::new(logical.text)
                    }
                    .and_then(|source| match &params.sksl {
                        Some(sksl) => source.with_sksl(sksl),
                        None => Ok(source),
                    })
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
                    Ok((source, values))
                });
                match result {
                    Ok((source, values)) => {
                        surface.source = Some(source);
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
            let time = params.time + surface.elapsed;
            let geometry = [
                node.content_size[0],
                node.content_size[1],
                logical.style.radius,
                self.shader_scale as f32,
            ];
            if surface.painted == Some((geometry, time)) && node.paint.widget.is_some() {
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
                paint.draw_shader_effect(surface.id, source.clone(), time, surface.values.clone());
            }
            let fragment = Arc::new(paint.finish());
            node.paint.widget = Some(fragment.clone());
            self.tree.get_node_context_mut(node.node_id).unwrap().widget = Some(fragment);
            surface.painted = Some((geometry, time));
            self.stats.paint_writes += 1;
            changed = true;
        }
        self.shader_reset_time = false;
        changed
    }
}
