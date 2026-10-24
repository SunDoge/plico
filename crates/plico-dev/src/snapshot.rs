use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Opaque, named-field MessagePack checkpoint. Empty means no checkpoint.
///
/// Field names allow Serde defaults and unknown-field handling across reloads;
/// changes to field types still require migration or an application reset.
/// The outer JSON transport represents these bytes as an array, never as a
/// JSON value representation of the application state.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StateSnapshot(Vec<u8>);

impl StateSnapshot {
    pub fn encode<T: Serialize + ?Sized>(state: &T) -> Result<Self, rmp_serde::encode::Error> {
        rmp_serde::to_vec_named(state).map(Self)
    }

    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, rmp_serde::decode::Error> {
        rmp_serde::from_slice(&self.0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
