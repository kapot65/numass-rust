extern crate cbindgen;

use std::env;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_config(
            cbindgen::Config::from_file("cbindgen.toml").expect("Can't find/parse cbindgen.toml"),
        )
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("./bindings/processing-internal.h");
}
