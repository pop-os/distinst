
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
        string squashfs;
        string lang;
        string remove;
    }

    [CCode (has_type_id = false, destroy_function = "")]
    public enum PartitionTable {
        NONE,
        GPT,
        MSDOS
    }

    public PartitionTable bootloader_detect ();

    [CCode (has_type_id = false, destroy_function = "")]
    public enum PartitionType {
        PRIMARY,
        LOGICAL
    }

    [CCode (has_type_id = false, destroy_function = "")]
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
        XFS
    }

    [CCode (has_type_id = false, destroy_function = "")]
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

    [CCode (has_type_id = false, destroy_function = "")]
    public struct PartitionFlags {
        PartitionFlag *flags;
        size_t length;
        size_t capacity;
    }

    [CCode (has_type_id = false, destroy_function = "")]
    public struct Partition {
        bool is_source;
        bool remove;
        bool format;
        bool active;
        bool busy;
        int32 number;
        int64 start_sector;
        int64 end_sector;
        PartitionType part_type;
        FileSystemType filesystem;
        PartitionFlags flags;
        string name;
        string mount_point;
        string target;
    }
    

    [CCode (has_type_id = false, destroy_function = "")]
    public struct Partitions {
        Partition *parts;
        size_t length;
    }

    [CCode (has_type_id = false, destroy_function = "")]
    public class PartitionBuilder {
        uint64 start_sector;
        uint64 end_sector;
        FileSystemType filesystem;
        PartitionType part_type;
        string name;
        string target;
        PartitionFlags flags;

        public PartitionBuilder (uint64 start_sector, uint64 end_sector, FileSystemType filesystem);
        public PartitionBuilder add_flag(PartitionFlag flag);
        public PartitionBuilder set_mount(string target);
        public PartitionBuilder set_name(string name);
        public PartitionBuilder set_partition_type(PartitionType part_type);
    }

    [CCode (has_type_id = false, free_function = "")]
    public enum SectorKind {
        Start,
        End,
        Unit,
        Megabyte
    }

    [CCode (has_type_id = false, free_function = "")]
    public class Sector {
        SectorKind flag;
        uint64 value;

        public static Sector start ();
        public static Sector end ();
        public static Sector unit (uint64 value);
        public static Sector megabyte (uint64 value);
    }

    [CCode (has_type_id = false, free_function = "distinst_disk_destroy")]
    public class Disk {
        string model_name;
        string serial;
        string device_path;
        string device_type;
        uint64 sectors;
        uint64 sector_size;
        Partitions partitions;
        PartitionTable table_type;
        bool read_only;

        public Disk (string path);
        public int add_partition (PartitionBuilder* partition);
        public int format_partition (int partition, FileSystemType fs);
        public uint64 get_sector (Sector sector);
        public int mklabel (PartitionTable table);
        public int move_partition (int partition, uint64 start);
        public int remove_partition (int partition);
        public int resize_partition (int partition, uint64 length);
        public int commit();
    }

    [CCode (has_type_id = false, free_function = "distinst_disks_destroy")]
    public class Disks {
        Disk *disks;
        size_t length;

        public Disks ();
        public static Disks new_with_capacity(size_t cap);
        public void push(Disk* disk);
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
    [CCode (free_function = "distinst_installer_destroy", has_type_id = false)]
    public class Installer {
        public Installer ();
        public void emit_error (Distinst.Error error);
        public void on_error (Distinst.ErrorCallback callback);
        public void emit_status (Distinst.Status error);
        public void on_status (Distinst.StatusCallback callback);
        public int install (Distinst.Disks disks, Distinst.Config config);
    }
}
