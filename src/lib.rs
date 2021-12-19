//! Asset loader.
//!
//! # Asset and AssetField derive macros
//!
//! Creates structures to act as two loading stages of asset and implement asset using those.
//! First stages must be deserializable with serde.
//! All fields with `#[external]` must implement `AssetField<External>`. Which has blanket impl for `Asset` implementors and some wrappers, like `Option<A>` and `Arc<[A]>` where `A: Asset`.
//! All fields with `#[container]` attribute must implement `AssetField<Container>`. It can be derived using `derive(AssetField)`. They can in turn contain fields with `#[external]` and `#[container]` attributes. Also implemented for wrappers like `Option<A>` and `Arc<[A]>`.
//! All fields without special attributes of the target struct must implement `DeserializeOwned`.
//! All fields transiently with #[external] attribute will be replaced with id for first stage struct and `AssetResult`s for second stage.
//! Second stages will have `AssetResult`s fields in place of the assets.
//!
//! # Example
//!
//! ```
//!
//! # use goods::*;
//!
//! /// Simple deserializable type. Included as-is into generated types for `#[derive(Asset)]` and #[derive(AssetField)].
//! #[derive(Clone, serde::Deserialize)]
//! struct Foo;
//!
//! /// Trivial asset type.
//! #[derive(Clone, Asset)]
//! struct Bar;
//!
//! /// Asset field type. `AssetField<Container>` implementation is generated, but not `Asset` implementation.
//! /// Fields of types with `#[derive(AssetField)]` attribute are not replaced by uuids as external assets.
//! #[derive(Clone, AssetField)]
//! struct Baz;
//!
//! /// Asset structure. Implements Asset trait using
//! /// two generated structures are intermediate phases.
//! #[derive(Clone, Asset)]
//! struct AssetStruct {
//!     /// Deserializable types are inlined into asset as is.
//!     foo: Foo,
//!
//!     /// `AssetField<External>` is implemented for all `Asset` implementors.
//!     /// Deserialized as `AssetId` and loaded recursively.
//!     #[external]
//!     bar: Bar,
//!
//!     /// Container fields are deserialized similar to types that derive `Asset`.
//!     /// If there is no external asset somewhere in hierarchy, decoded `Baz` is structurally equivalent to `Baz`.
//!     #[container]
//!     baz: Baz,
//! }
//! ```

mod asset;
mod field;
mod key;
mod loader;
pub mod source;

use std::{
    borrow::Borrow,
    fmt::{self, Debug, Display, LowerHex, UpperHex},
    marker::PhantomData,
    num::NonZeroU64,
};

pub use self::{
    asset::{Asset, AssetBuild, SimpleAsset, TrivialAsset},
    field::{AssetField, AssetFieldBuild, Container, External},
    loader::{AssetHandle, AssetResult, AssetResultPoisoned, Error, Loader, LoaderBuilder},
};
pub use goods_proc::{Asset, AssetField};

// Used by generated code.
#[doc(hidden)]
pub use {bincode, serde, serde_json, std::convert::Infallible, thiserror};

#[derive(Debug, thiserror::Error)]
#[error("Not found")]
struct NotFound;

/// Type for unique asset identification.
/// There are 2^64-1 valid values of this type that should be enough for now.
///
/// Using `NonZero` makes `Option<AssetId>` same size as `AssetId` which is good for performance.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
#[repr(transparent)]
pub struct AssetId(pub NonZeroU64);

impl AssetId {
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            None => None,
            Some(value) => Some(AssetId(value)),
        }
    }
}

impl Debug for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        LowerHex::fmt(&self.0.get(), f)
    }
}

impl Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        LowerHex::fmt(&self.0.get(), f)
    }
}

impl LowerHex for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        LowerHex::fmt(&self.0.get(), f)
    }
}

impl UpperHex for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        UpperHex::fmt(&self.0.get(), f)
    }
}

/// `AssetId` augmented with type information, specifying which asset type is referenced.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
#[repr(transparent)]
pub struct TypedAssetId<A> {
    pub id: AssetId,
    pub marker: PhantomData<fn() -> A>,
}

impl<A> Borrow<AssetId> for TypedAssetId<A> {
    fn borrow(&self) -> &AssetId {
        &self.id
    }
}

impl<A> Debug for TypedAssetId<A>
where
    A: Asset,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{}({:#?})", A::name(), self.id)
        } else {
            write!(f, "{}({:?})", A::name(), self.id)
        }
    }
}

impl<A> Display for TypedAssetId<A>
where
    A: Asset,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{}({:#})", A::name(), self.id)
        } else {
            write!(f, "{}({:})", A::name(), self.id)
        }
    }
}

impl<A> LowerHex for TypedAssetId<A>
where
    A: Asset,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{}({:#x})", A::name(), self.id)
        } else {
            write!(f, "{}({:x})", A::name(), self.id)
        }
    }
}

impl<A> UpperHex for TypedAssetId<A>
where
    A: Asset,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{}({:#X})", A::name(), self.id)
        } else {
            write!(f, "{}({:X})", A::name(), self.id)
        }
    }
}

/// Error type used by derive-macro.
#[derive(::std::fmt::Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("Failed to deserialize asset info from json")]
    Json(#[source] serde_json::Error),

    #[error("Failed to deserialize asset info from bincode")]
    Bincode(#[source] bincode::Error),
}
