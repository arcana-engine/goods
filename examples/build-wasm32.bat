@echo off
IF "%1"=="" (
    echo "Usage: build-wasm32 <example-name>"
) ELSE (
echo "Build WASM module"
setlocal
set RUSTFLAGS=-C linker=lld

cargo build --release --target=wasm32-unknown-unknown --bin %* || exit /b 1
wasm-bindgen --target web --out-dir "%~dp0\generated" --no-typescript "%~dp0\target\wasm32-unknown-unknown\release\%1.wasm" || exit /b 1
@REM wasm-opt "%~dp0\generated\%1_bg.wasm" -o "%~dp0\generated\%1.wasm" -O2 --disable-threads || exit /b 1
echo "Success"
)
