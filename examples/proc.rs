use {
    goods::{
        source::{AssetData, Source},
        Asset, AssetField, Loader, Uuid,
    },
    std::{
        collections::HashMap,
        convert::Infallible,
        future::{ready, Ready},
    },
};

#[derive(Clone, Asset)]
pub struct UnitAsset;

#[derive(Clone, Asset)]
pub struct SimpleAsset {
    field: SimpleFieldType,
}

#[derive(Clone, Asset)]
pub struct TwoLevelAsset {
    #[external]
    a: SimpleAsset,
}

#[derive(Clone, AssetField)]
pub struct Container {
    #[external]
    a: UnitAsset,
}

#[derive(Clone, Asset)]
pub struct ComplexAsset {
    #[container]
    c: Container,

    #[external]
    a: SimpleAsset,
}

#[derive(Clone, Asset)]
pub struct WrapperAsset {
    wrapped: u32,
}

impl From<WrapperAsset> for u32 {
    fn from(w: WrapperAsset) -> Self {
        w.wrapped
    }
}

#[derive(Clone, Asset)]
pub struct AssetWithWrapper {
    #[external(as WrapperAsset)]
    a: u32,
}

#[derive(Clone, Asset)]
#[serde(rename_all = "UPPERCASE")]
pub struct AssetWithSerdeAttribute {
    #[serde(default = "default_a")]
    a: u32,
}

fn default_a() -> u32 {
    42
}

#[derive(Clone, Asset)]
pub struct AssetWithOption {
    #[serde(default)]
    #[external]
    foo: Option<SimpleAsset>,
}

#[derive(Clone, serde::Deserialize)]
struct SimpleFieldType {}

/// Dummy source which just gives bytes from map.
struct HashMapSource(HashMap<Uuid, Box<[u8]>>);

impl Source for HashMapSource {
    type Error = Infallible;
    type Fut = Ready<Result<Option<AssetData>, Infallible>>;

    fn load(&self, uuid: &Uuid) -> Self::Fut {
        ready(Ok(match self.0.get(uuid) {
            Some(data) => Some(AssetData {
                bytes: data.clone(),
                version: 0,
            }),
            None => None,
        }))
    }
    fn update(&self, _uuid: &Uuid, _version: u64) -> Self::Fut {
        ready(Ok(None))
    }
}

fn main() {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async move { run().await.unwrap() })
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Fill map.
    let source = HashMapSource(
        vec![
            (Uuid::from_u128(2), b"null".to_vec().into_boxed_slice()),
            (
                Uuid::from_u128(3),
                b"{\"field\": {}}".to_vec().into_boxed_slice(),
            ),
            (
                Uuid::from_u128(4),
                b"{\"a\":\"00000000-0000-0000-0000-000000000003\"}"
                    .to_vec()
                    .into_boxed_slice(),
            ),
            (
                Uuid::from_u128(5),
                b"{\"c\": {\"a\":\"00000000-0000-0000-0000-000000000002\"}, \"a\":\"00000000-0000-0000-0000-000000000003\"}"
                    .to_vec()
                    .into_boxed_slice(),
            ),
            (
                Uuid::from_u128(6),
                b"{\"wrapped\": 42}".to_vec().into_boxed_slice(),
            ),
            (
                Uuid::from_u128(7),
                b"{\"a\":\"00000000-0000-0000-0000-000000000006\"}".to_vec().into_boxed_slice(),
            ),
            (
                Uuid::from_u128(8),
                b"{}".to_vec().into_boxed_slice(),
            ),
        ]
        .into_iter()
        .collect(),
    );

    let loader = Loader::builder().with(source).build();

    let _: &UnitAsset = loader.load(&Uuid::from_u128(2)).await.get(&mut ())?;
    println!("UnitAsset loaded");

    let _: &SimpleAsset = loader.load(&Uuid::from_u128(3)).await.get(&mut ())?;
    println!("SimpleAsset loaded");

    let _: &TwoLevelAsset = loader.load(&Uuid::from_u128(4)).await.get(&mut ())?;
    println!("TwoLevelAsset loaded");

    let _: &ComplexAsset = loader.load(&Uuid::from_u128(5)).await.get(&mut ())?;
    println!("ComplexAsset loaded");

    let _: &WrapperAsset = loader.load(&Uuid::from_u128(6)).await.get(&mut ())?;
    println!("WrapperAsset loaded");

    let _: &AssetWithWrapper = loader.load(&Uuid::from_u128(7)).await.get(&mut ())?;
    println!("AssetWithWrapper loaded");

    let _: &AssetWithSerdeAttribute = loader.load(&Uuid::from_u128(8)).await.get(&mut ())?;
    println!("AssetWithSerdeAttribute loaded");

    let _: &AssetWithOption = loader.load(&Uuid::from_u128(8)).await.get(&mut ())?;
    println!("AssetWithOption loaded");

    Ok(())
}
