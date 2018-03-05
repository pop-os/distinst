
[CCode (cprefix = "Distinst", lower_case_cprefix = "distinst_", cheader_filename = "distinst.h")]
namespace Distinst {
    [CCode (cname = "DISTINST_LOG_LEVEL", has_type_id = false)]
    public enum LogLevel {
        TRACE,
        DEBUG,
        INFO,
        WARN,
        ERROR
    }

    public delegate void LogCallback (Distinst.LogLevel level, string message);

    [CCode (cname = "DISTINST_STEP", has_type_id = false)]
    public enum Step {
        INIT,
        PARTITION,
        EXTRACT,
        CONFIGURE,
        BOOTLOADER
    }

    [CCode (has_type_id = false, destroy_function = "")]
    public struct Config {
        string hostname;
        string keyboard;
        string lang;
        string remove;
        string squashfs;
    }

    [CCode (cname = "DISTINST_PARTITION_TABLE", has_type_id = false)]
    public enum PartitionTable {
        NONE,
        GPT,
        MSDOS
    }

    public PartitionTable bootloader_detect ();

    [CCode (cname = "DISTINST_PARTITION_TYPE", has_type_id = false)]
    public enum PartitionType {
        PRIMARY,
        LOGICAL
    }

    [CCode (cname = "DISTINST_FILE_SYSTEM_TYPE", has_type_id = false)]
    public enum FileSystemType {
        NONE,
        BTRFS,
        EXFAT,
        EXT2,
        EXT3,
        EXT4,
        F2FS,
        FAT16,
        FAT32,
        NTFS,
        SWAP,
        XFS,
        LVM
    }

    public unowned string strfilesys(FileSystemType fs);

    [CCode (cname = "DISTINST_PARTITION_FLAG", has_type_id = false)]
    public enum PartitionFlag {
        BOOT,
        ROOT,
        SWAP,
        HIDDEN,
        RAID,
        LVM,
        LBA,
        HPSERVICE,
        PALO,
        PREP,
        MSFT_RESERVED,
        BIOS_GRUB,
        APPLE_TV_RECOVERY,
        DIAG,
        LEGACY_BOOT,
        MSFT_DATA,
        IRST,
        ESP
    }

    [CCode (has_type_id = false, unref_function = "")]
    public class PartitionBuilder {
        public PartitionBuilder (uint64 start_sector, uint64 end_sector, FileSystemType filesystem);
        public PartitionBuilder name (string name);
        public PartitionBuilder mount (string target);
        public PartitionBuilder partition_type (PartitionType part_type);
        public PartitionBuilder flag (PartitionFlag flag);
        public PartitionBuilder logical_volume (string group, LvmEncryption? encryption);
    }

    [SimpleType]
    [CCode (has_type_id = false)]
    public struct PartitionUsage {
        public uint64 value;
        public uint8 tag;
    }

    [CCode (has_type_id = false, unref_function = "")]
    public class Partition {
        public unowned uint8[] get_device_path ();
        public void set_flags (PartitionFlag[] flags, size_t len);
        public void set_mount (string target);
        public int format_with (FileSystemType fs);
        public uint64 get_start_sector ();
        public uint64 get_end_sector ();
        public string? get_label ();
        public FileSystemType get_file_system ();
        public string? probe_os ();
        public PartitionUsage sectors_used (uint64 sector_size);
    }

    [CCode (has_type_id = false)]
    public enum SectorKind {
        START,
        END,
        UNIT,
        UNIT_FROM_END,
        MEGABYTE,
        MEGABYTE_FROM_END,
        PERCENT
    }

    [SimpleType]
    [CCode (has_type_id = false)]
    public struct SectorResult {
        // 0 = Ok; 1 = Err
        public uint8 flag;
        // Err value
        public string error;
        // Ok value
        public Sector sector;
    }

    [SimpleType]
    [CCode (has_type_id = false)]
    public struct Sector {
        SectorKind flag;
        uint64 value;

        public static SectorResult from_str(string value);
        public static Sector start();
        public static Sector end();
        public static Sector unit(uint64 value);
        public static Sector unit_from_end(uint64 value);
        public static Sector megabyte(uint64 value);
        public static Sector megabyte_from_end(uint64 value);
        public static Sector percent(uint16 value);
    }

    [CCode (has_type_id = false, destroy_function = "distinst_disk_destroy", unref_function = "")]
    public class Disk {
        public Disk (string path);
        public unowned uint8[] get_device_path();
        public Partition get_partition(int partition);
        public unowned Partition[] list_partitions();
        public int add_partition (PartitionBuilder partition);
        public int format_partition (int partition, FileSystemType fs);
        public uint64 get_sectors ();
        public uint64 get_sector_size ();
        public uint64 get_sector (ref Sector sector);
        public int mklabel (PartitionTable table);
        public int move_partition (int partition, uint64 start);
        public int remove_partition (int partition);
        public int resize_partition (int partition, uint64 end);
        public int commit();
        public int initialize_volume_groups ();
        public unowned LvmDevice find_logical_volume (string group);
    }

    [CCode (has_type_id = false, destroy_function = "", unref_function = "")]
    public class LvmDevice {
        public uint64 last_used_sector ();
        public uint64 get_sector (ref Sector sector);
        public int add_partition (PartitionBuilder partition);
    }

    [CCode (has_type_id = false, destroy_function = "", unref_function = "")]
    public class LvmEncryption {
        string physical_volume;
        string? password;
        string? keydata;
    }

    [CCode (has_type_id = false, destroy_function = "distinst_disks_destroy", free_function = "", unref_function = "")]
    public class Disks {
        public static Disks probe();
        public Disks ();
        public unowned Disk[] list();
        public void push(Disk disk);
    }

    [CCode (has_type_id = false)]
    public struct Error {
        Distinst.Step step;
        int err;
    }

    public delegate void ErrorCallback (Distinst.Error status);

    [CCode (has_type_id = false)]
    public struct Status {
        Distinst.Step step;
        int percent;
    }

    public delegate void StatusCallback (Distinst.Status status);

    int log (Distinst.LogCallback callback);

    [Compact]
    [CCode (destroy_function = "distinst_installer_destroy", free_function = "", has_type_id = false)]
    public class Installer {
        public Installer ();
        public void emit_error (Distinst.Error error);
        public void on_error (Distinst.ErrorCallback callback);
        public void emit_status (Distinst.Status error);
        public void on_status (Distinst.StatusCallback callback);
        public int install (Distinst.Disks disks, Distinst.Config config);
    }
}
