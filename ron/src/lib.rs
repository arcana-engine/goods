use {goods::SimpleFormat, serde::de::DeserializeOwned};

/// Format that treats bytes as RON document and deserializes asset representation with `serde`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RonFormat;

impl<A, K> SimpleFormat<A, K> for RonFormat
where
    A: DeserializeOwned,
{
    fn decode_simple(self, _: K, bytes: Box<[u8]>) -> eyre::Result<A> {
        ron::de::from_bytes(&bytes).map_err(Into::into)
    }
}
