# autotools/configure&make support for build.rs

[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![dependency status](https://deps.rs/repo/github/lu-zero/autotools-rs/status.svg)](https://deps.rs/repo/github/lu-zero/autotools-rs)
[![crates.io](https://img.shields.io/crates/v/autotools.svg?style=flat)](https://crates.io/crates/autotools)
[![docs.rs](https://docs.rs/autotools/badge.svg)](https://docs.rs/autotools)

A build dependency to compile a native library that uses [autotools][1] or
a compatible `configure` script + `make`.

It is based on [cmake-rs](https://github.com/alexcrichton/cmake-rs) and
the API tries to be as similar as possible to it.

``` toml
# Cargo.toml
[build-dependencies]
autotools = "0.3"
```

``` rust
// build.rs
use autotools;

// Build the project in the path `foo` and installs it in `$OUT_DIR`
let dst = autotools::build("foo");

// Simply link the library without using pkg-config
println!("cargo:rustc-link-search=native={}", dst.display());
println!("cargo:rustc-link-lib=static=foo");
```

``` rust
// build.rs
use autotools::Config;

let dst = Config::new("foo")
    .reconf("-ivf")
    .enable("feature", None)
    .with("dep", None)
    .disable("otherfeature", None)
    .without("otherdep", None)
    .cflag("-Wall")
    .build();
```

[1]: https://www.gnu.org/software/autoconf/autoconf.html
