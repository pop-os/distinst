use super::{DiskError, FileSystemType, PartitionChange as Change, PartitionFlag};
use super::FileSystemType::*;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Command, Stdio};

pub struct Coordinates {
    start: u64,
    end:   u64,
}

impl Coordinates {
    pub fn new(start: u64, end: u64) -> Coordinates { Coordinates { start, end } }
}

pub struct ResizeOperation {
    sector_size: u64,
    old:         Coordinates,
    new:         Coordinates,
    resize_to:   i64,
}

struct Offset {
    offset:  i64,
    inner:   OffsetCoordinates,
    overlap: Option<OffsetCoordinates>,
}

struct OffsetCoordinates {
    skip:   u64,
    length: u64,
}

const MEBIBYTE: u64 = 1_048_576;
const MEGABYTE: u64 = 1_000_000;

impl ResizeOperation {
    pub fn new(sector_size: u64, old: Coordinates, new: Coordinates) -> ResizeOperation {
        // Obtain the differences between the start and end sectors.
        let diff_start = new.start as i64 - old.start as i64;
        let diff_end = new.end as i64 - old.end as i64;

        ResizeOperation {
            sector_size,
            old,
            new,
            resize_to: {
                // If the start position has not changed (diff is 0), then we are only resizing.
                // If the diff between the start & end sectors are the same, we are only moving.
                // Otherwise, the difference between the differences yields the new length.
                if diff_start == 0 {
                    diff_end
                } else if diff_start == diff_end {
                    0
                } else {
                    diff_end - diff_start
                }
            },
        }
    }

    /// Note that the `end` field in the coordinates are actually length values.
    fn offset(&self) -> Offset {
        let offset = self.new.start as i64 - self.old.start as i64;
        if self.new.start > self.old.start {
            Offset {
                offset,
                inner: OffsetCoordinates {
                    skip:   self.old.start,
                    length: offset as u64,
                },
                overlap: Some(OffsetCoordinates {
                    skip:   self.new.start,
                    length: self.old.start - self.new.start,
                }),
            }
        } else {
            Offset {
                offset,
                inner: OffsetCoordinates {
                    skip:   self.old.start,
                    length: self.old.end - self.old.start,
                },
                overlap: None,
            }
        }
    }

    fn is_shrinking(&self) -> bool { self.resize_to < 0 }

    fn is_growing(&self) -> bool { self.resize_to > 0 }

    fn is_moving(&self) -> bool { self.old.start != self.new.start }

    fn absolute_sectors(&self) -> u64 { (self.new.end - self.new.start) * self.sector_size }

    fn relative_sectors(&self) -> i64 { self.resize_to * self.sector_size as i64 }

    fn as_absolute_mebibyte(&self) -> u64 { self.absolute_sectors() / MEBIBYTE }

    fn as_absolute_megabyte(&self) -> u64 { self.absolute_sectors() / MEGABYTE }

    fn as_relative_megabyte(&self) -> i64 { self.relative_sectors() / MEGABYTE as i64 }

    fn as_relative_mebibyte(&self) -> i64 { self.relative_sectors() / MEBIBYTE as i64 }
}

#[allow(dead_code)]
enum ResizeUnit {
    AbsoluteMebibyte,
    AbsoluteMegabyte,
    RelativeMebibyte,
    RelativeMegabyte,
}

// TODO: Write tests for this function.

pub(crate) fn resize<DELETE, CREATE>(
    change: Change,
    resize: ResizeOperation,
    mut delete: DELETE,
    mut create: CREATE,
) -> Result<(), DiskError>
where
    DELETE: FnMut(u32) -> Result<(), DiskError>,
    CREATE: FnMut(u64, u64, Option<FileSystemType>, &[PartitionFlag]) -> Result<(), DiskError>,
{
    let moving = resize.is_moving();
    let shrinking = resize.is_shrinking();
    let growing = resize.is_growing();

    info!(
        "libdistinst: performing move and/or resize operation on {}",
        change.path.display()
    );

    // Create the command and its arguments based on the file system to apply.
    // TODO: Handle the unimplemented file systems.
    let (cmd, args, unit): (&str, &[&'static str], ResizeUnit) = match change.filesystem {
        // Some(Btrfs) => ("btrfs", &["filesystem", "resize"], false),
        Some(Ext2) | Some(Ext3) | Some(Ext4) => {
            ("resize2fs", &["-f"], ResizeUnit::AbsoluteMegabyte)
        }
        // Some(Exfat) => (),
        // Some(F2fs) => ("resize.f2fs"),
        // Some(Fat16) | Some(Fat32) => ("fatresize", &["-s"], true),
        // Some(Ntfs) => ("ntfsresize", &["-s"], true),
        Some(Swap) => unreachable!("Disk::diff() handles this"),
        // Some(Xfs) => ("xfs_growfs"),
        fs => unimplemented!("{:?} handling", fs),
    };

    // TODO: Should we not worry about data if we are going to reformat the partition?

    let new_start = resize.new.start;
    let new_end = resize.new.end;

    macro_rules! recreate {
        () => {
            if !moving {
                create(new_start, new_end, change.filesystem, &change.flags)?;
                false
            } else {
                true
            }
        };
    }

    let size = match unit {
        ResizeUnit::AbsoluteMebibyte => format!("{}M", resize.as_absolute_mebibyte()),
        ResizeUnit::AbsoluteMegabyte => format!("{}M", resize.as_absolute_megabyte()),
        ResizeUnit::RelativeMebibyte => format!("{}M", resize.as_relative_mebibyte()),
        ResizeUnit::RelativeMegabyte => format!("{}M", resize.as_relative_megabyte()),
    };

    // Record whether the partition was deleted & not recreated.
    // If shrinking, resize before deleting; otherwise, delete before resizing.
    let recreated = if shrinking {
        resize_(cmd, args, &size, &change.path).map_err(|why| DiskError::PartitionResize { why })?;
        delete(change.num as u32)?;
        recreate!()
    } else if growing {
        delete(change.num as u32)?;
        resize_(cmd, args, &size, &change.path).map_err(|why| DiskError::PartitionResize { why })?;
        recreate!()
    } else {
        false
    };

    // If the partition is to be moved, then we will ensure that it has been deleted,
    // and then dd will be used to move the partition before recreating it in the table.
    if moving {
        if !recreated {
            delete(change.num as u32)?;
        }

        dd(change.device_path, resize.offset(), change.sector_size)
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

fn dd<P: AsRef<Path>>(path: P, offset: Offset, bs: u64) -> io::Result<()> {
    fn dd_op<P: AsRef<Path>>(
        path: P,
        offset: i64,
        coords: OffsetCoordinates,
        bs: u64,
    ) -> io::Result<()> {
        info!(
            "libdistinst: moving partition on {} (skip: {}; offset: {}; length: {})",
            path.as_ref().display(),
            coords.skip,
            offset,
            coords.length
        );

        info!("libdistinst: opening disk as a file");
        let mut disk = OpenOptions::new().read(true).write(true).open(&path)?;
        let mut buffer = vec![0; bs as usize];

        for index in 0..coords.length {
            let input = bs * coords.skip + index;
            let offset = bs * ((coords.skip as i64 + offset) + index as i64) as u64;

            info!(
                "libdistinst: reading {} bytes from input sector {} on {}",
                bs as usize,
                input,
                path.as_ref().display()
            );

            disk.seek(SeekFrom::Start(input))?;
            disk.read_exact(&mut buffer[..bs as usize])?;

            info!(
                "libdistinst: writing {} bytes to offset sector {} on {}",
                bs as usize,
                offset,
                path.as_ref().display()
            );

            disk.seek(SeekFrom::Start(offset))?;
            disk.write_all(&mut buffer[..bs as usize])?;
        }

        disk.sync_all()
    }

    if let Some(excess) = offset.overlap {
        dd_op(&path, offset.offset, excess, bs)?;
    }

    dd_op(path, offset.offset, offset.inner, bs)?;

    Ok(())
}

fn resize_<P: AsRef<Path>>(cmd: &str, args: &[&str], size: &str, path: P) -> io::Result<()> {
    info!(
        "libdistinst: resizing {} to {}",
        path.as_ref().display(),
        size
    );

    let mut resize_cmd = Command::new(cmd);
    if !args.is_empty() {
        resize_cmd.args(args);
    }
    resize_cmd.arg(path.as_ref());
    resize_cmd.arg(size);

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
}
