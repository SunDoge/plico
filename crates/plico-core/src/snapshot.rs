use crate::{Key, Kind, Style};
/// Portable logical identity; never serialize arena handles across reloads.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PathKey {
    Named(String),
    Number(u64),
    Auto(u32),
    Generated(u64),
    Scoped { scope: u64, key: Box<PathKey> },
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    pub children: Vec<ViewNode>,
}
