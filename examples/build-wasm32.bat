@echo off
IF "%1"=="" (
    echo "Usage: build-wasm32 <example-name>"
) ELSE (
echo "Build WASM module"
setlocal
set RUSTFLAGS=-C linker=lld
cargo build --all-features --release --target=wasm32-unknown-unknown --example %1

echo "Generate bindings"
wasm-bindgen --target web --out-dir "%~dp0\generated" --no-typescript "%~dp0\..\..\target\wasm32-unknown-unknown\release\examples\%1.wasm"
wasm-opt "%~dp0\generated\%1_bg.wasm" -o "%~dp0\generated\%1.wasm" -O2 --disable-threads

echo "Success"
)
