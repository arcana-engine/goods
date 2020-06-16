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


async def run_check(*, toolchain, target=None, features=[]):
    args = [f'+{toolchain}', 'check',
            '--no-default-features', '--examples']
    if target is not None:
        args.append(f'--target={target}')
    if len(features) > 0:
        args.append(f'--features={",".join(features)}')

    proc = await asyncio.create_subprocess_exec('cargo', *args, stderr=subprocess.PIPE)
    returncode = await proc.wait()
    if returncode != 0:
        err = await proc.stderr.read()
        raise Exception(
            f'`cargo {" ".join(args)}` failed\n{err.decode("utf-8")}')
    else:
        print(f'`cargo {" ".join(args)}` succeeded')


async def run():
    _permutate_features = [
        "std",
    ]

    _iterate_features = [
        "fs",
        "json-format",
        "yaml-format",
        "ron-format",
        "futures-spawn",
    ]

    checks = []

    for target in [None, "wasm32-unknown-unknown"]:
        if target == "wasm32-unknown-unknown":
            permutate_features = _permutate_features
            iterate_features = _iterate_features + \
                ["fetch", "wasm-bindgen-spawn"]
        else:
            permutate_features = _permutate_features + ["sync"]
            iterate_features = _iterate_features + ["reqwest-default-tls",
                                                    "reqwest-native-tls",
                                                    "reqwest-rustls-tls",
                                                    "tokio-spawn"]

        toolchains = [
            "stable",
            "nightly",
        ]

        for toolchain in toolchains:
            for feature in iterate_features:
                for subset in powerset(permutate_features):
                    checks.append(run_check(features=subset + [feature],
                                            toolchain=toolchain,
                                            target=target))

    await asyncio.gather(*checks)


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
