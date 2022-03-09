extern crate cbindgen;

use std::{env, fs, io::Write, path::PathBuf};

fn main() {
    let target_dir = PathBuf::from("../target");

    let pkg_config = format!(
        include_str!("distinst.pc.in"),
        name = "distinst",
        description = env::var("CARGO_PKG_DESCRIPTION").unwrap(),
        version = env::var("CARGO_PKG_VERSION").unwrap()
    );

    fs::File::create(target_dir.join("distinst.pc.stub"))
        .expect("failed to create pc.stub")
        .write_all(pkg_config.as_bytes())
        .expect("failed to write pc.stub");

    cbindgen::generate(env::var("CARGO_MANIFEST_DIR").unwrap())
        .expect("unable to generate bindings")
        .write_to_file(target_dir.join("distinst.h"));
}
