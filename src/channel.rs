use {
    alloc::{sync::Arc, vec::Vec},
    core::{
        future::Future,
        mem::swap,
        pin::Pin,
        task::{Context, Poll, Waker},
    },
};

#[cfg(feature = "std")]
use parking_lot::Mutex;

#[cfg(not(feature = "std"))]
use spin::Mutex;

/// Reciver for spin-lock based channel.
pub(crate) struct Receiver<T> {
    inner: Arc<Mutex<Queue<T>>>,
}

impl<T> Receiver<T> {
    pub(crate) fn new() -> Self {
        let inner = Arc::new(Mutex::new(Queue {
            array: Vec::new(),
            wakers: Vec::new(),
        }));
        Receiver { inner }
    }

    pub(crate) fn sender(&self) -> Sender<T> {
        Sender {
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn recv(&self, scratch: &mut Vec<T>) {
        debug_assert!(scratch.is_empty());
        swap(&mut self.inner.lock().array, scratch);
    }
}

/// Sender for spin-lock based channel.
pub(crate) struct Sender<T> {
    inner: Arc<Mutex<Queue<T>>>,
}

impl<T> Sender<T> {
    pub(crate) fn send(&self, value: T) {
        let mut lock = self.inner.lock();
        lock.array.push(value);
        for waker in lock.wakers.drain(..) {
            waker.wake();
        }
    }
}

struct Queue<T> {
    array: Vec<T>,
    wakers: Vec<Waker>,
}

/// Spin-lock based shareable slot.
pub(crate) struct Slot<T> {
    inner: Arc<Mutex<SlotInner<T>>>,
}

impl<T> Slot<T> {
    pub(crate) fn poll(&mut self, ctx: &mut Context<'_>) -> Poll<T> {
        let mut lock = self.inner.lock();
        if let Some(value) = lock.value.take() {
            Poll::Ready(value)
        } else {
            let waker = ctx.waker();
            if let Some(w) = &lock.waker {
                if !w.will_wake(waker) {
                    lock.waker = Some(waker.clone());
                }
            } else {
                lock.waker = Some(waker.clone());
            }
            drop(lock);
            Poll::Pending
        }
    }
}

impl<T> Future for Slot<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<T> {
        self.get_mut().poll(ctx)
    }
}

/// Setter for spin-lock based channel.
pub(crate) struct Setter<T> {
    inner: Arc<Mutex<SlotInner<T>>>,
}

impl<T> Setter<T> {
    pub(crate) fn set(self, value: T) {
        let mut lock = self.inner.lock();
        debug_assert!(lock.value.is_none());
        lock.value = Some(value);
        if let Some(waker) = lock.waker.take() {
            waker.wake();
        }
    }
}

pub(crate) fn slot<T>() -> (Slot<T>, Setter<T>) {
    let inner = Arc::new(Mutex::new(SlotInner {
        value: None,
        waker: None,
    }));

    (
        Slot {
            inner: inner.clone(),
        },
        Setter { inner },
    )
}

struct SlotInner<T> {
    value: Option<T>,
    waker: Option<Waker>,
}
