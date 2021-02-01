use {
    crate::{
        asset::SimpleAsset,
        format::{AssetDefaultFormat, Format, PassthroughFormat},
    },
    std::sync::Arc,
};

impl SimpleAsset for Vec<u8> {}

impl<K> AssetDefaultFormat<K> for Vec<u8>
where
    PassthroughFormat: Format<Self, K>,
{
    type DefaultFormat = PassthroughFormat;
}

impl SimpleAsset for Box<[u8]> {}

impl<K> AssetDefaultFormat<K> for Box<[u8]>
where
    PassthroughFormat: Format<Self, K>,
{
    type DefaultFormat = PassthroughFormat;
}

impl SimpleAsset for Arc<[u8]> {}

impl<K> AssetDefaultFormat<K> for Arc<[u8]>
where
    PassthroughFormat: Format<Self, K>,
{
    type DefaultFormat = PassthroughFormat;
}
