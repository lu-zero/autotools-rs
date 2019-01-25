extern crate autotools;
extern crate bindgen;

use std::path::PathBuf;
use std::env::var;

fn main() {
    // Build the project insource, only building lib/libxmp.a
    let dst = autotools::Config::new("libxmp")
        .reconf("-v")
        .make_target("lib/libxmp.a")
        .insource(true)
        .build();

    // Simply link the library without using pkg-config
    println!("cargo:rustc-link-search=native={}", dst.join("lib").display());
    println!("cargo:rustc-link-lib=static=xmp");
    println!("cargo:rustc-link-lib=c");

    // generate bindings using bindgen
    let bindings = bindgen::Builder::default()
        .header("libxmp/include/xmp.h")
        .generate()
        .expect("unable to generate bindings");

    // setup the path to write bindings into
    let out_path = PathBuf::from(var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

}

