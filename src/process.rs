use {
    crate::{
        asset::Asset,
        channel::{Receiver, Sender, Setter},
    },
    alloc::{boxed::Box, vec::Vec},
    core::any::{Any, TypeId},
    hashbrown::hash_map::{Entry, HashMap},
};

#[cfg(feature = "std")]
use parking_lot::Mutex;

#[cfg(not(feature = "std"))]
use spin::Mutex;

pub(crate) trait AnyProcess<C> {
    fn run(self: Box<Self>, ctx: &mut C);
}

pub(crate) struct Process<A: Asset> {
    pub(crate) repr: A::Repr,
    pub(crate) setter: Setter<A::BuildFuture>,
}

impl<A> AnyProcess<A::Context> for Process<A>
where
    A: Asset,
{
    fn run(self: Box<Self>, ctx: &mut A::Context) {
        self.setter.set(A::build(self.repr, ctx))
    }
}

struct Processes<C> {
    receiver: Receiver<Box<dyn AnyProcess<C> + Send>>,
}

pub(crate) struct Processor {
    processes: Mutex<HashMap<TypeId, Box<dyn Any + Send>>>,
}

impl Processor {
    pub(crate) fn new() -> Self {
        Processor {
            processes: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn sender<A>(&self) -> Sender<Box<dyn AnyProcess<A::Context> + Send>>
    where
        A: Asset,
    {
        let mut lock = self.processes.lock();
        match lock.entry(TypeId::of::<A::Context>()) {
            Entry::Vacant(entry) => {
                let receiver = Receiver::new();
                let sender = receiver.sender();
                entry.insert(Box::new(Processes::<A::Context> { receiver }));
                sender
            }
            Entry::Occupied(entry) => Any::downcast_ref::<Processes<A::Context>>(&**entry.get())
                .unwrap()
                .receiver
                .sender(),
        }
    }

    pub(crate) fn run<C: 'static>(&self, ctx: &mut C) {
        let lock = self.processes.lock();
        if let Some(processes) = lock.get(&TypeId::of::<C>()) {
            let processes = Any::downcast_ref::<Processes<C>>(&**processes).unwrap();
            let mut received = Vec::new();
            processes.receiver.recv(&mut received);
            for received in received {
                received.run(ctx);
            }
        }
    }
}
