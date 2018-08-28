mod bootloader;
mod configure;
mod initialize;
mod mount;
mod partition;

pub use self::bootloader::*;
pub use self::configure::*;
pub use self::initialize::*;
pub use self::mount::*;
pub use self::partition::*;

use disk::mount::{BIND, Mount};
use std::io;
use std::fs;
use std::path::Path;
use NO_EFI_VARIABLES;
use std::sync::atomic::Ordering;

/// Installation step
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Step {
    Init,
    Partition,
    Extract,
    Configure,
    Bootloader,
}

fn mount_cdrom(mount_dir: &Path) -> io::Result<Option<Mount>> {
    let cdrom_source = Path::new("/cdrom");
    let cdrom_target = mount_dir.join("cdrom");
    mount_bind_if_exists(&cdrom_source, &cdrom_target)
}

fn mount_efivars(mount_dir: &Path) -> io::Result<Option<Mount>> {
    if NO_EFI_VARIABLES.load(Ordering::Relaxed) {
        info!("was ordered to not mount the efivars directory");
        Ok(None)
    } else {
        let efivars_source = Path::new("/sys/firmware/efi/efivars");
        let efivars_target = mount_dir.join("sys/firmware/efi/efivars");
        mount_bind_if_exists(&efivars_source, &efivars_target)
    }
}

fn mount_bind_if_exists(source: &Path, target: &Path) -> io::Result<Option<Mount>> {
    if source.exists() {
        let _ = fs::create_dir_all(&target);
        Ok(Some(Mount::new(&source, &target, "none", BIND, None)?))
    } else {
        Ok(None)
    }
}
