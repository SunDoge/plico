//! Overlay scrollbar geometry shared by scene painting and native hit testing.

#![warn(missing_docs)]

use kurbo::{Point, Rect};

use crate::layout::PlacedNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Scroll axis addressed by overlay-scrollbar interaction.
pub enum ScrollAxis {
    /// Left-to-right axis.
    Horizontal,
    /// Top-to-bottom axis.
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Hit region within one overlay scrollbar.
pub enum ScrollbarPart {
    /// Draggable thumb.
    Thumb,
    /// Track before the thumb, normally interpreted as page backward.
    TrackBefore,
    /// Track after the thumb, normally interpreted as page forward.
    TrackAfter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Scrollbar axis and part selected by hit testing.
pub struct ScrollbarTarget {
    /// Selected scroll axis.
    pub axis: ScrollAxis,
    /// Selected track/thumb region.
    pub part: ScrollbarPart,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Shared resolved geometry consumed by painting and interaction.
pub struct ResolvedScrollbarGeometry {
    /// Complete track rectangle in logical window coordinates.
    pub track: Rect,
    /// Draggable thumb rectangle in logical window coordinates.
    pub thumb: Rect,
}

/// Resolve viewport-dependent scrollbar dimensions once so paint, hit testing
/// and dragging cannot disagree for extreme custom values.
pub fn resolve(node: &PlacedNode, axis: ScrollAxis) -> Option<ResolvedScrollbarGeometry> {
    let index = usize::from(axis == ScrollAxis::Vertical);
    if node.scroll.range[index] <= 0.5 || !node.scroll.scrollable[index] {
        return None;
    }
    let [x0, y0, x1, y1] = node.scroll.port;
    let port = Rect::new(x0.into(), y0.into(), x1.into(), y1.into());
    let main = match axis {
        ScrollAxis::Horizontal => port.width(),
        ScrollAxis::Vertical => port.height(),
    }
    .max(0.0);
    let cross = match axis {
        ScrollAxis::Horizontal => port.height(),
        ScrollAxis::Vertical => port.width(),
    }
    .max(0.0);
    if main <= 0.0 || cross <= 0.0 {
        return None;
    }
    let style = node.paint.scrollbar_style();
    let track_thickness = f64::from(style.track_thickness).clamp(0.0, cross);
    let thumb_thickness = f64::from(style.thumb_thickness).clamp(0.0, track_thickness);
    let cross_margin = (track_thickness - thumb_thickness) * 0.5;
    let end_margin = f64::from(style.end_margin).clamp(0.0, main * 0.5);
    let track = match axis {
        ScrollAxis::Horizontal => Rect::new(port.x0, port.y1 - track_thickness, port.x1, port.y1),
        ScrollAxis::Vertical => Rect::new(port.x1 - track_thickness, port.y0, port.x1, port.y1),
    };
    let range = f64::from(node.scroll.range[index]);
    let inner = (main - 2.0 * end_margin).max(0.0);
    let length = (inner * main / (main + range))
        .max(f64::from(style.min_thumb_length))
        .clamp(0.0, inner);
    let progress = (f64::from(node.scroll.offset[index]) / range).clamp(0.0, 1.0);
    let start = end_margin + progress * (inner - length);
    let thumb = match axis {
        ScrollAxis::Horizontal => Rect::new(
            port.x0 + start,
            track.y0 + cross_margin,
            port.x0 + start + length,
            track.y1 - cross_margin,
        ),
        ScrollAxis::Vertical => Rect::new(
            track.x0 + cross_margin,
            port.y0 + start,
            track.x1 - cross_margin,
            port.y0 + start + length,
        ),
    };
    Some(ResolvedScrollbarGeometry { track, thumb })
}

/// Resolve the track rectangle for a scrollable axis.
pub fn track(node: &PlacedNode, axis: ScrollAxis) -> Option<Rect> {
    resolve(node, axis).map(|geometry| geometry.track)
}

/// Resolve the thumb rectangle for a scrollable axis.
pub fn thumb(node: &PlacedNode, axis: ScrollAxis) -> Option<Rect> {
    resolve(node, axis).map(|geometry| geometry.thumb)
}

/// Hit-test visible scrollbar parts, preferring vertical at an overlap corner.
pub fn hit(node: &PlacedNode, point: Point) -> Option<ScrollbarTarget> {
    if node.scroll.opacity <= 0. {
        return None;
    }
    for axis in [ScrollAxis::Vertical, ScrollAxis::Horizontal] {
        let Some(thumb) = thumb(node, axis) else {
            continue;
        };
        if thumb.contains(point) {
            return Some(ScrollbarTarget {
                axis,
                part: ScrollbarPart::Thumb,
            });
        }
        if track(node, axis).is_some_and(|track| track.contains(point)) {
            let before = match axis {
                ScrollAxis::Horizontal => point.x < thumb.x0,
                ScrollAxis::Vertical => point.y < thumb.y0,
            };
            return Some(ScrollbarTarget {
                axis,
                part: if before {
                    ScrollbarPart::TrackBefore
                } else {
                    ScrollbarPart::TrackAfter
                },
            });
        }
    }
    None
}

/// Convert logical thumb movement into content offset movement.
///
/// Returns zero when no draggable track remains.
pub fn drag_ratio(node: &PlacedNode, axis: ScrollAxis) -> f64 {
    let Some(thumb) = thumb(node, axis) else {
        return 0.0;
    };
    let index = usize::from(axis == ScrollAxis::Vertical);
    let viewport = match axis {
        ScrollAxis::Horizontal => node.scroll.port[2] - node.scroll.port[0],
        ScrollAxis::Vertical => node.scroll.port[3] - node.scroll.port[1],
    } as f64;
    let length = match axis {
        ScrollAxis::Horizontal => thumb.width(),
        ScrollAxis::Vertical => thumb.height(),
    };
    let style = node.paint.scrollbar_style();
    let end_margin = f64::from(style.end_margin).clamp(0.0, viewport * 0.5);
    let play = viewport - 2.0 * end_margin - length;
    if play > 0.0 {
        f64::from(node.scroll.range[index]) / play
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::style::{Paint, ScrollbarStyle};

    fn set_style(node: &mut PlacedNode, update: impl FnOnce(&mut ScrollbarStyle)) {
        let mut style = node.paint.scrollbar_style();
        update(&mut style);
        node.paint.scrollbar = Some(Arc::new(style));
    }

    fn node(offset: f32) -> PlacedNode {
        PlacedNode {
            node_id: taffy::tree::NodeId::from(0_u64),
            parent_node_id: None,
            depth: 0,
            world_transform: kurbo::Affine::IDENTITY,
            world_opacity: 1.0,
            rect: [0.0, 0.0, 100.0, 100.0],
            content_origin: [0.0, 0.0],
            content_size: [100.0, 100.0],
            clip: None,
            clip_radius: 0.0,
            clip_radii: crate::style::CornerRadii::default(),
            clip_depth: None,
            own_clip: Some([0.0, 0.0, 100.0, 100.0]),
            own_clip_radius: 0.0,
            own_clip_radii: crate::style::CornerRadii::default(),
            border_widths: [0.0; 4],
            scroll: crate::layout::ScrollMetrics {
                port: [0.0, 0.0, 100.0, 100.0],
                scrollable: [false, true],
                range: [0.0, 900.0],
                offset: [0.0, offset],
                opacity: 1.0,
                interaction: 0,
            },
            paint: Paint::default(),
        }
    }

    #[test]
    fn invisible_overlay_keeps_geometry_but_cannot_capture_pointer() {
        let mut node = node(0.);
        let thumb = thumb(&node, ScrollAxis::Vertical).unwrap();
        node.scroll.opacity = 0.;
        assert!(hit(&node, thumb.center()).is_none());
        assert!(
            track(&node, ScrollAxis::Vertical).is_some(),
            "edge hover can still reveal an invisible bar"
        );
        node.scroll.opacity = 0.1;
        assert_eq!(
            hit(&node, thumb.center()).unwrap().part,
            ScrollbarPart::Thumb
        );
    }

    #[test]
    fn blitz_geometry_maps_half_scroll_to_half_track() {
        let node = node(450.0);
        assert_eq!(
            thumb(&node, ScrollAxis::Vertical),
            Some(Rect::new(91.0, 38.0, 97.0, 62.0))
        );
        assert!((drag_ratio(&node, ScrollAxis::Vertical) - 900.0 / 72.0).abs() < 0.001);
        assert_eq!(
            hit(&node, Point::new(95.0, 50.0)),
            Some(ScrollbarTarget {
                axis: ScrollAxis::Vertical,
                part: ScrollbarPart::Thumb
            })
        );
    }

    #[test]
    fn custom_appearance_changes_shared_paint_and_hit_geometry() {
        let mut node = node(450.0);
        set_style(&mut node, |style| {
            style.track_thickness = 14.0;
            style.thumb_thickness = 8.0;
            style.end_margin = 3.0;
            style.min_thumb_length = 40.0;
        });
        assert_eq!(
            thumb(&node, ScrollAxis::Vertical),
            Some(Rect::new(89.0, 30.0, 97.0, 70.0))
        );
        assert_eq!(
            hit(&node, Point::new(90.0, 90.0)),
            Some(ScrollbarTarget {
                axis: ScrollAxis::Vertical,
                part: ScrollbarPart::TrackAfter,
            })
        );
    }

    #[test]
    fn extreme_custom_dimensions_are_clamped_to_the_scroll_port() {
        let mut node = node(450.0);
        set_style(&mut node, |style| {
            style.track_thickness = 500.0;
            style.thumb_thickness = 500.0;
            style.end_margin = 500.0;
            style.min_thumb_length = 500.0;
        });
        let geometry = resolve(&node, ScrollAxis::Vertical).unwrap();
        assert_eq!(geometry.track, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(geometry.thumb, Rect::new(0.0, 50.0, 100.0, 50.0));
        assert_eq!(drag_ratio(&node, ScrollAxis::Vertical), 0.0);
    }
}
