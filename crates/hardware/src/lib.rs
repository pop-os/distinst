extern crate distinst_utils as misc;
#[macro_use]
extern crate log;
extern crate os_release;
extern crate proc_modules;
extern crate raw_cpuid;

use os_release::OsRelease;
use raw_cpuid::CpuId;
use std::io::Read;

pub mod blacklist;
#[macro_use]
mod macros;

use proc_modules::Module;

// NOTE: Distributions should provide their distro ID and associated packages here, if applicable.

fn amd_microcode(os_release: &OsRelease) -> Option<&'static str> {
    if &os_release.id_like == "debian" {
        Some("amd64-microcode")
    } else {
        None
    }
}

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

/// Hardware enablement packages for hardware from specific vendors.
fn vendor_support(os_release: &OsRelease) -> Option<&'static str> {
    if let Some(vendor) = vendor() {
        // NOTE: Vendors should add their logic & package names here.
        vendor!(os_release, vendor.trim() => {
            starts_with "System76" => system76_driver
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
