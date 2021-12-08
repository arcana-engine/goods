use futures::future::BoxFuture;

use {
    goods::{
        source::{AssetData, Source},
        Asset, AssetField, AssetId, Loader,
    },
    std::{collections::HashMap, convert::Infallible, future::ready},
};

#[derive(Clone, Asset)]
#[asset(name = "unit")]
pub struct UnitAsset;

#[derive(Clone, Asset)]
#[asset(name = "simple")]
pub struct SimpleAsset {
    field: SimpleFieldType,
}

#[derive(Clone, Asset)]
#[asset(name = "two-level")]
pub struct TwoLevelAsset {
    #[asset(external)]
    a: SimpleAsset,
}

#[derive(Clone, AssetField)]
pub struct Container {
    #[asset(external)]
    a: UnitAsset,
}

#[derive(Clone, Asset)]
#[asset(name = "complex")]
pub struct ComplexAsset {
    #[asset(container)]
    c: Container,

    #[asset(external)]
    a: SimpleAsset,
}

#[derive(Clone, Asset)]
#[asset(name = "wrapper")]
pub struct WrapperAsset {
    wrapped: u32,
}

impl From<WrapperAsset> for u32 {
    fn from(w: WrapperAsset) -> Self {
        w.wrapped
    }
}

#[derive(Clone, Asset)]
#[asset(name = "with-wrapper")]
pub struct AssetWithWrapper {
    #[asset(external(as WrapperAsset))]
    a: u32,
}

#[derive(Clone, Asset)]
#[asset(name = "with-serde")]
#[serde(rename_all = "UPPERCASE")]
pub struct AssetWithSerdeAttribute {
    #[serde(default = "default_a")]
    a: u32,
}

fn default_a() -> u32 {
    42
}

#[derive(Clone, Asset)]
#[asset(name = "with-option")]
pub struct AssetWithOption {
    #[serde(default)]
    #[asset(external)]
    foo: Option<SimpleAsset>,
}

#[derive(Clone, serde::Deserialize)]
struct SimpleFieldType {}

/// Dummy source which just gives bytes from map.
struct HashMapSource(HashMap<AssetId, Box<[u8]>>);

impl Source for HashMapSource {
    type Error = Infallible;

    fn find(&self, path: &str, _asset: &str) -> BoxFuture<Option<AssetId>> {
        let id = AssetId(path.parse().unwrap());
        Box::pin(ready(Some(id)))
    }

    fn load(&self, id: AssetId) -> BoxFuture<Result<Option<AssetData>, Infallible>> {
        Box::pin(ready(Ok(match self.0.get(&id) {
            Some(data) => Some(AssetData {
                bytes: data.clone(),
                version: 0,
            }),
            None => None,
        })))
    }

    fn update(
        &self,
        _id: AssetId,
        _version: u64,
    ) -> BoxFuture<Result<Option<AssetData>, Infallible>> {
        Box::pin(ready(Ok(None)))
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
            (
                AssetId::new(2).unwrap(),
                b"null".to_vec().into_boxed_slice(),
            ),
            (
                AssetId::new(3).unwrap(),
                b"{\"field\": {}}".to_vec().into_boxed_slice(),
            ),
            (
                AssetId::new(4).unwrap(),
                b"{\"a\": 3}".to_vec().into_boxed_slice(),
            ),
            (
                AssetId::new(5).unwrap(),
                b"{\"c\": {\"a\": 2}, \"a\": 3}".to_vec().into_boxed_slice(),
            ),
            (
                AssetId::new(6).unwrap(),
                b"{\"wrapped\": 42}".to_vec().into_boxed_slice(),
            ),
            (
                AssetId::new(7).unwrap(),
                b"{\"a\": 6}".to_vec().into_boxed_slice(),
            ),
            (AssetId::new(8).unwrap(), b"{}".to_vec().into_boxed_slice()),
        ]
        .into_iter()
        .collect(),
    );

    let loader = Loader::builder().with(source).build();

    let _: &UnitAsset = loader.load("2").await.build(&mut ())?;
    println!("UnitAsset loaded");

    let _: &SimpleAsset = loader.load("3").await.build(&mut ())?;
    println!("SimpleAsset loaded");

    let _: &TwoLevelAsset = loader.load("4").await.build(&mut ())?;
    println!("TwoLevelAsset loaded");

    let _: &ComplexAsset = loader.load("5").await.build(&mut ())?;
    println!("ComplexAsset loaded");

    let _: &WrapperAsset = loader.load("6").await.build(&mut ())?;
    println!("WrapperAsset loaded");

    let _: &AssetWithWrapper = loader.load("7").await.build(&mut ())?;
    println!("AssetWithWrapper loaded");

    let _: &AssetWithSerdeAttribute = loader.load("8").await.build(&mut ())?;
    println!("AssetWithSerdeAttribute loaded");

    let _: &AssetWithOption = loader.load("8").await.build(&mut ())?;
    println!("AssetWithOption loaded");

    Ok(())
}
