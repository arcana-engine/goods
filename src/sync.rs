use core::{future::Future, pin::Pin};

#[cfg(not(feature = "sync"))]
pub trait Send {}
#[cfg(not(feature = "sync"))]
impl<T> Send for T {}

#[cfg(not(feature = "sync"))]
pub trait Sync {}
#[cfg(not(feature = "sync"))]
impl<T> Sync for T {}

#[cfg(feature = "sync")]
pub use core::marker::{Send, Sync};

/// An owned dynamically typed [`Future`] for use in cases where you can't
/// statically type your result or need to add some indirection.
#[cfg(not(feature = "sync"))]
pub type BoxFuture<'a, T> = Pin<alloc::boxed::Box<dyn Future<Output = T> + 'a>>;

/// An owned dynamically typed [`Future`] for use in cases where you can't
/// statically type your result or need to add some indirection.
#[cfg(feature = "sync")]
pub type BoxFuture<'a, T> = Pin<alloc::boxed::Box<dyn Future<Output = T> + Send + 'a>>;

#[cfg(not(feature = "sync"))]
pub(crate) type Ptr<T> = alloc::rc::Rc<T>;

#[cfg(feature = "sync")]
pub(crate) type Ptr<T> = alloc::sync::Arc<T>;

#[cfg(feature = "sync")]
pub(crate) type Lock<T> = spin::Mutex<T>;

#[cfg(not(feature = "sync"))]
#[repr(transparent)]
#[derive(Debug, Default)]
pub(crate) struct Lock<T: ?Sized> {
    cell: core::cell::RefCell<T>,
}

#[cfg(not(feature = "sync"))]
impl<T> Lock<T> {
    pub(crate) fn new(value: T) -> Self {
        Lock {
            cell: core::cell::RefCell::new(value),
        }
    }
}

#[cfg(not(feature = "sync"))]
impl<T> Lock<T>
where
    T: ?Sized,
{
    pub(crate) fn lock(&self) -> core::cell::RefMut<T> {
        self.cell.borrow_mut()
    }
}
