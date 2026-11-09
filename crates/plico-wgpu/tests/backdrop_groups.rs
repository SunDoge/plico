use kurbo::{Affine, Rect};
use peniko::{BlendMode, Color, Fill};
use plico_renderer::{PaintScene, Scene, ShaderEffectId, ShaderEffectSource, TextContext};
use plico_wgpu::Renderer;
fn effect(text: &mut TextContext, id: ShaderEffectId) -> Scene {
    let source = ShaderEffectSource::with_backdrop(
        "fn plico_effect(uv:vec2f)->vec4f { return plico_backdrop_at(uv); }",
        true,
    )
    .unwrap();
    let mut cx = plico_renderer::widget::PaintContext::new(40., 40., 1., text);
    cx.draw_shader_effect(id, source, 0., Vec::<f32>::new().into());
    cx.finish()
}
fn fill(scene: &mut Scene, rect: Rect, color: Color) {
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn opacity_groups_sample_prior_context_and_mix_premultiplied_alpha_once() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let id = ShaderEffectId::new();
    for base in [
        Color::from_rgb8(40, 80, 120),
        Color::from_rgba8(40, 80, 120, 128),
    ] {
        for alpha in [0.01, 0.25, 0.5, 0.9, 1.] {
            for nested in [false, true] {
                let mut scene = Scene::new();
                fill(&mut scene, Rect::new(0., 0., 80., 80.), base);
                scene.push_layer(
                    BlendMode::default(),
                    alpha,
                    Affine::IDENTITY,
                    &Rect::new(10., 10., 70., 70.),
                    None,
                    None,
                );
                if nested {
                    scene.push_layer(
                        BlendMode::default(),
                        0.5,
                        Affine::IDENTITY,
                        &Rect::new(20., 20., 60., 60.),
                        None,
                        None,
                    );
                }
                scene.append_scene(effect(&mut text, id), Affine::translate((20., 20.)));
                if nested {
                    scene.pop_layer();
                }
                scene.pop_layer();
                let pixels = renderer
                    .render_scene(&scene, [80, 80], [0.; 4], false)
                    .unwrap();
                // An identity backdrop adds no new content. Its group must
                // neither darken color nor increase destination alpha.
                let at = |x, y| &pixels[((y * 80 + x) * 4) as usize..][..4];
                let outside = at(5, 5);
                let center = at(40, 40);
                for (a, b) in outside.iter().zip(center) {
                    assert!(
                        a.abs_diff(*b) <= 2,
                        "base={base:?},alpha={alpha},nested={nested},outside={outside:?},center={center:?}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn grouped_effect_and_foreground_crossfade_together_and_release_contexts() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let id = ShaderEffectId::new();
    let source=ShaderEffectSource::with_backdrop("fn plico_effect(uv:vec2f)->vec4f {let c=plico_backdrop_at(uv);return vec4f(c.b,c.r,c.g,c.a);}",true).unwrap();
    let mut cx = plico_renderer::widget::PaintContext::new(40., 40., 1., &mut text);
    cx.draw_shader_effect(id, source, 0., Vec::<f32>::new().into());
    let effect = cx.finish();
    let base = || {
        let mut s = Scene::new();
        fill(
            &mut s,
            Rect::new(0., 0., 80., 80.),
            Color::from_rgba8(40, 80, 120, 128),
        );
        s
    };
    let append = |s: &mut Scene| {
        s.append_scene(effect.clone(), Affine::translate((20., 20.)));
        fill(s, Rect::new(30., 30., 40., 40.), Color::WHITE);
    };
    let before = full
        .render_scene(&base(), [80, 80], [0.; 4], false)
        .unwrap();
    let mut reference = base();
    append(&mut reference);
    let after = full
        .render_scene(&reference, [80, 80], [0.; 4], false)
        .unwrap();
    for alpha in [0.01, 0.1, 0.25, 0.5, 0.75, 0.99, 1.] {
        let mut s = base();
        s.push_layer(
            BlendMode::default(),
            alpha,
            Affine::IDENTITY,
            &Rect::new(10., 10., 70., 70.),
            None,
            None,
        );
        append(&mut s);
        s.pop_layer();
        let actual = renderer.render_scene(&s, [80, 80], [0.; 4], true).unwrap();
        for ((a, b), c) in before.iter().zip(&after).zip(&actual) {
            let expected = (*a as f32 * (1. - alpha) + *b as f32 * alpha).round() as u8;
            assert!(
                expected.abs_diff(*c) <= 2,
                "non-atomic fade alpha={alpha}: expected {expected},got {c}"
            );
        }
        assert!(renderer.layer_pool_stats().backdrop_context_bytes <= 80 * 80 * 4);
        assert!(renderer.layer_frame_bytes() < 80 * 80 * 8);
    }
    renderer
        .render_scene(&base(), [80, 80], [0.; 4], true)
        .unwrap();
    assert_eq!(renderer.layer_pool_stats().backdrop_context_bytes, 0);
}

#[test]
#[ignore = "requires a working wgpu adapter"]
fn changing_prior_and_later_ink_matches_full_replay_for_nested_groups() {
    let mut renderer = pollster::block_on(Renderer::new()).unwrap();
    let mut full = pollster::block_on(Renderer::new()).unwrap();
    let mut text = TextContext::new();
    let id = ShaderEffectId::new();
    for i in 0..12 {
        let mut s = Scene::new();
        fill(
            &mut s,
            Rect::new(0., 0., 100., 80.),
            Color::from_rgb8(20 + i * 10, 80, 120),
        );
        s.push_clip_layer(
            Affine::IDENTITY,
            &Rect::new(10., 10., 90., 70.).to_rounded_rect(8.),
        );
        s.push_layer(
            BlendMode::default(),
            0.5,
            Affine::IDENTITY,
            &Rect::new(0., 0., 100., 80.),
            None,
            None,
        );
        fill(
            &mut s,
            Rect::new(18., 18., 45., 45.),
            Color::from_rgba8(200, 20, 50, 128),
        );
        s.push_layer(
            BlendMode::default(),
            0.4,
            Affine::IDENTITY,
            &Rect::new(20., 20., 60., 60.),
            None,
            None,
        );
        s.append_scene(effect(&mut text, id), Affine::translate((20., 20.)));
        s.pop_layer();
        s.pop_layer();
        s.pop_layer();
        fill(
            &mut s,
            Rect::new(40., 0., 48., 80.),
            if i % 2 == 0 {
                Color::WHITE
            } else {
                Color::BLACK
            },
        );
        let actual = renderer.render_scene(&s, [100, 80], [0.; 4], true).unwrap();
        let expected = full.render_scene(&s, [100, 80], [0.; 4], false).unwrap();
        assert_eq!(actual, expected, "paint order / partial replay frame {i}");
    }
}
