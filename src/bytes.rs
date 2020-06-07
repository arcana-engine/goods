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

impl SimpleAsset for alloc::sync::Arc<[u8]> {}

impl<K> AssetDefaultFormat<K> for alloc::sync::Arc<[u8]> {
    type DefaultFormat = PassthroughFormat;
}

#[cfg(not(feature = "sync"))]
impl SimpleAsset for alloc::rc::Rc<[u8]> {}

#[cfg(not(feature = "sync"))]
impl<K> AssetDefaultFormat<K> for alloc::rc::Rc<[u8]> {
    type DefaultFormat = PassthroughFormat;
}
