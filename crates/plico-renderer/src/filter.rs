//! Backend-neutral shared filter description.
//!
//! The shared `Filter` is a single-primitive graph: an isolated layer's
//! supported filters are exactly one Gaussian blur (see `GaussianBlur`).
//! Backends and platforms read the same value; unsupported graphs are
//! rejected, never approximated.
/// How a sampled image extends beyond its bounds (SVG edge modes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeMode {
    /// Extend by transparent black.
    None,
    /// Extend by duplicating edge pixels (clamp to edge).
    Duplicate,
    /// Extend by wrapping to the opposite edge (repeat/tile).
    Wrap,
    /// Extend by reflecting each repeated tile.
    Mirror,
}
impl Default for EdgeMode {
    fn default() -> Self {
        Self::None
    }
}
/// A supported filter primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterPrimitive {
    /// Isotropic Gaussian blur; deviation is in local coordinates.
    GaussianBlur {
        /// Standard deviation of the blur.
        std_deviation: f32,
        /// How the sampled source extends beyond its bounds.
        edge_mode: EdgeMode,
    },
}
/// The shared filter description carried by isolated layers.
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    /// The single primitive this filter applies, or None for the identity.
    primitive: Option<FilterPrimitive>,
}
impl Filter {
    /// A filter applying exactly one primitive.
    pub fn from_primitive(primitive: FilterPrimitive) -> Self {
        Self {
            primitive: Some(primitive),
        }
    }
    /// The identity filter.
    pub fn identity() -> Self {
        Self { primitive: None }
    }
    /// The primitive this filter applies, when it is exactly one.
    pub fn single(&self) -> Option<&FilterPrimitive> {
        self.primitive.as_ref()
    }
}

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
    pub fn into_filter(self) -> Filter {
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

    /// Return None for unsupported filters or invalid sigma; never drop operations.
    pub fn from_filter(filter: &Filter) -> Option<Self> {
        let FilterPrimitive::GaussianBlur {
            std_deviation,
            edge_mode,
        } = filter.single()?
        else {
            return None;
        };
        if !std_deviation.is_finite() || *std_deviation < 0. || *std_deviation > 64. {
            return None;
        }
        Some(Self {
            sigma: *std_deviation,
            edge: match edge_mode {
                EdgeMode::None => BlurEdge::Transparent,
                EdgeMode::Duplicate => BlurEdge::Clamp,
                EdgeMode::Wrap => BlurEdge::Repeat,
                EdgeMode::Mirror => BlurEdge::Mirror,
            },
        })
    }
}
