use {goods::SimpleFormat, serde::de::DeserializeOwned};

/// Format that treats bytes as JSON document and deserializes asset representation with `serde`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct JsonFormat;

impl<A, K> SimpleFormat<A, K> for JsonFormat
where
    A: DeserializeOwned,
{
    fn decode_simple(self, _: K, bytes: Box<[u8]>) -> eyre::Result<A> {
        serde_json::from_slice(&bytes).map_err(Into::into)
    }
}
