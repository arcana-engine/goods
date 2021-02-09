use {
    crate::{
        asset::SimpleAsset,
        format::{AssetDefaultFormat, PassthroughFormat},
    },
    std::sync::Arc,
};

impl SimpleAsset for Vec<u8> {}

impl AssetDefaultFormat for Vec<u8> {
    type DefaultFormat = PassthroughFormat;
}

impl SimpleAsset for Box<[u8]> {}

impl AssetDefaultFormat for Box<[u8]> {
    type DefaultFormat = PassthroughFormat;
}

impl SimpleAsset for Arc<[u8]> {}

impl AssetDefaultFormat for Arc<[u8]> {
    type DefaultFormat = PassthroughFormat;
}
