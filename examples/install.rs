extern crate distinst;

use distinst::Chroot;

use std::fs;
use std::io::{Read, Result};
use std::process;

fn mount() -> Result<()> {
    let mut command = process::Command::new("unsquashfs");
    command.arg("-f");
    command.arg("-d");
    command.arg("chroot");
    command.arg("bash/filesystem.squashfs");
    command.status().unwrap();

    let mut chroot = Chroot::new("chroot").unwrap();

    chroot.command("apt", ["install", "pop-desktop"].iter()).unwrap();

    chroot.unmount().unwrap();

    Ok(())
}

fn main() {
    if let Err(err) = mount() {
        eprintln!("mount: failed: {}", err);
        process::exit(1);
    }
}
