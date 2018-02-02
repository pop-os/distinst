use super::{DiskError, FileSystemType, PartitionChange as Change, PartitionFlag};
use super::FileSystemType::*;
use std::fs::File;
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
    inner:   OffsetCoordinates,
    overlap: Option<OffsetCoordinates>,
}

struct OffsetCoordinates {
    skip:   u64,
    offset: i64,
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
        if self.old.end > self.new.start {
            Offset {
                inner:   OffsetCoordinates {
                    skip: self.old.start,
                    offset,
                    length: self.new.start - self.old.start,
                },
                overlap: Some(OffsetCoordinates {
                    skip: self.new.start,
                    offset,
                    length: self.new.start - self.old.start,
                }),
            }
        } else {
            Offset {
                inner:   OffsetCoordinates {
                    skip: self.old.start,
                    offset,
                    length: self.old.end - self.old.start,
                },
                overlap: None,
            }
        }
    }

    fn is_shrinking(&self) -> bool { self.resize_to < 0 }

    fn is_growing(&self) -> bool { self.resize_to > 0 }

    fn is_moving(&self) -> bool { self.old.start != self.new.start }

    fn resize_as_megabyte(&self) -> i64 {
        (self.resize_to * self.sector_size as i64) / MEGABYTE as i64
    }

    fn resize_as_mebibyte(&self) -> i64 {
        (self.resize_to * self.sector_size as i64) / MEBIBYTE as i64
    }
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
    // Due to different tools using different standards, this workaround is needed.
    let megabyte = format!("{}M", resize.resize_as_megabyte());
    let megabyte = megabyte.as_str();
    let mebibyte = format!("{}M", resize.resize_as_mebibyte());
    let mebibyte = mebibyte.as_str();

    let moving = resize.is_moving();
    let shrinking = resize.is_shrinking();
    let growing = resize.is_growing();

    // Create the command and its arguments based on the file system to apply.
    // TODO: Handle the unimplemented file systems.
    let (cmd, args, uses_megabyte): (&str, &[&'static str], bool) = match change.format {
        Some(Btrfs) => ("btrfs", &["filesystem", "resize"], false),
        Some(Ext2) | Some(Ext3) | Some(Ext4) => ("resize2fs", &["-f"], false),
        // Some(Exfat) => (),
        // Some(F2fs) => ("resize.f2fs"),
        Some(Fat16) | Some(Fat32) => ("fatresize", &["-s"], true),
        // Some(Ntfs) => ("ntfsresize", &["-s"], true),
        Some(Swap) => unreachable!("Disk::diff() handles this"),
        // Some(Xfs) => ("xfs_growfs"),
        _ => unimplemented!(),
    };

    // TODO: Should we not worry about data if we are going to reformat the partition?

    macro_rules! recreate {
        () => {
            if !moving {
                create(resize.new.start, resize.new.end, change.format, &change.flags)?;
                false
            } else {
                true
            }
        };
    }

    // Record whether the partition was deleted & not recreated.
    // If shrinking, resize before deleting; otherwise, delete before resizing.
    let recreated = if shrinking {
        resize_(
            cmd,
            args,
            if uses_megabyte { &megabyte } else { &mebibyte },
            &change.path,
        ).map_err(|_| DiskError::PartitionResize)?;
        delete(change.num as u32)?;
        recreate!()
    } else if growing {
        delete(change.num as u32)?;
        resize_(
            cmd,
            args,
            if uses_megabyte { &megabyte } else { &mebibyte },
            &change.path,
        ).map_err(|_| DiskError::PartitionResize)?;
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

        dd(change.path, resize.offset(), change.sector_size)
            .map_err(|_| DiskError::PartitionResize)?;

        create(
            resize.new.start,
            resize.new.end,
            change.format,
            &change.flags,
        )?;
    }

    Ok(())
}

fn dd<P: AsRef<Path>>(path: P, offset: Offset, bs: u64) -> io::Result<()> {
    fn dd_op<P: AsRef<Path>>(path: P, coords: OffsetCoordinates, bs: u64) -> io::Result<()> {
        info!(
            "libdistinst: moving partition on {} (skip: {}; offset: {}; length: {})",
            path.as_ref().display(),
            coords.skip,
            coords.offset,
            coords.length
        );

        let mut in_file = File::open(&path)?;
        let mut out_file = File::open(&path)?;

        let mut buffer = vec![0; bs as usize];
        in_file.seek(SeekFrom::Start(bs * coords.skip))?;
        out_file.seek(SeekFrom::Start(
            bs * (coords.skip as i64 + coords.offset) as u64,
        ))?;

        for _ in 0..coords.length {
            in_file.read_exact(&mut buffer[..bs as usize])?;
            out_file.write(&mut buffer[..bs as usize])?;
        }

        out_file.sync_all()
    }

    if let Some(excess) = offset.overlap {
        dd_op(&path, excess, bs)?;
    }

    dd_op(path, offset.inner, bs)?;

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
    resize_cmd.arg(size);
    resize_cmd.arg(path.as_ref());

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
