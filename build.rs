extern crate cheddar;

fn main() {
    cheddar::Cheddar::new().expect("could not read manifest")
        .module("c").expect("malformed module path")
        .run_build("target/include/distinst.h");
}
