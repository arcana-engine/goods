use {
    crate::{
        asset::{AssetDefaultFormat, SimpleAsset},
        formats::PassthroughFormat,
    },
    alloc::{boxed::Box, sync::Arc, vec::Vec},
};

impl SimpleAsset for Vec<u8> {}

impl<K> AssetDefaultFormat<K> for Vec<u8> {
    type DefaultFormat = PassthroughFormat;
}

impl SimpleAsset for Box<[u8]> {}

impl<K> AssetDefaultFormat<K> for Box<[u8]> {
    type DefaultFormat = PassthroughFormat;
}

impl SimpleAsset for Arc<[u8]> {}

impl<K> AssetDefaultFormat<K> for Arc<[u8]> {
    type DefaultFormat = PassthroughFormat;
}
