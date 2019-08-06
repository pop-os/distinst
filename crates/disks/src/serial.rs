//! Serial numbers can be used to ensure that any partitions written to disks
//! are written to the correct drives, as it could be possible, however
//! unlikely, that a user could hot swap drives after obtaining device
//! information, but before writing their changes to the disk.

use std::{io, path::Path, process::Command};

const PATTERN: &str = "E: ID_SERIAL=";

/// Obtains the serial of the given device by calling out to `udevadm`.
///
/// The `path` should be a value like `/dev/sda`.
pub fn get_serial(path: &Path) -> io::Result<String> {
    info!("obtaining serial model from {}", path.display());
    Command::new("udevadm")
        .args(&["info", "--query=all", &format!("--name={}", path.display())])
        .output()
        .and_then(|output| parse_serial(&output.stdout))
}

fn parse_serial(data: &[u8]) -> io::Result<String> {
    String::from_utf8_lossy(data)
        .lines()
        .find(|line| line.starts_with(PATTERN))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no serial field"))
        .map(|serial| serial.split_at(PATTERN.len()).1.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"P: /devices/pci0000:00/0000:00:17.0/ata4/host3/target3:0:0/3:0:0:0/block/sda
N: sda
S: disk/by-id/ata-Samsung_SSD_850_EVO_500GB_S21HNXAG806916N
S: disk/by-id/wwn-0x5002538d403d649a
S: disk/by-path/pci-0000:00:17.0-ata-4
E: DEVLINKS=/dev/disk/by-path/pci-0000:00:17.0-ata-4 /dev/disk/by-id/wwn-0x5002538d403d649a /dev/disk/by-id/ata-Samsung_SSD_850_EVO_500GB_S21HNXAG806916N
E: DEVNAME=/dev/sda
E: DEVPATH=/devices/pci0000:00/0000:00:17.0/ata4/host3/target3:0:0/3:0:0:0/block/sda
E: DEVTYPE=disk
E: ID_ATA=1
E: ID_ATA_DOWNLOAD_MICROCODE=1
E: ID_ATA_FEATURE_SET_HPA=1
E: ID_ATA_FEATURE_SET_HPA_ENABLED=1
E: ID_ATA_FEATURE_SET_PM=1
E: ID_ATA_FEATURE_SET_PM_ENABLED=1
E: ID_ATA_FEATURE_SET_SECURITY=1
E: ID_ATA_FEATURE_SET_SECURITY_ENABLED=0
E: ID_ATA_FEATURE_SET_SECURITY_ENHANCED_ERASE_UNIT_MIN=8
E: ID_ATA_FEATURE_SET_SECURITY_ERASE_UNIT_MIN=2
E: ID_ATA_FEATURE_SET_SECURITY_FROZEN=1
E: ID_ATA_FEATURE_SET_SMART=1
E: ID_ATA_FEATURE_SET_SMART_ENABLED=1
E: ID_ATA_ROTATION_RATE_RPM=0
E: ID_ATA_SATA=1
E: ID_ATA_SATA_SIGNAL_RATE_GEN1=1
E: ID_ATA_SATA_SIGNAL_RATE_GEN2=1
E: ID_ATA_WRITE_CACHE=1
E: ID_ATA_WRITE_CACHE_ENABLED=1
E: ID_BUS=ata
E: ID_MODEL=Samsung_SSD_850_EVO_500GB
E: ID_MODEL_ENC=Samsung\x20SSD\x20850\x20EVO\x20500GB\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20
E: ID_PART_TABLE_TYPE=gpt
E: ID_PART_TABLE_UUID=8f60266f-963f-40e6-8c29-c525632dfbe8
E: ID_PATH=pci-0000:00:17.0-ata-4
E: ID_PATH_TAG=pci-0000_00_17_0-ata-4
E: ID_REVISION=EMT01B6Q
E: ID_SERIAL=Samsung_SSD_850_EVO_500GB_S21HNXAG806916N
E: ID_SERIAL_SHORT=S21HNXAG806916N
E: ID_TYPE=disk
E: ID_WWN=0x5002538d403d649a
E: ID_WWN_WITH_EXTENSION=0x5002538d403d649a
E: MAJOR=8
E: MINOR=0
E: SUBSYSTEM=block
E: TAGS=:systemd:
E: USEC_INITIALIZED=1564088"#;

    #[test]
    fn serial() {
        assert_eq!(
            parse_serial(SAMPLE.as_bytes()).unwrap(),
            String::from("Samsung_SSD_850_EVO_500GB_S21HNXAG806916N")
        );
    }
}
