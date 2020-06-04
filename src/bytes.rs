use {
    crate::{
        asset::{AssetDefaultFormat, SimpleAsset},
        formats::PassthroughFormat,
    },
    alloc::{boxed::Box, vec::Vec},
};

impl SimpleAsset for Vec<u8> {}

impl<K> AssetDefaultFormat<K> for Vec<u8> {
    type DefaultFormat = PassthroughFormat;
}

impl SimpleAsset for Box<[u8]> {}

impl<K> AssetDefaultFormat<K> for Box<[u8]> {
    type DefaultFormat = PassthroughFormat;
}
