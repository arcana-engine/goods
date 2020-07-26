use core::{fmt::Debug, hash::Hash};

/// Compound trait that is implemented for any type that implements all bound traits.
pub trait Key: Send + Sync + Debug + Eq + Hash + Clone + 'static {}
impl<T> Key for T where T: Send + Sync + Debug + Eq + Hash + Clone + 'static {}
