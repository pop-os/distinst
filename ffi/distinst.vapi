
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
        string keyboard_layout;
        string? keyboard_model;
        string? keyboard_variant;
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
        LVM,
        LUKS,
    }

    /**
     * Obtains the string variant of a file system type.
     */
    public unowned string strfilesys(FileSystemType fs);

    /** Obtain the file size specified in `/cdrom/casper/filesystem.size`, or
     * return a default value.
     *
     * If the value in `filesystem.size` is lower than that of the default, the
     * default will be returned instead.
     */
    public uint64 minimum_disk_size(uint64 size);

    /**
     * Determines if the given hostname is valid or not
     */
    public bool validate_hostname(string hostname);

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

    /**
     * Partition builders are supplied as inputs to the `add_partition` method.
     */
    [CCode (has_type_id = false, unref_function = "")]
    public class PartitionBuilder {
        /**
         * Creates a new partition builder which has it's start and end sectors defined, as well
         * as the file system to assign to it.
         */
        public PartitionBuilder (uint64 start_sector, uint64 end_sector, FileSystemType filesystem);

        /**
         * Defines a label for the new partition.
         */
        public PartitionBuilder name (string name);

        /**
         * Specifies where the new partition should be mounted.
         */
        public PartitionBuilder mount (string target);

        /**
         * Defines if the partition is either primary or logical.
         */
        public PartitionBuilder partition_type (PartitionType part_type);

        /**
         * Adds a partition flag to the new partition.
         */
        public PartitionBuilder flag (PartitionFlag flag);

        /**
         * Assigns this new partition to a logical volume group.
         *
         * If the encryption parameter is not set, this will be a LVM partition.
         * Otherwise, a LUKS partition will be created with the information in in the
         * encryption parameter, and a LVM partition will be assigned on top of that.
         */
        public PartitionBuilder logical_volume (string volume_group, LvmEncryption? encryption);

        /**
         * Species that this partition will contain a keyfile that belongs to the associated ID.
         *
         * Note that this partition should also have a mount target, or otherwise
         * an error will occur.
         */
        public PartitionBuilder associate_keyfile (string keyfile_id);
    }

    [SimpleType]
    [CCode (has_type_id = false)]
    public struct PartitionUsage {
        /**
         * None = 0; Some(usage) = 1;
         */
        public uint8 tag;
        /**
         * The size, in sectors, that a partition is used.
         */
        public uint64 value;
    }

    [CCode (has_type_id = false, unref_function = "")]
    public class Partition {
        /**
         * Returns the partition's device path.
        */
        public unowned uint8[] get_device_path ();

        /**
         * Sets the flags that will be assigned to this partition.
        */
        public void set_flags (PartitionFlag[] flags, size_t len);

        /**
         * Sets the mount target for this partition.
        */
        public void set_mount (string target);

        /**
         * Marks to format the partition with the provided file system.
         *
         * Retains the partiton's name.
        */
        public int format_and_keep_name (FileSystemType fs);

        /**
         * Marks to format the partition with the provided file system.
         *
         * Also removes the partition's name in the process.
        */
        public int format_with (FileSystemType fs);

        /**
         * Gets the start sector where this partition lies on the disk.
        */
        public uint64 get_start_sector ();

        /**
         * Gets the end sector where this partition lies on the disk.
        */
        public uint64 get_end_sector ();

        /**
         * Gets the name of the partition.
        */
        public string? get_label ();

        /**
         * Gets the mount point of the partition.
        */
        public string? get_mount_point ();

        /**
         * Returns the file system which the partition is formatted with
        */
        public FileSystemType get_file_system ();

        /**
         * Returns the name of the OS which is installed here
        */
        public string? probe_os ();

        /**
         * Returns the number of sectors that are used in the file system
        */
        public PartitionUsage sectors_used (uint64 sector_size);

        /**
         * Species that this partition will contain a keyfile that belongs to the associated ID.
         *
         * Note that this partition should also have a mount target, or otherwise
         * an error will occur.
         */
        public void associate_keyfile (string keyfile_id);
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
        /**
         * 0 = Ok; 1 = Err
         */
        public uint8 flag;
        /**
         * Err value
         */
        public string error;
        /**
         * Ok value
         */
        public Sector sector;
    }

    /**
     * A human-friendly algebraic data type for obtaining sector positions from a device.
     */
    [SimpleType]
    [CCode (has_type_id = false)]
    public struct Sector {
        SectorKind flag;
        uint64 value;

        /**
         * Obtains a `Sector` from a string. IE:
         * - "90%"
         * - "500M"
         * - "-4096M"
         * -  "start"
         */
        public static SectorResult from_str(string value);

        /**
         * Creates a `Sector::Start` variant.
         */
        public static Sector start();

        /**
         * Creates a `Sector::End` variant.
         */
        public static Sector end();

        /**
         * Creates a `Sector::Unit(value)` variant.
         */
        public static Sector unit(uint64 value);

        /**
         * Creates a `Sector::UnitFromEnd(value)` variant.
         */
        public static Sector unit_from_end(uint64 value);

        /**
         * Creates a `Sector::Megabyte(value)` variant.
         */
        public static Sector megabyte(uint64 value);

        /**
         * Creates a `Sector::MegabyteFromEnd(value)` variant.
         */
        public static Sector megabyte_from_end(uint64 value);

        /**
         * Creates a `Sector::Percent(value)` variant.
         */
        public static Sector percent(uint16 value);
    }

    [CCode (free_function = "distinst_disk_destroy", has_type_id = false)]
    [Compact]
    public class Disk {
        public Disk (string path);
        public unowned uint8[] get_device_path();

        /**
         * Gets the partition at the specified location.
         */
        public unowned Partition get_partition(int partition);

        /**
         * Returns a slice of all partitions on this disk.
         */
        public unowned Partition[] list_partitions();

        /**
         * Adds a new partition to the physical device from a partition builder.
         */
        public int add_partition (PartitionBuilder partition);

        /**
         * Specifies to format a partition at the given partition ID with the specified
         * file system.
         */
        public int format_partition (int partition, FileSystemType fs);

        /**
         * Returns the model name of the device, ie: (ATA Samsung 850 EVO)
         */
        public string? get_model();

        /**
         * Returns the serial of the device, ie: (Samsung_SSD_850_EVO_500GB_S21HNXAG806916N)
         */
        public string? get_serial();

        /**
         * Returns the size of the device, in sectors.
         */
        public uint64 get_sectors ();

        /**
         * Returns the size of a sector, in bytes.
         */
        public uint64 get_sector_size ();

        /**
         * Gets the actual sector position from a `Sector` unit.
         */
        public uint64 get_sector (ref Sector sector);

        /**
         * Returns true if the device contains a partition mounted at the specified target.
         */
        public bool contains_mount (string mount);

        /**
         * Returns true if the device is a removable device.
         */
        public bool is_removable ();

        /**
         * Returns true if the device is a spinny disk.
         */
        public bool is_rotational ();

        /**
         * Marks all partitions for removal, and specifies to write a new partition table
         */
        public int mklabel (PartitionTable table);

        /**
         * Moves the partition to the new start sector.
         */
        public int move_partition (int partition, uint64 start);

        /**
         * Removes the specified partition with the provided number.
         *
         * A value of `1` on a Disk whose path is `/dev/sda` will remove `/dev/sda1`,
         *     and a value of `1` with a Disk at `/dev/nvme0` will remove `/dev/nvme0p1`.
         */
        public int remove_partition (int partition);

        /**
         * Resizes the partition to the new end sector.
         *
         * NOTE: This should always be called after `move_partition`.
         *       Distinst automatically handles the shrink/grow & move order for you.
         */
        public int resize_partition (int partition, uint64 end);

        /**
         * Commits all changes made to this in-memory reprsentation of the Disk to the actual
         * hardware.
         */
        public int commit();
    }

    [CCode (has_type_id = false, destroy_function = "", unref_function = "")]
    public class LvmDevice {
        public unowned uint8[] get_device_path ();

        /**
         * Returns the model name of the device in the format of "LVM <VG>"
         */
        public string? get_model ();

        /**
         * Gets the actual sector position from a `Sector` unit.
         */
        public uint64 get_sector (ref Sector sector);

        /**
         * Gets a logical volume by the volume name.
         */
        public unowned Partition? get_volume (string volume);

        /**
         * Returns true if the device contains a partition mounted at the specified target.
         */
        public bool contains_mount (string mount);

        /**
         * Returns a slice of all partitions on this volume.
         */
        public unowned Partition[] list_partitions ();

        /**
         * Partitions are assigned left to right, so this will get the end
         * sector of the last partition.
         */
        public uint64 last_used_sector ();

        /**
         * Adds a new partition to the physical device from a partition builder.
         */
        public int add_partition (PartitionBuilder partition);

        /**
         * Sets the remove bit on the specified logical volume
         *
         * # Return Values
         *
         * - `0` means that there was no error.
         * - `1` means that the provided string was not UTF-8.
         * - `2` means that the partition could not be found.
         */
        public int remove_partition (string volume);

        /**
         * For each logical volume in the LVM device, this will set the remove bit.
         */
        public void clear_partitions ();
    }

    /**
     * Defines the configuration options to use when creating a new LUKS partition.
     */
    [CCode (has_type_id = false, destroy_function = "", unref_function = "")]
    public struct LvmEncryption {
        /**
         * Defines the name of the new PV that the LUKS partition will expose
         * IE: "cryptdata" set here will create a new device map at `/dev/mapper/cryptdata`
         */
        string physical_volume;

        /**
         * Optionally defines a password that will be used to encrypt & decrypt the LUKS partition
         */
        string? password;

        /**
         * Optionally defines the key ID that the LUKS partition will find it's keyfile on.
         * This key ID will need to be assigned to another partition, or the install will fail.
         */
        string? keydata;
    }

    /**
     * This object will contain all physical and logical disk configurations for the installer.
     */
    [CCode (free_function = "distinst_disks_destroy", has_type_id = false)]
    [Compact]
    public class Disks {
        public static Disks probe();
        public Disks ();
        public void push(Disk disk);

        /**
         * Returns a slice of physical devices in the configuration.
         */
        public unowned Disk[] list();

        /**
         * Returns a slice of logical devices in the configuration.
         */
        public unowned LvmDevice[] list_logical();

        /**
         * Obtains the logical device with the specified volume group.
         *
         * Will return a null value if the input string is not UTF-8,
         * or the logical device could not be found.
         */
        public unowned LvmDevice? get_logical_device (string volume_group);

        /**
         * To be used after configuring all physical partitions on physical disks,
         * this method will initialize all of the logical devices within the `Disks`
         * object.
         */
        public int initialize_volume_groups ();

        /**
         * Decrypts the specified LUKS partition by its device path.
         *
         * # Return Values
         *
         * - 0 means success
         * - 1 means that critical input values were null
         * - 2 indicates that a UTF-8 error occurred
         * - 3 indicates that neither a password or keydata was supplied
         * - 4 indicates an error when decrypting the partition -- likely an invalid password
         * - 5 indicates that the decrypted partition lacks a LVM volume group
         * - 6 indicates that the specified LUKS partition at `path` was not found
         */
        public int decrypt_partition (string path, LvmEncryption encryption);

        /**
         * Finds the logical device which is associated with the given volume group.
         */
        public unowned LvmDevice find_logical_volume (string volume_group);
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
        public int install (owned Distinst.Disks disks, Distinst.Config config);
    }
}
