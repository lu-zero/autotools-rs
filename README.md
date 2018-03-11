# autotools/configure&make support for build.rs

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A build dependency to compile a native library that uses [autotools][1] or
a compatible `configure` script + `make`.

It is based on [cmake-rs](https://github.com/alexcrichton/cmake-rs) and
the API tries to be as similar as possible to it.

``` toml
# Cargo.toml
[build-dependencies]
autotools = "0.1"
```
