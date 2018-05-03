use std::fs::File;
use raw_cpuid::CpuId;
use std::io::Read;

#[macro_use]
mod macros;

// NOTE: Distributions should provide their distro ID and associated packages here, if applicable.

package!(amd_microcode {
    "debian" => "amd64-microcode"
});

package!(intel_microcode {
    "debian" => "intel-microcode"
});

package!(system76_driver {
    "debian" => "system76-driver"
});

pub fn append_packages(install_pkgs: &mut Vec<&'static str>) {
    // TODO: Obtain this from the environment.
    let distro = "debian";

    append_packages!(install_pkgs, distro => {
        processor_support,
        vendor_support
    });
}

/// Microcode packages for specific processor vendors.
fn processor_support(distro: &str) -> Option<&'static str> {
    if let Some(vf) = CpuId::new().get_vendor_info() {
        return match vf.as_string() {
            "AuthenticAMD" => amd_microcode(distro),
            "GenuineIntel" => intel_microcode(distro),
            _ => None
        };
    }

    None
}

/// Hardware enablement packages for hardware from specific vendors.
fn vendor_support(distro: &str) -> Option<&'static str> {
    if let Ok(mut file) = File::open("/sys/class/dmi/id/sys_vendor") {
        let mut string = String::new();
        if let Ok(_) = file.read_to_string(&mut string) {
            // NOTE: Vendors should add their logic & package names here.
            vendor!(string.trim(), distro, {
                starts_with "System76" => system76_driver
            });
        }
    }

    None
}
