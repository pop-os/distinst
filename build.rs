extern crate cbindgen;

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn generate_dylib_bindings() {
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

	let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

	cbindgen::generate(crate_dir)
		.expect("unable to generate bindings")
		.write_to_file(target_path.join("include").join("distinst.h"));
}

fn main() {
	// NOTE: Comment this out when developing.
	generate_dylib_bindings();
}
