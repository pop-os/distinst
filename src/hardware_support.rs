use std::fs::File;
use raw_cpuid::CpuId;
use std::io::Read;

macro_rules! package {
    ($name:tt { $($distro:expr => $package:expr),+ })  => (
        fn $name(distro: &str) -> Option<&'static str> {
            match distro {
                $($distro => Some($package)),+,
                _ => None
            }
        }
    )
}

macro_rules! append_packages {
    ($install_pkgs:ident, $distro:expr => { $($detect:tt),+ }) => (
        $(
            if let Some(package) = $detect($distro) {
                $install_pkgs.push(package);
            }
        )+
    );
}

macro_rules! vendor {
    ($input:expr, $distro:expr, { $($method:tt $pattern:expr => $func:tt),+ }) => (
        $(
            if $input.$method($pattern) {
                return $func($distro);
            }
        )+
    )
}

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

fn vendor_support(distro: &str) -> Option<&'static str> {
    if let Ok(mut file) = File::open("/sys/class/dmi/id/sys_vendor") {
        let mut string = String::new();
        if let Ok(_) = file.read_to_string(&mut string) {
            vendor!(string.trim(), distro, {
                starts_with "System76" => system76_driver
            });
        }
    }

    None
}
