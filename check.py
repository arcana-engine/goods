#!/usr/bin/env python3

import asyncio
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


async def check(*, toolchain='stable', target=None, features=[], mandatory_features=[]):
    for subset in powerset(features):
        subset = set(subset) | set(mandatory_features)

        args = [f'+{toolchain}', 'check',
                '--no-default-features', '--examples']
        if len(subset) > 0:
            args.append(f'--features={",".join(subset)}')

        if target is not None:
            args.append(f'--target={target}')

        proc = await asyncio.create_subprocess_exec('cargo', *args, stderr=subprocess.PIPE)
        returncode = await proc.wait()
        if returncode != 0:
            err = await proc.stderr.read()
            raise Exception(
                f'`cargo {" ".join(args)}` failed\n{err.decode("utf-8")}')


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


async def run():
    await asyncio.gather(
        check(toolchain="nightly", features=features),
        check(toolchain="stable", features=features,
              mandatory_features=['std']),
        check(toolchain="nightly", target="wasm32-unknown-unknown",
              features=wasm_features),
        check(toolchain="stable", target="wasm32-unknown-unknown",
              features=wasm_features, mandatory_features=['std']),
    )


def main():
    (major, minor, micro, _, _) = sys.version_info
    if major >= 3:
        if minor >= 7:
            asyncio.run(run())
            return
        elif minor >= 4:
            loop = asyncio.get_event_loop()
            loop.run_until_complete(run())
            loop.close()
            return

    print(
        f'Python version 3.4+ is required, but current version is {major}.{minor}.{micro}')
    sys.exit(1)


if __name__ == '__main__':
    main()
