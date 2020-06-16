#![cfg(not(target_arch = "wasm32"))]

use {
    goods::*,
    legion::{entity::Entity, world::World},
    std::{convert::Infallible, iter::once, path::Path},
};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
struct Vel([f32; 2]);

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
struct Pos([f32; 2]);

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct VelPos {
    vel: Vel,
    pos: Pos,
}

#[derive(Default)]
struct VelPosFormat;

#[derive(Debug, Clone)]
struct VelPosEntity(Entity);

impl SyncAsset for VelPosEntity {
    type Error = Infallible;
    type Context = World;
    type Repr = VelPos;

    fn build(repr: VelPos, world: &mut World) -> Result<Self, Infallible> {
        if let &[e] = world.insert((), once((repr.vel, repr.pos))) {
            Ok(VelPosEntity(e))
        } else {
            panic!("Failed to insert entity into World");
        }
    }
}

/// Let's say that default format for `Object` is `JsonFormat`.
/// Only format types that implement `Default` may be default formats.
impl<K> AssetDefaultFormat<K> for VelPosEntity {
    type DefaultFormat = RonFormat;
}

fn main() {
    // Init logging system.
    env_logger::init();

    // Build source registry.
    let registry = Registry::builder()
        // One of the simplest sources is `FileSource`.
        // It reads asset data from files.
        // To get file path it joins root path with asset key.
        .with(FileSource::new(
            // Root dir will be parent dir of this file.
            Path::new(env!("CARGO_MANIFEST_DIR")).join("examples"),
        ))
        .build();

    // Create new asset cache with built registry.
    let cache = Cache::new(registry, futures_executor::ThreadPool::new().unwrap());

    // Now lets finally load some assets.
    // First asset will be "asset.json".
    // We expect `FsSource` to find the sibling file with that name.
    // `Object`s default format is json, so we don't have to specify it here.
    let entity: Handle<VelPosEntity> = cache.load("velpos.ron");

    // Create legion world that will receive loaded entities.
    let mut world = World::new();

    while entity.is_pending() {
        // Process raw assets into their final form.
        cache.process(&mut world);

        std::thread::yield_now();
    }

    // Unwrap loaded entity.
    let VelPosEntity(entity) = *entity.get().unwrap();

    log::info!("Entity: {}", entity);
    log::info!(
        "Pos: {:?}, Vel: {:?}",
        *world.get_component::<Pos>(entity).unwrap(),
        *world.get_component::<Vel>(entity).unwrap(),
    );
}
