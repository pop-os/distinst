use super::{DiskError, FileSystemType, PartitionChange as Change, PartitionFlag};
use super::FileSystemType::*;
use super::external::{blockdev, fsck};
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Defines the start and end sectors of a partition on the disk.
pub struct Coordinates {
    start: u64,
    end:   u64,
}

impl Coordinates {
    pub fn new(start: u64, end: u64) -> Coordinates { Coordinates { start, end } }

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
pub struct ResizeOperation {
    sector_size: u64,
    old:         Coordinates,
    new:         Coordinates,
}

/// Defines how many sectors to skip, and how the partition is.
struct OffsetCoordinates {
    skip:   u64,
    offset: i64,
    length: u64,
}

const MEBIBYTE: u64 = 1_048_576;
const MEGABYTE: u64 = 1_000_000;

impl ResizeOperation {
    pub fn new(sector_size: u64, old: Coordinates, new: Coordinates) -> ResizeOperation {
        ResizeOperation {
            sector_size,
            old,
            new,
        }
    }

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

// TODO: Write tests for this function.

/// Performs all move & resize operations for a given partition.
pub(crate) fn resize<DELETE, CREATE>(
    mut change: Change,
    mut resize: ResizeOperation,
    mut delete: DELETE,
    mut create: CREATE,
) -> Result<(), DiskError>
where
    DELETE: FnMut(u32) -> Result<(), DiskError>,
    CREATE: FnMut(u64, u64, Option<FileSystemType>, &[PartitionFlag]) -> Result<(i32, PathBuf), DiskError>,
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
    let (cmd, args, unit, size_before_path): (&str, &[&'static str], ResizeUnit, bool) =
        match change.filesystem {
            // Some(Btrfs) => ("btrfs", &["filesystem", "resize"], false),
            Some(Ext2) | Some(Ext3) | Some(Ext4) => {
                ("resize2fs", &[], ResizeUnit::AbsoluteMebibyte, false)
            }
            // Some(Exfat) => (),
            // Some(F2fs) => ("resize.f2fs"),
            Some(Fat16) | Some(Fat32) => ("fatresize", &["-s"], ResizeUnit::AbsoluteMegabyte, true),
            // Some(Ntfs) => ("ntfsresize", &["-s"], true),
            Some(Swap) => unreachable!("Disk::diff() handles this"),
            // Some(Xfs) => ("xfs_growfs"),
            fs => unimplemented!("{:?} handling", fs),
        };

    // Each file system uses different units for specifying the size, and these units
    // are sometimes written in non-standard and conflicting ways.
    let size = match unit {
        ResizeUnit::AbsoluteMebibyte => format!("{}M", resize.as_absolute_mebibyte()),
        ResizeUnit::AbsoluteMegabyte => format!("{}M", resize.as_absolute_megabyte()),
        ResizeUnit::AbsoluteSectors => format!("{}", resize.absolute_sectors()),
        ResizeUnit::RelativeMebibyte => format!("{}M", resize.as_relative_mebibyte()),
        ResizeUnit::RelativeMegabyte => format!("{}M", resize.as_relative_megabyte()),
        ResizeUnit::RelativeSectors => format!("{}", resize.relative_sectors()),
    };

    // If the partition is shrinking, we will want to shrink before we move.
    // If the partition is growing and moving, we will want to move first, then resize.
    //
    // In addition, the partition in the partition table must be deleted before moving,
    // and recreated with the new size before attempting to grow.
    if shrinking {
        info!("libdistinst: shrinking {}", change.path.display());
        resize_(cmd, args, &size, &change.path, size_before_path)
            .map_err(|why| DiskError::PartitionResize { why })?;
        delete(change.num as u32)?;
        let (num, path) = create(
            resize.new.start,
            resize.new.end,
            change.filesystem,
            &change.flags,
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

            dd(&change.device_path, resize.offset(), change.sector_size)
                .map_err(|why| DiskError::PartitionMove { why })?;

            moving = false;
        }

        let (num, path) = create(
            resize.new.start,
            resize.new.end,
            change.filesystem,
            &change.flags,
        )?;

        change.num = num;
        change.path = path;

        info!("libdistinst: growing {}", change.path.display());
        resize_(cmd, args, &size, &change.path, size_before_path)
            .map_err(|why| DiskError::PartitionResize { why })?;
    }

    // If the partition is to be moved, then we will ensure that it has been deleted,
    // and then dd will be used to move the partition before recreating it in the table.
    if moving {
        info!("libdistinst: moving {}", change.path.display());
        delete(change.num as u32)?;
        let abs_sectors = resize.absolute_sectors();
        resize.old.resize_to(abs_sectors); // TODO: NLL

        dd(&change.device_path, resize.offset(), change.sector_size)
            .map_err(|why| DiskError::PartitionMove { why })?;

        create(
            resize.new.start,
            resize.new.end,
            change.filesystem,
            &change.flags,
        )?;
    }
    Ok(())
}

/// Performs direct reads & writes on the disk to shift a partition either to the left or right,
/// using the supplied offset coordinates to determine where the partition is, and where it
/// should be.
/// An internal operation which may be called twice to move a partition.
fn dd<P: AsRef<Path>>(path: P, coords: OffsetCoordinates, bs: u64) -> io::Result<()> {
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

    // Some dynamic dispatching, based on whether we need to move forward or backwards.
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
        disk.write_all(&mut buffer[..bs as usize])?;
    }

    disk.sync_all()
}

/// Resizes a given partition to a specified size using an external command specific
/// to that file system.
fn resize_<P: AsRef<Path>>(
    cmd: &str,
    args: &[&str],
    size: &str,
    path: P,
    size_before_path: bool,
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
    
    if size_before_path {
        resize_cmd.arg(size);
        resize_cmd.arg(path.as_ref());
    } else {
        resize_cmd.arg(path.as_ref());
        resize_cmd.arg(size);
    }

    eprintln!("{:?}", resize_cmd);

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

    fsck(&path).and_then(|_| {
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
