use super::external::{blockdev, fsck};
use super::mount::Mount;
use super::FileSystemType::*;
use super::{DiskError, FileSystemType, PartitionChange as Change, PartitionFlag, PartitionType};
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempdir::TempDir;

/// Defines the start and end sectors of a partition on the disk.
#[derive(new)]
pub struct Coordinates {
    start: u64,
    end:   u64,
}

impl Coordinates {
    /// Modifies the coordinates based on the new length that is supplied. Coordinates
    /// will be adjusted automatically based on whether the partition is shrinking or
    /// growing.
    pub fn resize_to(&mut self, new_len: u64) {
        let offset = (self.end - self.start) as i64 - new_len as i64;
        if offset < 0 {
            self.end += offset.abs() as u64;
        } else {
            self.end -= offset as u64;
        }
    }
}

/// Contains the source and target coordinates for a partition, as well as the size in
/// bytes of each sector. An `OffsetCoordinates` will be calculated from this data structure.
#[derive(new)]
pub struct ResizeOperation {
    sector_size: u64,
    old:         Coordinates,
    new:         Coordinates,
}

/// Defines how many sectors to skip, and how the partition is.
#[derive(Clone, Copy)]
struct OffsetCoordinates {
    skip:   u64,
    offset: i64,
    length: u64,
}

const MEBIBYTE: u64 = 1_048_576;
const MEGABYTE: u64 = 1_000_000;

impl ResizeOperation {
    /// Calculates the offsets between two coordinates.
    ///
    /// A negative offset means that the partition is moving backwards.
    fn offset(&self) -> OffsetCoordinates {
        info!(
            "libdistinst: calculating offsets: {} - {} -> {} - {}",
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

    fn is_shrinking(&self) -> bool { self.relative_sectors() < 0 }

    fn is_growing(&self) -> bool { self.relative_sectors() > 0 }

    fn is_moving(&self) -> bool { self.old.start != self.new.start }

    fn absolute_sectors(&self) -> u64 { (self.new.end - self.new.start) }

    fn relative_sectors(&self) -> i64 {
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

    fn as_absolute_mebibyte(&self) -> u64 { self.absolute_sectors() * self.sector_size / MEBIBYTE }

    fn as_absolute_megabyte(&self) -> u64 { self.absolute_sectors() * self.sector_size / MEGABYTE }

    fn as_relative_megabyte(&self) -> i64 {
        self.relative_sectors() * self.sector_size as i64 / MEGABYTE as i64
    }

    fn as_relative_mebibyte(&self) -> i64 {
        self.relative_sectors() * self.sector_size as i64 / MEBIBYTE as i64
    }
}

#[allow(dead_code)]
enum ResizeUnit {
    AbsoluteMebibyte,
    AbsoluteMegabyte,
    AbsoluteSectors,
    RelativeMebibyte,
    RelativeMegabyte,
    RelativeSectors,
}

const SIZE_BEFORE_PATH: u8 = 0b1;
const NO_SIZE: u8 = 0b10;
const BTRFS: u8 = 0b100;
const XFS: u8 = 0b1000;

// TODO: Write tests for this function.

/// Performs all move & resize operations for a given partition.
pub(crate) fn transform<DELETE, CREATE>(
    mut change: Change,
    mut resize: ResizeOperation,
    mut delete: DELETE,
    mut create: CREATE,
) -> Result<(), DiskError>
where
    DELETE: FnMut(u32) -> Result<(), DiskError>,
    CREATE:
        FnMut(u64, u64, Option<FileSystemType>, Vec<PartitionFlag>, Option<String>, PartitionType)
            -> Result<(i32, PathBuf), DiskError>,
{
    let mut moving = resize.is_moving();
    let shrinking = resize.is_shrinking();
    let growing = resize.is_growing();

    info!(
        "libdistinst: resize operations: {{ moving: {}, shrinking: {}, growing: {} }}",
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
        Some(Ext2) | Some(Ext3) | Some(Ext4) => ("resize2fs", &[], ResizeUnit::AbsoluteMebibyte, 0),
        // Some(Exfat) => (),
        // Some(F2fs) => ("resize.f2fs"),
        Some(Fat16) | Some(Fat32) => (
            "fatresize",
            &["-s"],
            ResizeUnit::AbsoluteMegabyte,
            SIZE_BEFORE_PATH,
        ),
        Some(Ntfs) => (
            "ntfsresize",
            &["--force", "--force", "-s"],
            ResizeUnit::AbsoluteMegabyte,
            SIZE_BEFORE_PATH,
        ),
        Some(Swap) => unreachable!("Disk::diff() handles this"),
        Some(Xfs) => {
            if shrinking {
                return Err(DiskError::UnsupportedShrinking);
            }

            (
                "xfs_growfs",
                &["-d"],
                ResizeUnit::AbsoluteMegabyte,
                NO_SIZE | XFS,
            )
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
        ResizeUnit::AbsoluteMebibyte => format!("{}M", resize.as_absolute_mebibyte()),
        ResizeUnit::AbsoluteMegabyte => format!("{}M", resize.as_absolute_megabyte()),
        ResizeUnit::AbsoluteSectors => format!("{}", resize.absolute_sectors()),
        ResizeUnit::RelativeMebibyte => format!("{}M", resize.as_relative_mebibyte()),
        ResizeUnit::RelativeMegabyte => format!("{}M", resize.as_relative_megabyte()),
        ResizeUnit::RelativeSectors => format!("{}", resize.relative_sectors()),
    };

    // If the partition is shrinking, we will want to shrink before we move.
    // If the partition is growing and moving, we will want to move first, then
    // resize.
    //
    // In addition, the partition in the partition table must be deleted before
    // moving, and recreated with the new size before attempting to grow.
    if shrinking {
        info!("libdistinst: shrinking {}", change.path.display());
        resize_partition(cmd, args, &size, &change.path, fs, opts)
            .map_err(|why| DiskError::PartitionResize { why })?;
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
            info!(
                "libdistinst: moving before growing {}",
                change.path.display()
            );
            let abs_sectors = resize.absolute_sectors();
            resize.old.resize_to(abs_sectors); // TODO: NLL

            move_partition(&change.device_path, resize.offset(), change.sector_size)
                .map_err(|why| DiskError::PartitionMove { why })?;

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

        info!("libdistinst: growing {}", change.path.display());
        resize_partition(cmd, args, &size, &change.path, fs, opts)
            .map_err(|why| DiskError::PartitionResize { why })?;
    }

    // If the partition is to be moved, then we will ensure that it has been
    // deleted, and then dd will be used to move the partition before
    // recreating it in the table.
    if moving {
        info!("libdistinst: moving {}", change.path.display());
        delete(change.num as u32)?;
        let abs_sectors = resize.absolute_sectors();
        resize.old.resize_to(abs_sectors); // TODO: NLL

        move_partition(&change.device_path, resize.offset(), change.sector_size)
            .map_err(|why| DiskError::PartitionMove { why })?;

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

/// Performs direct reads & writes on the disk to shift a partition either to the left or right,
/// using the supplied offset coordinates to determine where the partition is, and where it
/// should be.
fn move_partition<P: AsRef<Path>>(path: P, coords: OffsetCoordinates, bs: u64) -> io::Result<()> {
    info!(
        "libdistinst: moving partition on {} with {} sector size: {{ skip: {}; offset: {}; \
         length: {} }}",
        path.as_ref().display(),
        bs as usize,
        coords.skip,
        coords.offset,
        coords.length
    );

    let mut disk = OpenOptions::new().read(true).write(true).open(&path)?;

    let source_skip = coords.skip;
    let offset_skip = (source_skip as i64 + coords.offset) as u64;

    info!(
        "libdistinst: source sector: {}; offset sector: {}",
        source_skip, offset_skip
    );

    // TODO: Rather than writing one sector at a time, use a significantly larger
    // buffer size to improve the performance of partition moves.
    let mut buffer = vec![0; bs as usize];

    // Some dynamic dispatching, based on whether we need to move forward or
    // backwards.
    let range: Box<Iterator<Item = u64>> = if coords.offset > 0 {
        Box::new((0..coords.length).rev())
    } else {
        Box::new(0..coords.length)
    };

    // Write one sector at a time, until all sectors have been moved.
    for sector in range {
        let input = source_skip + sector;
        disk.seek(SeekFrom::Start(input * bs))?;
        disk.read_exact(&mut buffer[..bs as usize])?;

        let offset = offset_skip + sector;
        disk.seek(SeekFrom::Start(offset * bs))?;
        disk.write_all(&buffer[..bs as usize])?;
    }

    disk.sync_all()
}

/// Resizes a given partition to a specified size using an external command specific
/// to that file system.
fn resize_partition<P: AsRef<Path>>(
    cmd: &str,
    args: &[&str],
    size: &str,
    path: P,
    fs: &str,
    options: u8,
) -> io::Result<()> {
    info!(
        "libdistinst: resizing {} to {}",
        path.as_ref().display(),
        size
    );

    let mut resize_cmd = Command::new(cmd);
    if !args.is_empty() {
        resize_cmd.args(args);
    }

    // Attempt to sync three times before returning an error.
    for attempt in 0..3 {
        ::std::thread::sleep(::std::time::Duration::from_secs(1));
        let result = blockdev(&path, &["--flushbufs", "--rereadpt"]);
        if result.is_err() && attempt == 2 {
            result.map_err(|why| DiskError::DiskSync { why })?
        } else {
            break;
        }
    }

    fsck(
        path.as_ref(),
        if options & BTRFS != 0 {
            Some(("btrfsck", "--repair"))
        } else if options & XFS != 0 {
            Some(("xfs_repair", "-v"))
        } else {
            None
        },
    ).and_then(|_| {
        // Btrfs is a strange case that needs resize operations to be performed while
        // it is mounted.
        let (npath, _mount) = if options & (BTRFS | XFS) != 0 {
            let temp = TempDir::new("distinst")?;
            info!(
                "libdistinst: temporarily mounting {} to {}",
                path.as_ref().display(),
                temp.path().display()
            );
            let mount = Mount::new(path.as_ref(), temp.path(), fs, 0, None)?;
            (temp.path().to_path_buf(), Some((mount, temp)))
        } else {
            (path.as_ref().to_path_buf(), None)
        };

        if options & NO_SIZE != 0 {
            resize_cmd.arg(&npath);
        } else if options & SIZE_BEFORE_PATH != 0 {
            resize_cmd.arg(size);
            resize_cmd.arg(&npath);
        } else {
            resize_cmd.arg(&npath);
            resize_cmd.arg(size);
        }

        eprintln!("{:?}", resize_cmd);
        let status = resize_cmd.stdout(Stdio::null()).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "resize for {:?} failed with status: {}",
                    path.as_ref().display(),
                    status
                ),
            ))
        }
    })
}
