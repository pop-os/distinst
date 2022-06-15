extern crate distinst_utils as misc;
#[macro_use]
extern crate log;
extern crate os_release;
extern crate proc_modules;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
extern crate raw_cpuid;

use os_release::OsRelease;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use raw_cpuid::CpuId;
use std::{fs, io::Read};

pub mod switchable_graphics;
#[macro_use]
mod macros;

use proc_modules::Module;

// NOTE: Distributions should provide their distro ID and associated packages here, if applicable.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn amd_microcode(os_release: &OsRelease) -> Option<&'static str> {
    if &os_release.id_like == "debian" {
        Some("amd64-microcode")
    } else {
        None
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn intel_microcode(os_release: &OsRelease) -> Option<&'static str> {
    if &os_release.id_like == "debian" {
        Some("intel-microcode")
    } else {
        None
    }
}

fn system76_driver(os_release: &OsRelease) -> Option<&'static str> {
    if &os_release.name == "Pop!_OS" {
        Some("system76-driver")
    } else {
        None
    }
}

fn version(os_release: &OsRelease) -> Option<(u8, u8)> {
    let mut components = os_release.version_id.split('.');
    let major = components.next()?.parse::<u8>().ok()?;
    let minor = components.next()?.parse::<u8>().ok()?;
    Some((major, minor))
}

fn hp_vendor(os_release: &OsRelease) -> Option<&'static str> {
    if &os_release.name == "Pop!_OS" && version(os_release).unwrap_or((0, 0)) >= (22, 04) {
        let board = fs::read_to_string("/sys/class/dmi/id/board_name").ok()?;
        // HP Dev One
        if board.trim() == "8A78" {
            return Some("pop-hp-vendor");
        }
    }
    None
}

fn nvidia_driver(os_release: &OsRelease) -> Option<&'static str> {
    if &os_release.name == "Pop!_OS"
        && vendor().map_or(false, |vendor| vendor.starts_with("System76"))
    {
        return Some("system76-driver-nvidia");
    }

    None
}
pub fn append_packages(install_pkgs: &mut Vec<&'static str>, os_release: &OsRelease) {
    append_packages!(
        os_release,
        install_pkgs { processor_support, vendor_support, graphics_support }
    );
}

fn graphics_support(os_release: &OsRelease) -> Option<&'static str> {
    Module::all().ok().and_then(|modules| {
        if modules.iter().any(|x| &x.module == "nvidia") {
            return nvidia_driver(os_release);
        }

        None
    })
}

/// Microcode packages for specific processor vendors.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn processor_support(os_release: &OsRelease) -> Option<&'static str> {
    if let Some(vf) = CpuId::new().get_vendor_info() {
        return match vf.as_string() {
            "AuthenticAMD" => amd_microcode(os_release),
            "GenuineIntel" => intel_microcode(os_release),
            _ => None,
        };
    }

    None
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn processor_support(_os_release: &OsRelease) -> Option<&'static str> {
    None
}

/// Hardware enablement packages for hardware from specific vendors.
fn vendor_support(os_release: &OsRelease) -> Option<&'static str> {
    if let Some(vendor) = vendor() {
        // NOTE: Vendors should add their logic & package names here.
        vendor!(os_release, vendor.trim() => {
            starts_with "System76" => system76_driver,
            starts_with "HP" => hp_vendor
        });
    }

    None
}

fn vendor() -> Option<String> {
    let mut vendor = String::new();
    misc::open("/sys/class/dmi/id/sys_vendor")
        .and_then(|mut file| file.read_to_string(&mut vendor))
        .ok()
        .map(|_| vendor)
}
