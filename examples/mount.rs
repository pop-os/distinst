extern crate distinst;

use distinst::{Mount, MountKind};

use std::fs;
use std::io::{Read, Result};
use std::process;

fn mount() -> Result<()> {
    fs::create_dir_all("test_proc").unwrap();

    let mount = Mount::new("/proc", "test_proc", MountKind::Bind).unwrap();

    let mut file = fs::File::open("test_proc/cmdline").unwrap();

    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    println!("{}", string);

    drop(mount);

    Ok(())
}

fn main() {
    if let Err(err) = mount() {
        eprintln!("mount: failed: {}", err);
        process::exit(1);
    }
}
