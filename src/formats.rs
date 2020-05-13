#[allow(unused)]
use {
    crate::{
        asset::{Asset, LeafFormat},
        Loader, PhantomContext,
    },
    alloc::vec::Vec,
};

#[cfg(feature = "serde")]
use serde::de::DeserializeOwned;

/// Format that treats bytes as JSON document and deserializes asset representation with `serde`.
#[cfg(feature = "json-format")]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct JsonFormat;

#[cfg(feature = "json-format")]
impl<A, K> LeafFormat<A, K> for JsonFormat
where
    A: Asset,
    A::Repr: DeserializeOwned,
    A::Error: From<serde_json::Error>,
{
    fn decode(self, bytes: Vec<u8>) -> Result<A::Repr, A::Error> {
        serde_json::from_slice(&bytes).map_err(Into::into)
    }
}

/// Format that treats bytes as YAML document and deserializes asset representation with `serde`.
#[cfg(feature = "yaml-format")]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct YamlFormat;

#[cfg(feature = "yaml-format")]
impl<A, K> LeafFormat<A, K> for YamlFormat
where
    A: Asset,
    A::Repr: DeserializeOwned,
    A::Error: From<serde_yaml::Error>,
{
    fn decode(self, bytes: Vec<u8>) -> Result<A::Repr, A::Error> {
        serde_yaml::from_slice(&bytes).map_err(Into::into)
    }
}

/// Format that treats bytes as RON document and deserializes asset representation with `serde`.
#[cfg(feature = "ron-format")]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RonFormat;

#[cfg(feature = "ron-format")]
impl<A, K> LeafFormat<A, K> for RonFormat
where
    A: Asset,
    A::Repr: DeserializeOwned,
    A::Error: From<ron::de::Error>,
{
    fn decode(self, bytes: Vec<u8>) -> Result<A::Repr, A::Error> {
        ron::de::from_bytes(&bytes).map_err(Into::into)
    }
}
