# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0] - 2021-07-19

Total rework.
This is whole asset pipeline now.

`goods` crates remains a loader.
Added:
* Basic support for asset hot-reloading.
* Proc-macro for automatic definition of complex asset types.

Changed:
* Assets are addressed by UUIDs.
* Improved multi-threading support.

`goods-treasury` asset database with programmable importing and hot-reimporting.
