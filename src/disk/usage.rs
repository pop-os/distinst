use super::FileSystemType;
use std::io::{self, BufRead, Cursor};
use std::path::Path;
use std::process::{Command, Stdio};

/// Executes a given file system's dump command to obtain the minimum shrink size
pub(crate) fn get_used_sectors<P: AsRef<Path>>(
    part: P,
    fs: FileSystemType,
    sector_size: u64,
) -> io::Result<u64> {
    use FileSystemType::*;
    match fs {
        Ext2 | Ext3 | Ext4 => {
            let mut reader = Cursor::new(
                Command::new("dumpe2fs")
                    .arg("-h")
                    .arg(part.as_ref())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()?
                    .stdout,
            );

            get_ext4_usage(reader.lines().skip(1), sector_size)
        }
        Fat16 | Fat32 => {
            let mut reader = Cursor::new(
                Command::new("fsck.fat")
                    .arg("-nv")
                    .arg(part.as_ref())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()?
                    .stdout,
            );

            get_fat_usage(reader.lines().skip(1), sector_size)
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

            let mut reader = Cursor::new(cmd.stdout).lines().skip(1);
            if cmd.status.success() {
                get_ntfs_usage(reader, sector_size)
            } else {
                get_ntfs_size(reader, sector_size)
            }
        }
        _ => unimplemented!(),
    }
}

fn get_ext4_usage<R: Iterator<Item = io::Result<String>>>(
    mut reader: R,
    sector_size: u64,
) -> io::Result<u64> {
    let total_blocks = parse_field(&mut reader, "Block count:", 2)?;
    let free_blocks = parse_field(&mut reader, "Free blocks:", 2)?;
    let block_size = parse_field(&mut reader, "Block size:", 2)?;
    Ok(((total_blocks - free_blocks) * block_size) / sector_size)
}

fn get_ntfs_usage<R: Iterator<Item = io::Result<String>>>(
    mut reader: R,
    sector_size: u64,
) -> io::Result<u64> {
    parse_field(&mut reader, "You might resize at", 4).map(|bytes| bytes / sector_size)
}

fn get_ntfs_size<R: Iterator<Item = io::Result<String>>>(
    mut reader: R,
    sector_size: u64,
) -> io::Result<u64> {
    parse_field(&mut reader, "Current volume size", 3).map(|bytes| bytes / sector_size)
}

fn get_fat_usage<R: Iterator<Item = io::Result<String>>>(
    mut reader: R,
    sector_size: u64,
) -> io::Result<u64> {
    let cluster_size = parse_fsck_field(&mut reader, "per logical sector")?;
    let (used, _) = parse_fsck_cluster_summary(&mut reader)?;
    Ok((used * cluster_size) / sector_size)
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
                        _ => break Err(io::Error::new(io::ErrorKind::Other, "invalid dump output")),
                    }
                }
            }
            None => {
                break Err(io::Error::new(
                    io::ErrorKind::Other,
                    "invalid dump output: EOF",
                ));
            }
        }
    }
}

fn parse_fsck_cluster_summary<R: Iterator<Item = io::Result<String>>>(
    reader: &mut R,
) -> io::Result<(u64, u64)> {
    let mut summary_found = false;
    loop {
        match reader.next() {
            Some(line) => {
                let line = line?;
                if summary_found {
                    if let Some(stats) = line.split_whitespace().skip(3).next() {
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
                } else if line.starts_with("Checking free cluster") {
                    summary_found = true;
                }
            }
            None => {
                break Err(io::Error::new(
                    io::ErrorKind::Other,
                    "invalid dump output: EOF",
                ));
            }
        }
    }
}

fn parse_field<R: Iterator<Item = io::Result<String>>>(
    reader: &mut R,
    field: &str,
    value: usize,
) -> io::Result<u64> {
    while let Some(line) = reader.next() {
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

    #[test]
    fn fat_parsing() {
        let mut reader = FAT_INPUT.lines().map(|x| Ok(x.into()));
        assert_eq!(
            parse_fsck_field(&mut reader, "bytes per cluster").unwrap(),
            4096
        );
        assert_eq!(
            parse_fsck_cluster_summary(&mut reader).unwrap(),
            (1, 261628)
        );
    }

    #[test]
    fn fat_usage() {
        assert_eq!(
            get_fat_usage(FAT_INPUT.lines().map(|x| Ok(x.into())), 512).unwrap(),
            1
        );
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
        assert_eq!(
            get_ext4_usage(EXT_INPUT.lines().map(|x| Ok(x.into())), 512).unwrap(),
            1010312
        );
    }

    #[test]
    fn ext_parsing() {
        let mut reader = EXT_INPUT.lines().map(|x| Ok(x.into()));
        assert_eq!(
            parse_field(&mut reader, "Block count:", 2).unwrap(),
            5242880
        );

        assert_eq!(
            parse_field(&mut reader, "Free blocks:", 2).unwrap(),
            5116591
        );

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
        let mut reader = NTFS_INPUT.lines().map(|x| Ok(x.into()));
        assert_eq!(get_ntfs_usage(reader, 512).unwrap(), 133256);
    }
}
