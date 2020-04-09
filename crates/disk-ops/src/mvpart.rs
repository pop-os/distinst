use super::OffsetCoordinates;
use std::{
    fs::OpenOptions,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

/// Performs direct reads & writes on the disk to shift a partition either to the left or right,
/// using the supplied offset coordinates to determine where the partition is, and where it
/// should be.
pub fn move_partition<P: AsRef<Path>>(
    path: P,
    coords: OffsetCoordinates,
    bs: u64,
) -> io::Result<()> {
    info!(
        "moving partition on {} with {} sector size: {{ skip: {}; offset: {}; length: {} }}",
        path.as_ref().display(),
        bs as usize,
        coords.skip,
        coords.offset,
        coords.length
    );

    let mut disk = OpenOptions::new().read(true).write(true).open(&path)?;

    let source_skip = coords.skip;
    let offset_skip = (source_skip as i64 + coords.offset) as u64;

    // TODO: Rather than writing one sector at a time, use a significantly larger
    // buffer size to improve the performance of partition moves.
    let mut buffer = vec![0; bs as usize];

    // Some dynamic dispatching, based on whether we need to move forward or
    // backwards.
    let range: Box<dyn Iterator<Item = u64>> = if coords.offset > 0 {
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
