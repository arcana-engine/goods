use std::sync::Arc;

use crate::AssetId;

use {
    super::asset::Asset,
    std::{
        any::TypeId,
        hash::{Hash, Hasher},
    },
};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct IdKey {
    pub type_id: TypeId,
    pub id: AssetId,
}

impl IdKey {
    pub fn new<A: Asset>(asset: AssetId) -> Self {
        IdKey {
            type_id: TypeId::of::<A>(),
            id: asset,
        }
    }

    pub fn eq_key<A: Asset>(&self, asset: AssetId) -> bool {
        self.type_id == TypeId::of::<A>() && self.id == asset
    }
}

pub fn hash_id_key<A, H>(id: AssetId, state: &mut H)
where
    A: Asset,
    H: Hasher,
{
    TypeId::of::<A>().hash(state);
    id.hash(state);
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PathKey {
    pub type_id: TypeId,
    pub path: Arc<str>,
}

impl PathKey {
    pub fn new<A: Asset>(asset: Arc<str>) -> Self {
        PathKey {
            type_id: TypeId::of::<A>(),
            path: asset,
        }
    }

    pub fn eq_key<A: Asset>(&self, asset: &str) -> bool {
        self.type_id == TypeId::of::<A>() && *self.path == *asset
    }
}

pub fn hash_path_key<A, H>(path: &str, state: &mut H)
where
    A: Asset,
    H: Hasher,
{
    TypeId::of::<A>().hash(state);
    path.hash(state);
}
