use self::FileSystem::*;
use super::{move_partition, BlockCoordinates, OffsetCoordinates, MEBIBYTE, MEGABYTE};
use disk_types::{FileSystem, PartitionType};
use external::{blockdev, fsck};
use libparted::PartitionFlag;
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use sys_mount::*;
use tempdir::TempDir;

/// The size should be before the path argument.
pub const SIZE_BEFORE_PATH: u8 = 0b1;
/// The program does not require a size to be defined.
pub const NO_SIZE: u8 = 0b10;
/// This is a BTRFS partition.
pub const BTRFS: u8 = 0b100;
/// This is a XFS partition.
pub const XFS: u8 = 0b1000;
/// This is a NTFS partition.
pub const NTFS: u8 = 0b10000;

/// Defines the unit of measurement to pass on to resizing tools.
///
/// Some tools require the sector to be defined, others require it by mebibyte or megabyte.
#[allow(dead_code)]
pub enum ResizeUnit {
    AbsoluteBytes,
    AbsoluteKibis,
    AbsoluteMebibyte,
    AbsoluteMegabyte,
    AbsoluteSectors,
    AbsoluteSectorsWithUnit,
}

/// Contains the source and target coordinates for a partition, as well as the size in
/// bytes of each sector. An `OffsetCoordinates` will be calculated from this data structure.
#[derive(new)]
pub struct ResizeOperation {
    pub sector_size: u64,
    pub old:         BlockCoordinates,
    pub new:         BlockCoordinates,
}

impl ResizeOperation {
    /// Calculates the offsets between two coordinates.
    ///
    /// A negative offset means that the partition is moving backwards.
    pub fn offset(&self) -> OffsetCoordinates {
        info!(
            "calculating offsets: {} - {} -> {} - {}",
            self.old.start, self.old.end, self.new.start, self.new.end
        );
        assert!(
            self.old.end - self.old.start == self.new.end - self.new.start,
            "offsets were not adjusted before or after resize operations"
        );

        OffsetCoordinates {
            offset: self.new.start as i64 - self.old.start as i64,
            skip:   self.old.start,
            length: self.old.end - self.old.start,
        }
    }

    pub fn is_shrinking(&self) -> bool { self.relative_sectors() < 0 }

    pub fn is_growing(&self) -> bool { self.relative_sectors() > 0 }

    pub fn is_moving(&self) -> bool { self.old.start != self.new.start }

    pub fn absolute_sectors(&self) -> u64 { self.new.end - self.new.start }

    pub fn relative_sectors(&self) -> i64 {
        // Obtain the differences between the start and end sectors.
        let diff_start = self.new.start as i64 - self.old.start as i64;
        let diff_end = self.new.end as i64 - self.old.end as i64;

        if diff_start == 0 {
            diff_end
        } else if diff_start == diff_end {
            0
        } else {
            diff_end - diff_start
        }
    }

    pub fn as_absolute_mebibyte(&self) -> u64 {
        (self.absolute_sectors() * self.sector_size / MEBIBYTE) - 1
    }

    pub fn as_absolute_megabyte(&self) -> u64 {
        (self.absolute_sectors() * self.sector_size / MEGABYTE) - 1
    }
}

/// Resizes a given partition to a specified size using an external command specific
/// to that file system.
pub fn resize_partition<P: AsRef<Path>>(
    cmd: &str,
    args: &[&str],
    size: &str,
    path: P,
    fs: &str,
    options: u8,
) -> io::Result<()> {
    info!("resizing {} to {}", path.as_ref().display(), size);

    let mut resize_cmd = Command::new(cmd);
    if !args.is_empty() {
        resize_cmd.args(args);
    }

    // Attempt to sync three times before returning an error.
    for attempt in 0..3 {
        ::std::thread::sleep(::std::time::Duration::from_secs(1));
        let result = blockdev(&path, &["--flushbufs"]);
        if result.is_err() && attempt == 2 {
            result?;
        } else {
            break;
        }
    }

    let fsck_options = if options & BTRFS != 0 {
        Some(("btrfsck", "--repair"))
    } else if options & XFS != 0 {
        Some(("xfs_repair", "-v"))
    } else {
        None
    };

    fsck(path.as_ref(), fsck_options).and_then(|_| {
        // Btrfs is a strange case that needs resize operations to be performed while
        // it is mounted.
        let (npath, _mount) = if options & (BTRFS | XFS) != 0 {
            let temp = TempDir::new("distinst")?;
            info!("temporarily mounting {} to {}", path.as_ref().display(), temp.path().display());
            let mount = Mount::new(path.as_ref(), temp.path(), fs, MountFlags::empty(), None)?;
            let mount = mount.into_unmount_drop(UnmountFlags::DETACH);
            (temp.path().to_path_buf(), Some((mount, temp)))
        } else {
            (path.as_ref().to_path_buf(), None)
        };

        if options & NO_SIZE != 0 {
            resize_cmd.arg(&npath);
        } else if options & SIZE_BEFORE_PATH != 0 {
            resize_cmd.arg(size).arg(&npath);
        } else {
            resize_cmd.arg(&npath).arg(size);
        };

        let status = if options & NTFS != 0 {
            ntfs_dry_run(&npath, size)?;

            info!("executing {:?}", resize_cmd);
            let mut child = resize_cmd.stdin(Stdio::piped()).spawn()?;
            child.stdin.as_mut().expect("failed to get stdin").write_all(b"y\n")?;
            child.wait()?
        } else {
            info!("executing {:?}", resize_cmd);
            resize_cmd.status()?
        };

        if status.success() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("resize for {:?} failed with status: {}", path.as_ref().display(), status),
            ))
        }
    })
}

/// Defines the move and resize operations that the partition with this number
/// will need to perform.
///
/// If the `start` sector differs from the source, the partition will be moved.
/// If the `end` minus the `start` differs from the length of the source, the
/// partition will be resized. Once partitions have been moved and resized,
/// they will be formatted accordingly, if formatting was set.
///
/// # Note
///
/// Resize operations should always be performed before move operations.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionChange {
    /// The location of the device where the partition resides.
    pub device_path: PathBuf,
    /// The location of the partition in the system.
    pub path:        PathBuf,
    /// The partition ID that will be changed.
    pub num:         i32,
    /// Defines whether this is a Primary or Logical partition.
    pub kind:        PartitionType,
    /// The start sector that the partition will have.
    pub start:       u64,
    /// The end sector that the partition will have.
    pub end:         u64,
    /// The file system that is currently on the partition.
    pub filesystem:  Option<FileSystem>,
    /// A diff of flags which should be set on the partition.
    pub flags:       Vec<PartitionFlag>,
    /// All of the flags that are set on the new disk.
    pub new_flags:   Vec<PartitionFlag>,
    /// Defines the label to apply
    pub label:       Option<String>,
}

/// Performs all move & resize operations for a given partition.
pub fn transform<DELETE, CREATE>(
    mut change: PartitionChange,
    mut resize: ResizeOperation,
    mut delete: DELETE,
    mut create: CREATE,
) -> io::Result<()>
where
    DELETE: FnMut(u32) -> io::Result<()>,
    CREATE: FnMut(
        u64,
        u64,
        Option<FileSystem>,
        Vec<PartitionFlag>,
        Option<String>,
        PartitionType,
    ) -> io::Result<(i32, PathBuf)>,
{
    let mut moving = resize.is_moving();
    let shrinking = resize.is_shrinking();
    let growing = resize.is_growing();

    info!(
        "resize operations: {{ moving: {}, shrinking: {}, growing: {} }}",
        moving, shrinking, growing
    );

    // Create the command and its arguments based on the file system to apply.
    // TODO: Handle the unimplemented file systems.
    let (cmd, args, unit, opts): (&str, &[&'static str], ResizeUnit, u8) = match change.filesystem {
        Some(Btrfs) => (
            "btrfs",
            &["filesystem", "resize"],
            ResizeUnit::AbsoluteMebibyte,
            BTRFS | SIZE_BEFORE_PATH,
        ),
        Some(Ext2) | Some(Ext3) | Some(Ext4) => {
            ("resize2fs", &[], ResizeUnit::AbsoluteSectorsWithUnit, 0)
        }
        // Some(Exfat) => (),
        // Some(F2fs) => ("resize.f2fs"),
        Some(Fat16) | Some(Fat32) => {
            ("fatresize", &["-s"], ResizeUnit::AbsoluteKibis, SIZE_BEFORE_PATH)
        }
        Some(Ntfs) => (
            "ntfsresize",
            &["--force", "--force", "-s"],
            ResizeUnit::AbsoluteBytes,
            SIZE_BEFORE_PATH | NTFS,
        ),
        Some(Swap) => unreachable!("Disk::diff() handles this"),
        Some(Xfs) => {
            if shrinking {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "XFS partitions do not support shrinking",
                ));
            }

            ("xfs_growfs", &["-d"], ResizeUnit::AbsoluteMegabyte, NO_SIZE | XFS)
        }
        fs => unimplemented!("{:?} handling", fs),
    };

    let fs = match change.filesystem {
        Some(Fat16) | Some(Fat32) => "vfat",
        Some(fs) => fs.into(),
        None => "none",
    };

    // Each file system uses different units for specifying the size, and these
    // units are sometimes written in non-standard and conflicting ways.
    let size = match unit {
        ResizeUnit::AbsoluteBytes => format!("{}", resize.absolute_sectors() * 512),
        ResizeUnit::AbsoluteKibis => format!("{}ki", resize.absolute_sectors() / 2),
        ResizeUnit::AbsoluteSectorsWithUnit => format!("{}s", resize.absolute_sectors()),
        ResizeUnit::AbsoluteMebibyte => format!("{}M", resize.as_absolute_mebibyte()),
        ResizeUnit::AbsoluteMegabyte => format!("{}M", resize.as_absolute_megabyte()),
        ResizeUnit::AbsoluteSectors => format!("{}", resize.absolute_sectors()),
    };

    // If the partition is shrinking, we will want to shrink before we move.
    // If the partition is growing and moving, we will want to move first, then
    // resize.
    //
    // In addition, the partition in the partition table must be deleted before
    // moving, and recreated with the new size before attempting to grow.
    if shrinking {
        info!("shrinking {}", change.path.display());
        resize_partition(cmd, args, &size, &change.path, fs, opts).map_err(|why| {
            io::Error::new(
                why.kind(),
                format!("failed to shrink {}: {}", change.path.display(), why),
            )
        })?;

        delete(change.num as u32)?;
        let (num, path) = create(
            resize.new.start,
            resize.new.end,
            change.filesystem,
            change.new_flags.clone(),
            change.label.clone(),
            change.kind,
        )?;

        change.num = num;
        change.path = path;
    } else if growing {
        delete(change.num as u32)?;
        if resize.new.start != resize.old.start {
            info!("moving before growing {}", change.path.display());
            let abs_sectors = resize.absolute_sectors();
            resize.old.resize_to(abs_sectors); // TODO: NLL

            move_partition(&change.device_path, resize.offset(), 512).map_err(|why| {
                io::Error::new(
                    why.kind(),
                    format!("failed to move partition at {}: {}", change.path.display(), why),
                )
            })?;

            moving = false;
        }

        let (num, path) = create(
            resize.new.start,
            resize.new.end,
            change.filesystem,
            change.new_flags.clone(),
            change.label.clone(),
            change.kind,
        )?;

        change.num = num;
        change.path = path;

        info!("growing {}", change.path.display());
        resize_partition(cmd, args, &size, &change.path, fs, opts).map_err(|why| {
            io::Error::new(
                why.kind(),
                format!("failed to resize partition at {}: {}", change.path.display(), why),
            )
        })?;
    }

    // If the partition is to be moved, then we will ensure that it has been
    // deleted, and then dd will be used to move the partition before
    // recreating it in the table.
    if moving {
        info!("moving {}", change.path.display());
        delete(change.num as u32)?;
        let abs_sectors = resize.absolute_sectors();
        resize.old.resize_to(abs_sectors); // TODO: NLL

        move_partition(&change.device_path, resize.offset(), 512).map_err(|why| {
            io::Error::new(
                why.kind(),
                format!("failed to move partition at {}: {}", change.path.display(), why),
            )
        })?;

        create(
            resize.new.start,
            resize.new.end,
            change.filesystem,
            change.new_flags,
            change.label,
            change.kind,
        )?;
    }

    Ok(())
}

fn ntfs_dry_run(path: &Path, size: &str) -> io::Result<()> {
    let mut consistency_check = Command::new("ntfsresize");
    consistency_check.args(&["-f", "-f", "--no-action", "-s"]).arg(size).arg(path);

    info!("executing {:?}", consistency_check);
    let mut child = consistency_check.stdin(Stdio::piped()).spawn()?;
    child.stdin.as_mut().expect("failed to get stdin").write_all(b"y\n")?;

    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, format!("ntfsresize exited with {:?}", status)))
    }
}
