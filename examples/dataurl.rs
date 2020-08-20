use {
    goods::*,
    std::{collections::HashMap, path::Path},
};

/// First we defined type to represent our assets.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
struct Object {
    foo: String,
    bar: u32,
}

/// We implement `SimpleAsset` for `Object` as it doesn't require any contextual conversion.
/// If asset type requires access to some context (like texture asset may require access to graphics context)
/// it must implemet `Asset` type then.
///
/// `SimpleAsset` implementations implement `Asset` automagically.
impl SimpleAsset for Object {}

/// Let's say that default format for `Object` is `JsonFormat`.
/// Only format types that implement `Default` may be default formats.
impl<K> AssetDefaultFormat<K> for Object {
    type DefaultFormat = JsonFormat;
}

fn main() {
    // Init logging system.
    env_logger::init();

    // Build source registry.
    // We'll use `String` as asset key.
    // Key type must be compatible with all used sources.
    let registry = Registry::<String>::builder()
        // One of the simplest sources is `DataUrlSource`.
        // It reads asset data embeded into URL that is used as key.
        .with(DataUrlSource)
        .build();

    // Create new asset cache with built registry.
    // Loading tasks will be spawned to `ThreadPool`.
    let cache = Cache::new(registry, futures_executor::ThreadPool::new().unwrap());

    let object = Object {
        foo: "qwerty".to_owned(),
        bar: 42,
    };

    let object_json =
        base64::encode_config(&serde_json::to_string(&object).unwrap(), base64::URL_SAFE);
    let url = format!("data:application/json;base64,{}", object_json);

    // Now lets finally load some assets.
    // First asset will be "asset.json".
    // We expect `FsSource` to find the sibling file with that name.
    // `Object`s default format is json, so we don't have to specify it here.
    let object_json: Handle<Object> = cache.load(url.clone());

    while object_json.is_pending() {
        std::thread::yield_now();
    }

    let object_json = object_json.get().unwrap();
    assert_eq!(*object_json, object);

    log::info!("From json: {:#?}", object_json);
}
