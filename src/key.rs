use {
    super::asset::Asset,
    std::{
        any::TypeId,
        hash::{Hash, Hasher},
    },
    uuid::Uuid,
};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Key {
    type_id: TypeId,
    uuid: Uuid,
}

impl Key {
    pub fn new<A: Asset>(uuid: Uuid) -> Self {
        Key {
            type_id: TypeId::of::<A>(),
            uuid,
        }
    }

    pub fn eq_key<A: Asset>(&self, uuid: &Uuid) -> bool {
        self.type_id == TypeId::of::<A>() && self.uuid == *uuid
    }
}

pub fn hash_key<A, H>(uuid: &Uuid, state: &mut H)
where
    A: Asset,
    H: Hasher,
{
    TypeId::of::<A>().hash(state);
    uuid.hash(state);
}
