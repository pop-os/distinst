use std::fs::File;
use raw_cpuid::CpuId;
use std::io::Read;
use os_release::OS_RELEASE;

pub mod blacklist;
#[macro_use]
mod macros;
mod modules;

use self::modules::Module;

// NOTE: Distributions should provide their distro ID and associated packages here, if applicable.

package!(amd_microcode {
    like "debian" => "amd64-microcode"
});

package!(intel_microcode {
    like "debian" => "intel-microcode"
});

package!(system76_driver {
    like "debian" => "system76-driver"
});

package!(nvidia_driver {
    like "debian", vendor "System76" => "system76-driver-nvidia"
});

pub fn append_packages(install_pkgs: &mut Vec<&'static str>) {
    append_packages!(install_pkgs {
        processor_support,
        vendor_support,
        graphics_support
    });
}

fn graphics_support() -> Option<&'static str> {
    Module::all().ok().and_then(|modules| {
        if modules.iter().any(|x| &x.name == "nvidia") {
            return nvidia_driver();
        }

        None
    })
}

/// Microcode packages for specific processor vendors.
fn processor_support() -> Option<&'static str> {
    if let Some(vf) = CpuId::new().get_vendor_info() {
        return match vf.as_string() {
            "AuthenticAMD" => amd_microcode(),
            "GenuineIntel" => intel_microcode(),
            _ => None
        };
    }

    None
}

/// Hardware enablement packages for hardware from specific vendors.
fn vendor_support() -> Option<&'static str> {
    if let Some(vendor) = vendor() {
        // NOTE: Vendors should add their logic & package names here.
        vendor!(vendor.trim() => {
            starts_with "System76" => system76_driver
        });
    }

    None
}

fn vendor() -> Option<String> {
    let mut vendor = String::new();
    File::open("/sys/class/dmi/id/sys_vendor")
        .and_then(|mut file| file.read_to_string(&mut vendor))
        .ok().map(|_| vendor)
}
