use crate::{Key, Kind, Style};
/// Portable logical identity; never serialize arena handles across reloads.
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "archive", rkyv(
 serialize_bounds(__S: rkyv::ser::Writer + rkyv::ser::Allocator, __S::Error: rkyv::rancor::Source),
 deserialize_bounds(__D::Error: rkyv::rancor::Source),
 bytecheck(bounds(__C: rkyv::validation::ArchiveContext, __C::Error: rkyv::rancor::Source))
))]
pub enum PathKey {
    Named(String),
    Number(u64),
    Auto(u32),
    Generated(u64),
    Scoped {
        scope: u64,
        #[cfg_attr(feature = "archive", rkyv(omit_bounds))]
        key: Box<PathKey>,
    },
}
impl From<Key> for PathKey {
    fn from(key: Key) -> Self {
        match key {
            Key::Static(s) => Self::Named(s.into()),
            Key::Number(n) => Self::Number(n),
            Key::Auto(n) => Self::Auto(n),
            Key::Generated(n) => Self::Generated(n),
        }
    }
}
impl PathKey {
    pub(crate) fn scoped(key: Key, scope: u64) -> Self {
        if scope == 0 {
            key.into()
        } else {
            Self::Scoped {
                scope,
                key: Box::new(key.into()),
            }
        }
    }
    pub(crate) fn matches_scoped(&self, key: Key, scope: u64) -> bool {
        match self {
            Self::Scoped {
                scope: expected,
                key: local,
            } => *expected == scope && local.matches(key),
            _ => scope == 0 && self.matches(key),
        }
    }

    pub(crate) fn matches(&self, key: Key) -> bool {
        match (self, key) {
            (Self::Named(a), Key::Static(b)) => a == b,
            (Self::Number(a), Key::Number(b)) => *a == b,
            (Self::Auto(a), Key::Auto(b)) => *a == b,
            (Self::Generated(a), Key::Generated(b)) => *a == b,
            _ => false,
        }
    }
}
#[cfg_attr(
    feature = "archive",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "archive", rkyv(
 serialize_bounds(__S: rkyv::ser::Writer + rkyv::ser::Allocator, __S::Error: rkyv::rancor::Source),
 deserialize_bounds(__D::Error: rkyv::rancor::Source),
 bytecheck(bounds(__C: rkyv::validation::ArchiveContext, __C::Error: rkyv::rancor::Source))
))]
pub struct ViewNode {
    pub key: PathKey,
    pub kind: Kind,
    pub style: Style,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<crate::TextSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<crate::GridTemplate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shader: Option<crate::ShaderParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numeric: Option<crate::NumericRange>,
    #[cfg_attr(feature = "archive", rkyv(omit_bounds))]
    pub children: Vec<ViewNode>,
}
