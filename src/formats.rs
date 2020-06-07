#[allow(unused)]
use {
    crate::{
        asset::{Asset, Format},
        ready, Cache, PhantomContext, Ready,
    },
    alloc::vec::Vec,
    core::convert::Infallible,
};

#[cfg(feature = "serde")]
use serde::de::DeserializeOwned;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PassthroughFormat;

impl<A, K> Format<A, K> for PassthroughFormat
where
    A: Asset,
    A::Repr: From<Vec<u8>>,
{
    type Error = Infallible;
    type DecodeFuture = Ready<Result<A::Repr, Infallible>>;

    fn decode(self, bytes: Vec<u8>, _cache: &Cache<K>) -> Ready<Result<A::Repr, Infallible>> {
        ready(Ok(bytes.into()))
    }
}

/// Format that treats bytes as JSON document and deserializes asset representation with `serde`.
#[cfg(feature = "json-format")]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(doc, feature = "unstable-doc"), doc(cfg(feature = "json-format")))]
pub struct JsonFormat;

#[cfg(feature = "json-format")]
impl<A, K> Format<A, K> for JsonFormat
where
    A: Asset,
    A::Repr: DeserializeOwned,
{
    type Error = serde_json::Error;
    type DecodeFuture = Ready<Result<A::Repr, Self::Error>>;

    fn decode(
        self,
        bytes: Vec<u8>,
        _cache: &Cache<K>,
    ) -> Ready<Result<A::Repr, serde_json::Error>> {
        ready(serde_json::from_slice(&bytes).map_err(Into::into))
    }
}

/// Format that treats bytes as YAML document and deserializes asset representation with `serde`.
#[cfg(feature = "yaml-format")]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(doc, feature = "unstable-doc"), doc(cfg(feature = "yaml-format")))]
pub struct YamlFormat;

#[cfg(feature = "yaml-format")]
impl<A, K> Format<A, K> for YamlFormat
where
    A: Asset,
    A::Repr: DeserializeOwned,
{
    type Error = serde_yaml::Error;
    type DecodeFuture = Ready<Result<A::Repr, Self::Error>>;

    fn decode(
        self,
        bytes: Vec<u8>,
        _cache: &Cache<K>,
    ) -> Ready<Result<A::Repr, serde_yaml::Error>> {
        ready(serde_yaml::from_slice(&bytes).map_err(Into::into))
    }
}

/// Format that treats bytes as RON document and deserializes asset representation with `serde`.
#[cfg(feature = "ron-format")]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(all(doc, feature = "unstable-doc"), doc(cfg(feature = "ron-format")))]
pub struct RonFormat;

#[cfg(feature = "ron-format")]
impl<A, K> Format<A, K> for RonFormat
where
    A: Asset,
    A::Repr: DeserializeOwned,
{
    type Error = ron::de::Error;
    type DecodeFuture = Ready<Result<A::Repr, Self::Error>>;

    fn decode(self, bytes: Vec<u8>, _cache: &Cache<K>) -> Ready<Result<A::Repr, ron::de::Error>> {
        ready(ron::de::from_bytes(&bytes).map_err(Into::into))
    }
}
