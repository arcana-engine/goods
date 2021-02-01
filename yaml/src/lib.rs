use {goods::SimpleFormat, serde::de::DeserializeOwned};

/// Format that treats bytes as YAML document and deserializes asset representation with `serde`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct YamlFormat;

impl<A, K> SimpleFormat<A, K> for YamlFormat
where
    A: DeserializeOwned,
{
    fn decode_simple(self, _key: K, bytes: Box<[u8]>) -> eyre::Result<A> {
        serde_yaml::from_slice(&bytes).map_err(Into::into)
    }
}
