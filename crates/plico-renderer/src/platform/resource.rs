//! Typed generational handles for native shell resources.

#![warn(missing_docs)]

use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

const fn valid_resource_key_parts(lo: u32, hi: u32) -> bool {
    lo != 0 && hi != 0 && hi % 2 == 1
}

/// Full-width generational identity for one family of native resources.
///
/// The family is a Rust-only brand. Every wire boundary uses the stable
/// `{ lo, hi }` representation without truncating a `u64`.
pub struct ResourceKey<Family> {
    lo: u32,
    hi: u32,
    family: PhantomData<fn() -> Family>,
}

impl<Family> Copy for ResourceKey<Family> {}
impl<Family> Clone for ResourceKey<Family> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Family> PartialEq for ResourceKey<Family> {
    fn eq(&self, other: &Self) -> bool {
        self.lo == other.lo && self.hi == other.hi
    }
}
impl<Family> Eq for ResourceKey<Family> {}
impl<Family> std::hash::Hash for ResourceKey<Family> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.lo, state);
        std::hash::Hash::hash(&self.hi, state);
    }
}
impl<Family> std::fmt::Debug for ResourceKey<Family> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResourceKey")
            .field("lo", &self.lo)
            .field("hi", &self.hi)
            .finish()
    }
}

impl<Family> ResourceKey<Family> {
    /// Construct a validated key from its complete wire representation.
    pub const fn from_parts(lo: u32, hi: u32) -> Option<Self> {
        if !valid_resource_key_parts(lo, hi) {
            return None;
        }
        Some(Self {
            lo,
            hi,
            family: PhantomData,
        })
    }

    /// Reconstruct and validate a key packed in SlotMap's FFI representation.
    pub const fn from_ffi(value: u64) -> Option<Self> {
        Self::from_parts(value as u32, (value >> 32) as u32)
    }

    /// Low 32 bits: the non-zero SlotMap slot index.
    pub const fn lo(self) -> u32 {
        self.lo
    }

    /// High 32 bits: the non-zero odd SlotMap generation.
    pub const fn hi(self) -> u32 {
        self.hi
    }

    /// Convert to the compact representation used only inside Rust.
    pub const fn as_ffi(self) -> u64 {
        self.lo as u64 | ((self.hi as u64) << 32)
    }

    /// Return the explicit wire pair.
    pub const fn into_parts(self) -> (u32, u32) {
        (self.lo, self.hi)
    }
}

impl<Family> Serialize for ResourceKey<Family> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct WireKey {
            lo: u32,
            hi: u32,
        }

        WireKey {
            lo: self.lo,
            hi: self.hi,
        }
        .serialize(serializer)
    }
}

impl<'de, Family> Deserialize<'de> for ResourceKey<Family> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireKey {
            lo: u32,
            hi: u32,
        }

        let wire = WireKey::deserialize(deserializer)?;
        Self::from_parts(wire.lo, wire.hi).ok_or_else(|| {
            de::Error::custom("resource key lo must be non-zero and hi must be non-zero and odd")
        })
    }
}

impl<Family> std::fmt::Display for ResourceKey<Family> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}v{}", self.lo, self.hi)
    }
}

#[cfg(test)]
mod resource_key_tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    enum ImageResource {}
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    enum FontResource {}

    #[test]
    fn resource_keys_share_one_validated_wire_shape_without_losing_the_family() {
        let image = ResourceKey::<ImageResource>::from_parts(7, 3).unwrap();
        let encoded = serde_json::to_string(&image).unwrap();
        let decoded: ResourceKey<ImageResource> = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.into_parts(), (7, 3));
        assert_eq!(
            ResourceKey::<ImageResource>::from_ffi(image.as_ffi()),
            Some(image)
        );
        assert_eq!(image.to_string(), "7v3");
        assert!(ResourceKey::<FontResource>::from_parts(7, 2).is_none());
        // An image key cannot be passed where ResourceKey<FontResource> is
        // required; resource families remain distinct before serialization.
    }
}
