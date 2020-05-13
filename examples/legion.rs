#[cfg(target_arch = "wasm32")]
core::compile_error!("This example cannot be built for wasm32 target");

use {
    futures_task::noop_waker_ref,
    goods::*,
    legion::{entity::Entity, world::World},
    std::{iter::once, path::Path, task::Context},
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

impl Asset for VelPosEntity {
    type Error = ron::de::Error;
    type Context = World;
    type Repr = VelPos;

    fn build(repr: VelPos, world: &mut World) -> Result<Self, ron::de::Error> {
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

    // Create new asset loader to drive async loading tasks.
    let mut loader = Loader::new();

    // Create new asset cache with built registry and loader.
    // Cache will issue loading tasks into this loader.
    // Note that `loader` is borrowed only for `Cache::new` function execution.
    let cache = Cache::new(registry, &loader);

    // Now lets finally load some assets.
    // First asset will be "asset.json".
    // We expect `FsSource` to find the sibling file with that name.
    // `Object`s default format is json, so we don't have to specify it here.
    let entity: Handle<VelPosEntity> = cache.load("velpos.ron");

    // Dummy async context.
    let mut ctx = Context::from_waker(noop_waker_ref());

    // Drive async loading tasks.
    // `FsSource` is not trully asynchronous as it uses `std::fs::File` API which is sync.
    // `FsSource::read` returns future that will be resolved on first `poll`.
    // So we expect loading to be finished after single call.
    let _ = loader.poll(&mut ctx, &cache);

    // Create legion world that will receive loaded entities.
    let mut world = World::new();

    // Process raw assets into their final form.
    // This variant of the function accept context
    // which is `legion::World` for entities.
    cache.process(&mut world);

    // Unwrap loaded entity.
    let VelPosEntity(entity) = *entity.get().unwrap();

    log::info!("Entity: {}", entity);
    log::info!(
        "Pos: {:?}, Vel: {:?}",
        *world.get_component::<Pos>(entity).unwrap(),
        *world.get_component::<Vel>(entity).unwrap(),
    );
}
