use crate::{
    asset::{AssetDefaultFormat, SimpleAsset},
    formats::PassthroughFormat,
};

impl SimpleAsset for Vec<u8> {
    type Error = std::convert::Infallible;
}

impl<K> AssetDefaultFormat<K> for Vec<u8> {
    type DefaultFormat = PassthroughFormat;
}
