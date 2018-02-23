use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let target_path = PathBuf::from("target");

    let pkg_config = format!(
        include_str!("distinst.pc.in"),
        name = env::var("CARGO_PKG_NAME").unwrap(),
        description = env::var("CARGO_PKG_DESCRIPTION").unwrap(),
        version = env::var("CARGO_PKG_VERSION").unwrap()
    );

    fs::create_dir_all(target_path.join("pkgconfig")).unwrap();
    fs::File::create(target_path.join("pkgconfig").join("distinst.pc.stub"))
        .unwrap()
        .write_all(&pkg_config.as_bytes())
        .unwrap();
}
