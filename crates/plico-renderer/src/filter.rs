//! Backend-neutral interpretation of supported shared filter graphs.
/// How a sampled image extends beyond its bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BlurEdge {
    /// Transparent black.
    Transparent = 0,
    /// Repeat the closest edge pixel.
    Clamp = 1,
    /// Tile the image.
    Repeat = 2,
    /// Mirror each repeated tile.
    Mirror = 3,
}
/// A single isotropic Gaussian operation; graph interpretation stays shared.
#[derive(Clone, Copy, Debug)]
pub struct GaussianBlur {
    /// Standard deviation in local coordinates.
    pub sigma: f32,
    /// Sampling outside the source image.
    pub edge: BlurEdge,
}
impl GaussianBlur {
    /// Encode this operation into the shared scene filter representation.
    pub fn into_filter(self) -> vello_common::filter_effects::Filter {
        use vello_common::filter_effects::{EdgeMode, Filter, FilterPrimitive};
        Filter::from_primitive(FilterPrimitive::GaussianBlur {
            std_deviation: self.sigma,
            edge_mode: match self.edge {
                BlurEdge::Transparent => EdgeMode::None,
                BlurEdge::Clamp => EdgeMode::Duplicate,
                BlurEdge::Repeat => EdgeMode::Wrap,
                BlurEdge::Mirror => EdgeMode::Mirror,
            },
        })
    }

    /// Return None for unsupported graphs or invalid sigma; never drop operations.
    pub fn from_filter(filter: &vello_common::filter_effects::Filter) -> Option<Self> {
        use vello_common::filter_effects::{EdgeMode, FilterPrimitive};
        if filter.graph.output.0 != 0 || filter.graph.primitives.len() != 1 {
            return None;
        }
        let FilterPrimitive::GaussianBlur {
            std_deviation,
            edge_mode,
        } = filter.graph.primitives[0]
        else {
            return None;
        };
        if !std_deviation.is_finite() || std_deviation < 0. {
            return None;
        }
        Some(Self {
            sigma: std_deviation,
            edge: match edge_mode {
                EdgeMode::None => BlurEdge::Transparent,
                EdgeMode::Duplicate => BlurEdge::Clamp,
                EdgeMode::Wrap => BlurEdge::Repeat,
                EdgeMode::Mirror => BlurEdge::Mirror,
            },
        })
    }
}
