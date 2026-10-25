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

const SESSION_PREFIX: &[u8] = b"PLICO-STATE\0\x01";
#[derive(Serialize, Deserialize)]
struct SessionSnapshot {
    app: Vec<u8>,
    storage: plico_core::StateStorageSnapshot,
}
impl StateSnapshot {
    pub fn encode<T: Serialize + ?Sized>(state: &T) -> Result<Self, rmp_serde::encode::Error> {
        rmp_serde::to_vec_named(state).map(Self)
    }

    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, rmp_serde::decode::Error> {
        if let Some(bytes) = self.0.strip_prefix(SESSION_PREFIX) {
            let session: SessionSnapshot = rmp_serde::from_slice(bytes)?;
            rmp_serde::from_slice(&session.app)
        } else {
            rmp_serde::from_slice(&self.0)
        }
    }

    pub(crate) fn encode_session<T: Serialize + ?Sized>(
        state: &T,
        storage: plico_core::StateStorageSnapshot,
    ) -> Result<Self, rmp_serde::encode::Error> {
        if storage.is_empty() {
            return Self::encode(state);
        }
        let session = SessionSnapshot {
            app: rmp_serde::to_vec_named(state)?,
            storage,
        };
        let mut bytes = SESSION_PREFIX.to_vec();
        bytes.extend(rmp_serde::to_vec_named(&session)?);
        Ok(Self(bytes))
    }
    pub(crate) fn persisted(
        &self,
    ) -> Result<Option<plico_core::StateStorageSnapshot>, rmp_serde::decode::Error> {
        self.0
            .strip_prefix(SESSION_PREFIX)
            .map(|bytes| {
                rmp_serde::from_slice::<SessionSnapshot>(bytes).map(|session| session.storage)
            })
            .transpose()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
