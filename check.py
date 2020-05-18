#!/usr/bin/env python3

import subprocess
import sys


def powerset(input):
    if len(input) == 0:
        return [[]]

    pivot = input[0]

    subset = powerset(input[1:])
    with_pivot = subset.copy()
    for i, set in enumerate(with_pivot):
        with_pivot[i] = [pivot] + set

    return subset + with_pivot


def check(*, toolchain='stable', target=None, features=[], mandatory_features=[]):
    for subset in powerset(features):
        subset = set(subset) | set(mandatory_features)

        cmd = ["cargo", f'+{toolchain}', "check",
               "--no-default-features", "--examples"]
        if len(subset) > 0:
            cmd.append(f'--features={",".join(subset)}')

        if target is not None:
            cmd.append(f'--target={target}')

        print(" ".join(cmd))
        status = subprocess.run(cmd)
        if status.returncode != 0:
            sys.exit(1)


features = [
    "std",
    "fs",
    "sync",
    "json-format",
    "yaml-format",
    "ron-format",
    "reqwest-default-tls",
    "reqwest-native-tls",
    "reqwest-rustls-tls",
    "futures-spawn",
]

wasm_features = [
    "std",
    "fetch",
    "json-format",
    "yaml-format",
    "ron-format",
    "wasm-bindgen-spawn",
]

check(toolchain="nightly", features=features)
check(toolchain="stable", features=features, mandatory_features=['std'])
check(toolchain="nightly", target="wasm32-unknown-unknown", features=wasm_features)
check(toolchain="stable", target="wasm32-unknown-unknown",
      features=wasm_features, mandatory_features=['std'])
