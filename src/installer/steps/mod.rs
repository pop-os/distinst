mod bootloader;
mod configure;
mod initialize;
mod partition;

pub use self::{bootloader::*, configure::*, initialize::*, partition::*};

use std::{
    borrow::Cow,
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

use sys_mount::*;
use crate::NO_EFI_VARIABLES;

/// Installation step
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Step {
    Backup,
    Init,
    Partition,
    Extract,
    Configure,
    Bootloader,
}

fn mount_cdrom(mount_dir: &Path) -> io::Result<Option<(UnmountDrop<Mount>, PathBuf)>> {
    let cdrom_source = Path::new("/cdrom");
    let cdrom_target = mount_dir.join("cdrom");
    mount_bind_if_exists(&cdrom_source, &cdrom_target).map(|res| res.map(|m| (m, cdrom_target)))
}

pub fn mount_efivars(mount_dir: &Path) -> io::Result<Option<UnmountDrop<Mount>>> {
    if NO_EFI_VARIABLES.load(Ordering::Relaxed) {
        info!("was ordered to not mount the efivars directory");
        Ok(None)
    } else {
        let efivars_source = Path::new("/sys/firmware/efi/efivars");
        let efivars_target = mount_dir.join("sys/firmware/efi/efivars");
        mount_bind_if_exists(&efivars_source, &efivars_target)
    }
}

fn mount_bind_if_exists(source: &Path, target: &Path) -> io::Result<Option<UnmountDrop<Mount>>> {
    if source.exists() {
        let _ = fs::create_dir_all(&target);
        let mount = Mount::builder()
            .fstype("none")
            .flags(MountFlags::BIND)
            .mount_autodrop(source, target, UnmountFlags::empty())?;
        Ok(Some(mount))
    } else {
        Ok(None)
    }
}

/// Replace spaces in OS names as necessary, and rename elementary OS to ubuntu.
fn normalize_os_release_name(name: &str) -> Cow<str> {
    if name.contains(' ') {
        let name = name.replace(' ', "_");
        if &*name == "elementary_OS" {
            Cow::Borrowed("ubuntu")
        } else {
            Cow::Owned(name)
        }
    } else {
        Cow::Borrowed(name)
    }
}
