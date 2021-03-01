use crate::fs::FileSystem;
use std::{
    io::{self, BufRead, Cursor},
    path::Path,
    process::{Command, Stdio},
};

/// Executes a given file system's dump command to obtain the minimum shrink
/// size
pub fn sectors_used<P: AsRef<Path>>(part: P, fs: FileSystem) -> io::Result<u64> {
    use self::FileSystem::*;
    match fs {
        Ext2 | Ext3 | Ext4 => {
            let reader = Cursor::new(
                Command::new("dumpe2fs")
                    .arg("-h")
                    .arg(part.as_ref())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()?
                    .stdout,
            );

            get_ext4_usage(reader.lines().skip(1))
        }
        Fat16 | Fat32 => {
            let mut cmd = Command::new("fsck.fat")
                .arg("-nv")
                .arg(part.as_ref())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()?;

            if !cmd.status.success() {
                // If a failure occurred, try to correct any fixable errors.
                Command::new("fsck.fat")
                    .arg("-fy")
                    .arg(part.as_ref())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()?;

                // Then re-run the fsck command to get the status again.
                cmd = Command::new("fsck.fat")
                    .arg("-nv")
                    .arg(part.as_ref())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()?;
            }

            let reader = Cursor::new(cmd.stdout);
            get_fat_usage(reader.lines().skip(1))
        }
        Ntfs => {
            let cmd = Command::new("ntfsresize")
                .arg("--info")
                .arg("--force")
                .arg("--no-progress-bar")
                .arg(part.as_ref())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()?;

            let reader = Cursor::new(cmd.stdout).lines().skip(1);
            if cmd.status.success() {
                get_ntfs_usage(reader)
            } else {
                get_ntfs_size(reader)
            }
        }
        Btrfs => {
            let cmd = Command::new("btrfs")
                .arg("filesystem")
                .arg("show")
                .arg(part.as_ref())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()?;

            let reader = Cursor::new(cmd.stdout).lines().skip(1);
            get_btrfs_usage(reader)
        }
        _ => Err(io::Error::new(io::ErrorKind::NotFound, "unsupported file system")),
    }
}

fn get_btrfs_usage<R: Iterator<Item = io::Result<String>>>(mut reader: R) -> io::Result<u64> {
    parse_field_as_unit(&mut reader, "Total devices", 6).map(|used| used / 512)
}

fn get_ext4_usage<R: Iterator<Item = io::Result<String>>>(mut reader: R) -> io::Result<u64> {
    let total_blocks = parse_field(&mut reader, "Block count:", 2)?;
    let free_blocks = parse_field(&mut reader, "Free blocks:", 2)?;
    let block_size = parse_field(&mut reader, "Block size:", 2)?;
    Ok(((total_blocks - free_blocks) * block_size) / 512)
}

fn get_ntfs_usage<R: Iterator<Item = io::Result<String>>>(mut reader: R) -> io::Result<u64> {
    parse_field(&mut reader, "You might resize at", 4)
        .map(|bytes| (bytes + (2 * 1024 * 1024)) / 512)
}

fn get_ntfs_size<R: Iterator<Item = io::Result<String>>>(mut reader: R) -> io::Result<u64> {
    parse_field(&mut reader, "Current volume size", 3).map(|bytes| bytes / 512)
}

fn get_fat_usage<R: Iterator<Item = io::Result<String>>>(mut reader: R) -> io::Result<u64> {
    let cluster_size = parse_fsck_field(&mut reader, "bytes per cluster")?;
    let (used, _) = parse_fsck_cluster_summary(&mut reader)?;
    Ok((used * cluster_size) / 512)
}

fn parse_fsck_field<R: Iterator<Item = io::Result<String>>>(
    reader: &mut R,
    end: &str,
) -> io::Result<u64> {
    loop {
        match reader.next() {
            Some(line) => {
                let line = line?;
                let line = line.trim();
                if line.ends_with(end) {
                    match line.split_whitespace().next().map(|v| v.parse::<u64>()) {
                        Some(Ok(value)) => break Ok(value),
                        _ => {
                            break Err(io::Error::new(io::ErrorKind::Other, "invalid dump output"))
                        }
                    }
                }
            }
            None => {
                break Err(io::Error::new(io::ErrorKind::Other, "invalid dump output: EOF"));
            }
        }
    }
}

fn parse_fsck_cluster_summary<R: Iterator<Item = io::Result<String>>>(
    reader: &mut R,
) -> io::Result<(u64, u64)> {
    loop {
        match reader.next() {
            Some(line) => {
                let line = line?;
                if line.split_whitespace().next().map_or(false, |word| word.ends_with(':')) {
                    if let Some(stats) = line.split_whitespace().nth(3) {
                        if let Some(id) = stats.find('/') {
                            if stats.len() > id + 1 {
                                if let Ok(used) = stats[..id].parse::<u64>() {
                                    if let Ok(total) = stats[id + 1..].parse::<u64>() {
                                        break Ok((used, total));
                                    }
                                }
                            }
                        }
                    }

                    break Err(io::Error::new(io::ErrorKind::Other, "invalid dump output"));
                }
            }
            None => {
                break Err(io::Error::new(io::ErrorKind::Other, "invalid dump output: EOF"));
            }
        }
    }
}

fn parse_field<R: Iterator<Item = io::Result<String>>>(
    reader: &mut R,
    field: &str,
    value: usize,
) -> io::Result<u64> {
    for line in reader {
        let line = line?;
        if line.starts_with(field) {
            match line.split_whitespace().nth(value).map(|v| v.parse::<u64>()) {
                Some(Ok(value)) => return Ok(value),
                _ => return Err(io::Error::new(io::ErrorKind::Other, "invalid usage field")),
            }
        }
    }

    Err(io::Error::new(io::ErrorKind::Other, "invalid usage output"))
}

fn parse_unit(unit: &str) -> io::Result<u64> {
    let (value, unit) = unit.split_at(unit.len() - 3);
    eprintln!("Value: {}, unit: {}", value, unit);
    let value = match value.parse::<f64>() {
        Ok(value) => value,
        Err(why) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("invalid unit value: {}", why),
            ));
        }
    };

    match unit {
        "KiB" => Ok((value * 1024f64) as u64),
        "MiB" => Ok((value * 1024f64 * 1024f64) as u64),
        "GiB" => Ok((value * 1024f64 * 1024f64 * 1024f64) as u64),
        "TiB" => Ok((value * 1024f64 * 1024f64 * 1024f64 * 1024f64) as u64),
        _ => Err(io::Error::new(io::ErrorKind::Other, format!("invalid unit type: {}", unit))),
    }
}

fn parse_field_as_unit<R: Iterator<Item = io::Result<String>>>(
    reader: &mut R,
    field: &str,
    value: usize,
) -> io::Result<u64> {
    for line in reader {
        let line = line?;
        let line = line.trim_start();
        if line.starts_with(field) {
            match line.split_whitespace().nth(value) {
                Some(value) => {
                    let value = parse_unit(value)?;
                    return Ok(value);
                }
                None => return Err(io::Error::new(io::ErrorKind::Other, "invalid usage field")),
            }
        }
    }

    Err(io::Error::new(io::ErrorKind::Other, "invalid usage output"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAT_INPUT: &str = r#"fsck.fat 4.1 (2017-01-24)
Checking we can access the last sector of the filesystem
Boot sector contents:
System ID "mkfs.fat"
Media byte 0xf8 (hard disk)
       512 bytes per logical sector
      4096 bytes per cluster
        32 reserved sectors
First FAT starts at byte 16384 (sector 32)
         2 FATs, 32 bit entries
   1048576 bytes per FAT (= 2048 sectors)
Root directory start at cluster 2 (arbitrary size)
Data area starts at byte 2113536 (sector 4128)
    261628 data clusters (1071628288 bytes)
63 sectors/track, 255 heads
      2048 hidden sectors
   2097152 sectors total
Checking for unused clusters.
Checking free cluster summary.
/dev/sdb1: 0 files, 1/261628 clusters"#;

    const FAT2_INPUT: &str = r#"fsck.fat 4.1 (2017-01-24)
Checking we can access the last sector of the filesystem
Boot sector contents:
System ID "mkfs.fat"
Media byte 0xf8 (hard disk)
       512 bytes per logical sector
      4096 bytes per cluster
        32 reserved sectors
First FAT starts at byte 16384 (sector 32)
         2 FATs, 32 bit entries
    524288 bytes per FAT (= 1024 sectors)
Root directory start at cluster 2 (arbitrary size)
Data area starts at byte 1064960 (sector 2080)
    130812 data clusters (535805952 bytes)
63 sectors/track, 255 heads
      2048 hidden sectors
   1048576 sectors total
Checking for unused clusters.
Checking free cluster summary.
/dev/sda1: 31 files, 66356/130812 clusters
"#;

    const FAT3_INPUT: &str = r#"fsck.fat 4.1 (2017-01-24)
Checking we can access the last sector of the filesystem
Boot sector contents:
System ID "mkfs.fat"
Media byte 0xf8 (hard disk)
       512 bytes per logical sector
      8192 bytes per cluster
        16 reserved sectors
First FAT starts at byte 8192 (sector 16)
         2 FATs, 16 bit entries
    131072 bytes per FAT (= 256 sectors)
Root directory starts at byte 270336 (sector 528)
       512 root directory entries
Data area starts at byte 286720 (sector 560)
     65501 data clusters (536584192 bytes)
63 sectors/track, 255 heads
      2048 hidden sectors
   1048576 sectors total
Checking for unused clusters.
/dev/sda1: 35 files, 24176/65501 clusters
"#;

    #[test]
    fn fat_parsing() {
        let mut reader = FAT_INPUT.lines().map(|x| Ok(x.into()));
        assert_eq!(parse_fsck_field(&mut reader, "bytes per cluster").unwrap(), 4096);
        assert_eq!(parse_fsck_cluster_summary(&mut reader).unwrap(), (1, 261628));
    }

    #[test]
    fn fat_parsing2() {
        let mut reader = FAT2_INPUT.lines().map(|x| Ok(x.into()));
        assert_eq!(parse_fsck_field(&mut reader, "bytes per cluster").unwrap(), 4096);
        assert_eq!(parse_fsck_cluster_summary(&mut reader).unwrap(), (66356, 130812));
    }

    #[test]
    fn fat_parsing3() {
        let mut reader = FAT3_INPUT.lines().map(|x| Ok(x.into()));
        assert_eq!(parse_fsck_field(&mut reader, "bytes per cluster").unwrap(), 8192);
        assert_eq!(parse_fsck_cluster_summary(&mut reader).unwrap(), (24176, 65501));
    }

    #[test]
    fn fat_usage() {
        assert_eq!(get_fat_usage(FAT_INPUT.lines().map(|x| Ok(x.into()))).unwrap(), 8);
    }

    #[test]
    fn fat_usage2() {
        assert_eq!(get_fat_usage(FAT2_INPUT.lines().map(|x| Ok(x.into()))).unwrap(), 530848);
    }

    #[test]
    fn fat_usage3() {
        assert_eq!(get_fat_usage(FAT3_INPUT.lines().map(|x| Ok(x.into()))).unwrap(), 386816);
    }

    const EXT_INPUT: &str = r#"dumpe2fs 1.43.9 (8-Feb-2018)
Filesystem volume name:   <none>
Last mounted on:          <not available>
Filesystem UUID:          5d9baf52-67c5-4ed2-ba13-ef20b2dfc0a7
Filesystem magic number:  0xEF53
Filesystem revision #:    1 (dynamic)
Filesystem features:      has_journal ext_attr resize_inode dir_index filetype extent flex_bg sparse_super large_file huge_file dir_nlink extra_isize metadata_csum
Filesystem flags:         signed_directory_hash
Default mount options:    user_xattr acl
Filesystem state:         clean
Errors behavior:          Continue
Filesystem OS type:       Linux
Inode count:              1310720
Block count:              5242880
Reserved block count:     262144
Free blocks:              5116591
Free inodes:              1310709
First block:              0
Block size:               4096
Fragment size:            4096
Reserved GDT blocks:      1022
Blocks per group:         32768
Fragments per group:      32768
Inodes per group:         8192
Inode blocks per group:   512
Flex block group size:    16
Filesystem created:       Tue Feb 27 13:35:37 2018
Last mount time:          n/a
Last write time:          Tue Feb 27 13:35:37 2018
Mount count:              0
Maximum mount count:      -1
Last checked:             Tue Feb 27 13:35:37 2018
Check interval:           0 (<none>)
Lifetime writes:          132 MB
Reserved blocks uid:      0 (user root)
Reserved blocks gid:      0 (group root)
First inode:              11
Inode size:               256
Required extra isize:     32
Desired extra isize:      32
Journal inode:            8
Default directory hash:   half_md4
Directory Hash Seed:      05d9ad6e-d157-401f-be37-350a5017ddbf
Journal backup:           inode blocks
Checksum type:            crc32c
Checksum:                 0x9449cff8
Journal features:         (none)
Journal size:             128M
Journal length:           32768
Journal sequence:         0x00000001
Journal start:            0
"#;

    #[test]
    fn ext_usage() {
        assert_eq!(get_ext4_usage(EXT_INPUT.lines().map(|x| Ok(x.into()))).unwrap(), 1010312);
    }

    #[test]
    fn ext_parsing() {
        let mut reader = EXT_INPUT.lines().map(|x| Ok(x.into()));
        assert_eq!(parse_field(&mut reader, "Block count:", 2).unwrap(), 5242880);

        assert_eq!(parse_field(&mut reader, "Free blocks:", 2).unwrap(), 5116591);

        assert_eq!(parse_field(&mut reader, "Block size:", 2).unwrap(), 4096);
    }

    const NTFS_INPUT: &str = r#"ntfsresize v2017.3.23 (libntfs-3g)
Device name        : /dev/sdb4
NTFS volume version: 3.1
Cluster size       : 4096 bytes
Current volume size: 21474832896 bytes (21475 MB)
Current device size: 21474836480 bytes (21475 MB)
Checking filesystem consistency ...
Accounting clusters ...
Space in use       : 69 MB (0.3%)
Collecting resizing constraints ...
You might resize at 68227072 bytes or 69 MB (freeing 21406 MB).
Please make a test run using both the -n and -s options before real resizing!"#;

    #[test]
    fn ntfs_usage() {
        let reader = NTFS_INPUT.lines().map(|x| Ok(x.into()));
        assert_eq!(get_ntfs_usage(reader).unwrap(), 133_256 + (2 * 1024 * 1024) / 512);
    }

    const BTRFS_INPUT: &str = r#"Label: none  uuid: 8a69ba4c-6cf5-46cc-aff3-f0c23251a21b
        Total devices 1 FS bytes used 112.00KiB
        devid    1 size 20.00GiB used 2.02GiB path /dev/sdb2"#;

    #[test]
    fn btrfs_usage() {
        let reader = BTRFS_INPUT.lines().map(|x| Ok(x.into()));
        assert_eq!(get_btrfs_usage(reader).unwrap(), 224);
    }
}
