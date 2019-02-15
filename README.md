# autotools/configure&make support for build.rs

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![dependency status](https://deps.rs/repo/github/lu-zero/autotools-rs/status.svg)](https://deps.rs/repo/github/lu-zero/autotools-rs)
[![crates.io](https://img.shields.io/crates/v/autotools.svg?style=flat)](https://crates.io/crates/autotools)

A build dependency to compile a native library that uses [autotools][1] or
a compatible `configure` script + `make`.

It is based on [cmake-rs](https://github.com/alexcrichton/cmake-rs) and
the API tries to be as similar as possible to it.

``` toml
# Cargo.toml
[build-dependencies]
autotools = "0.1"
```

[1]: https://www.gnu.org/software/autoconf/autoconf.html
