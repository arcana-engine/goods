#!/bin/bash

set -e

if [ -z "$1" ]
then
    echo "Usage: build-wasm32.sh <example-name>"
else
    cargo build --target=wasm32-unknown-unknown --bin $@
    wasm-bindgen --target web --out-dir "$(dirname $0)/generated" --debug --no-typescript "$(dirname $0)/../target/wasm32-unknown-unknown/debug/$1.wasm"
    # wasm-opt "$(dirname $0)/generated/%1_bg.wasm" -o "$(dirname $0)/generated/%1.wasm" -O2 --disable-threads
fi
