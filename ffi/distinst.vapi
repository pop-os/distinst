
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
        BACKUP,
        INIT,
        PARTITION,
        EXTRACT,
        CONFIGURE,
        BOOTLOADER
    }

    public const uint8 MODIFY_BOOT_ORDER;
    public const uint8 INSTALL_HARDWARE_SUPPORT;
    public const uint8 KEEP_OLD_ROOT;
    public const uint8 RUN_UBUNTU_DRIVERS;

    [CCode (has_type_id = false, destroy_function = "")]
    public struct Config {
        string hostname;
        string keyboard_layout;
        string? keyboard_model;
        string? keyboard_variant;
        string? old_root;
        string lang;
        string remove;
        string squashfs;
        uint8 flags;
    }

    [CCode (has_type_id = false)]
    public struct OsRelease {
        string bug_report_url;
        string home_url;
        string id_like;
        string id;
        string name;
        string pretty_name;
        string privacy_policy_url;
        string support_url;
        string version_codename;
        string version_id;
    }

    [CCode (has_type_id = false, destroy_function = "")]
    public struct UserAccountCreate {
        string username;
        string? realname;
        string? password;
        string profile_icon;
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
        LOGICAL,
        EXTENDED,
    }

    [CCode (cname = "DISTINST_FILE_SYSTEM", has_type_id = false)]
    public enum FileSystem {
        NONE,
        BTRFS,
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

    [CCode (cname = "DISTINST_UPGRADE_TAG", has_type_id = false)]
    public enum UpgradeTag {
        ATTEMPTING_REPAIR,
        ATTEMPTING_UPGRADE,
        DPKG_INFO,
        DPKG_ERR,
        UPGRADE_INFO,
        UPGRADE_ERR,
        PACKAGE_PROCESSING,
        PACKAGE_PROGRESS,
        PACKAGE_SETTING_UP,
        PACKAGE_UNPACKING,
        RESUMING_UPGRADE,
    }

    [CCode (cname = "DISTINST_INSTALL_OPTION_VARIANT", has_type_id = false)]
    public enum InstallOptionVariant {
        ALONGSIDE,
        ERASE,
        RECOVERY,
        REFRESH,
        UPGRADE,
    }

    [SimpleType]
    [CCode (has_type_id = false)]
    public struct UpgradeEvent {
        public UpgradeTag tag;
        public uint8 percent;
        public unowned uint8[] str1;
        public unowned uint8[] str2;
        public unowned uint8[] str3;
    }

    public delegate void UpgradeEventCallback (UpgradeEvent event);

    public delegate bool UpgradeRepairCallback ();

    public int upgrade (Disks disks, RecoveryOption option, UpgradeEventCallback event_cb,
                        UpgradeRepairCallback repair_cb);

    [CCode (has_type_id = false, unref_function = "", ref_function = "")]
    public class AlongsideOption {
        public bool is_linux ();
        public bool is_mac_os ();
        public bool is_windows ();
        public unowned uint8[] get_device ();
        public unowned uint8[] get_os ();
        public int get_os_release (out OsRelease release);
        public unowned uint8[] get_path ();
        public int get_partition ();
        public uint64 get_sectors_free ();
        public uint64 get_sectors_total ();
    }

    /**
     * An "Erase and Install" installation option.
     */
    [CCode (has_type_id = false, unref_function = "", ref_function = "")]
    public class EraseOption {
        /**
         * The location of the device in the file system.
         */
        public unowned uint8[] get_device_path ();
        /**
         * The model name or serial of the device.
         */
        public unowned uint8[] get_model ();
        /**
         * Returns a GTK icon name to associate with this device.
         */
        public unowned uint8[] get_linux_icon ();
        /**
         * If true, this device is connected via USB.
         */
        public bool is_removable ();
        /**
         * If true, this is a standard magnetic hard drive.
         */
        public bool is_rotational ();
        /**
         * Returns true if the disk is a valid size. Use this field to control
         * sensitivity of UI elements.
         */
        public bool meets_requirements ();
        /**
         * Gets the number of sectors that this option's device contains.
         */
        public uint64 get_sectors ();
    }

    /**
     * A "Refresh" installation option, which may be used to optionally retain user accounts.
     */
    [CCode (has_type_id = false, unref_function = "", ref_function = "")]
    public class RefreshOption {
        /**
         * If true, the original system may be kept in a backup directory.
         */
        public bool can_retain_old ();

        /**
         * Data from the /etc/os-release file, parsed into a data structure.
         */
        public int get_os_release (out OsRelease release);

        /**
         * The OS name string obtained from the disk.
         */
        public unowned uint8[] get_os_name ();

        /**
         * The OS pretty name obtained from the disk.
         */
        public unowned uint8[] get_os_pretty_name ();

        /**
         * The OS version string obtained from the disk.
         */
        public unowned uint8[] get_os_version ();

        /**
         * The UUID of the root partition.
         */
        public unowned uint8[] get_root_part ();
    }

    [CCode (has_type_id = false, unref_function = "", ref_function = "")]
    public class RecoveryOption {
        public unowned uint8[]? get_efi_uuid ();
        public unowned uint8[] get_recovery_uuid ();
        public unowned uint8[] get_luks_uuid ();
        public unowned uint8[] get_root_uuid ();
        public unowned uint8[] get_hostname ();
        public unowned uint8[] get_kbd_layout ();
        public unowned uint8[]? get_kbd_model ();
        public unowned uint8[]? get_kbd_variant ();
        public unowned uint8[] get_language ();
        public unowned uint8[]? mode ();
        public bool get_oem_mode ();
    }

    /**
     * Converts into an ADT within the backend to select an installation option to use.
     */
    [CCode (has_type_id = false, unref_function = "", ref_function = "")]
    public class InstallOption {
        public InstallOption ();

        /**
         * Defines which field to use.
         */
        public InstallOptionVariant tag;

        /**
         * Available valid values are:
         *
         * - EraseOption
         * - RecoveryOption
         * - RefreshOption
         */
        public void* option;

        /**
         * The encryption password to optionally use with an erase and install option.
         */
        public string? encrypt_pass;

        /**
         * The amount of available free space to use, if applicable.
         */
        public uint64 sectors;

        /**
         * Applies the stored option to the given disks object.
         */
        public int apply (Distinst.Disks disks);
    }

    /**
     * An object that will store all the available installation options.
     */
    [CCode (free_function = "distinst_install_options_destroy", has_type_id = false)]
    [Compact]
    public class InstallOptions {
        /**
         * Creates a new object from a given disks object.
         *
         * The `required` field will be used to set the `MEETS_REQUIREMENTS`
         * flag for each erase option collected.
         */
        public InstallOptions (Disks disks, uint64 required, uint64 shrink_overhead);

        public unowned RecoveryOption? get_alongside_option ();

        public bool has_alongside_options ();

        public unowned AlongsideOption[] get_alongside_options ();

        public unowned RecoveryOption? get_recovery_option ();

        public bool has_refresh_options ();

        /**
         * Gets a boxed array of refresh installation options that were collected.
         */
        public unowned RefreshOption[] get_refresh_options ();

        public bool has_erase_options ();

        /**
         * Gets a boxed array of erase and install options that were collected.
         */
        public unowned EraseOption[] get_erase_options ();
    }

    [CCode (has_type_id = false, unref_function = "")]
    public class KeyboardVariant {
        public unowned uint8[] get_name ();
        public unowned uint8[] get_description ();
    }

    [CCode (has_type_id = false, unref_function = "")]
    public class KeyboardLayout {
        public unowned uint8[] get_name ();
        public unowned uint8[] get_description ();
        public KeyboardVariant[] get_variants ();
    }

    [CCode (free_function = "", destroy_function = "distinst_keyboard_layouts_destroy", has_type_id = false)]
    [Compact]
    public class KeyboardLayouts {
        public KeyboardLayouts ();
        public KeyboardLayout[] get_layouts ();
    }

    /**
     * Deactivates all logical devices. Should be executed at the start of the installer.
     */
    public int deactivate_logical_devices ();

    /**
     * Hashes the contents of `/dev/`; useful for detecting layout changes.
     */
    public uint64 device_layout_hash ();

    /**
     * Returns true if the device name already exists
     */
    public bool device_map_exists (string name);

    /**
     * Obtains the default locale associated with a language.
     */
    public string? locale_get_default (string lang);

    /**
     * Obtains the main country for a given language code.
     */
    public unowned uint8[]? locale_get_main_country (string code);

    /**
     * Obtains a list of available language locales.
     */
    public string[] locale_get_language_codes ();

    /**
     * Obtains a list of countries associated with a language
     */
    public string[]? locale_get_country_codes (string lang);

    /**
     * Get the name of a language by the ISO 639 language code.
     */
    public unowned uint8[]? locale_get_language_name (string code);

    /**
     * Get the translated name of a language by the ISO 639 language code.
     */
    public string? locale_get_language_name_translated (string code);

    /**
     * Get the name of a country by the ISO 3166 country code.
     */
    public unowned uint8[]? locale_get_country_name (string code);

    /**
     * Get the translated name of a country by the ISO 3166 country code,
     * and the ISO 639 language code.
     */
    public string? locale_get_country_name_translated (string country, string lang);

    /**
     * Generates a unique volume group name.
     */
    public string? generate_unique_id (string prefix);

    /**
     * Obtains the string variant of a file system type.
     */
    public unowned string strfilesys (FileSystem fs);

    /** Obtain the file size specified in `/cdrom/casper/filesystem.size`, or
     * return a default value.
     *
     * If the value in `filesystem.size` is lower than that of the default, the
     * default will be returned instead.
     */
    public uint64 minimum_disk_size (uint64 size);

    /**
     * Determines if the given hostname is valid or not
     */
    public bool validate_hostname (string hostname);

    /**
     * Inhibits suspend via org.freedesktop.login1.Manager.
     *
     * Returns a raw file descriptor which will unlock the inhibitor when closed.
     */
    public int session_inhibit_suspend ();

    /**
     * The followting functions take information from a static structure
     * in the library which contains information from `/etc/os-release`.
     */

    public uint8[] get_os_bug_report_url ();
    public uint8[] get_os_home_url ();
    public uint8[] get_os_id_like ();
    public uint8[] get_os_id ();
    public uint8[] get_os_name ();
    public uint8[] get_os_pretty_name ();
    public uint8[] get_os_privacy_policy_url ();
    public uint8[] get_os_support_url ();
    public uint8[] get_os_version_codename ();
    public uint8[] get_os_version_id ();
    public uint8[] get_os_version ();

    [CCode (has_type_id = false, ref_function = "", unref_function = "")]
    [Compact]
    public class Timezones {
        public Timezones ();
        public Zones zones ();
    }

    [CCode (has_type_id = false, ref_function = "", unref_function = "")]
    [Compact]
    public class Zones {
        public unowned Zone? next ();
        public unowned Zone? nth (int nth);
    }

    [CCode (has_type_id = false, ref_function = "", unref_function = "")]
    [Compact]
    public class Regions {
        public unowned Region? next ();
        public unowned Region? nth (int nth);
    }

    [CCode (has_type_id = false, ref_function = "", unref_function = "", destroy_function = "")]
    [Compact]
    public class Zone {
        public unowned uint8[] name ();
        public Regions regions ();
    }

    [CCode (has_type_id = false, ref_function = "", unref_function = "", destroy_function = "")]
    [Compact]
    public class Region {
        public unowned uint8[] name ();
        public Region clone ();
    }

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

    [CCode (has_type_id = false, unref_function = "", destroy_function = "distinst_partition_and_disk_path_destroy")]
    public class PartitionAndDiskPath {
        public string disk_path;
        public unowned Partition partition;
    }

    [CCode (has_type_id = false, unref_function = "")]
    public class Partition {
        /**
         * Returns the partition's number.
         */
        public int get_number ();

        /**
         * Returns the partition's device path.
         */
        public unowned uint8[] get_device_path ();

        /**
         * Sets the flags that will be assigned to this partition.
         */
        public void set_flags (PartitionFlag[] flags);

        /**
         * Sets the mount target for this partition.
         */
        public void set_mount (string target);

        /**
         * Marks to format the partition with the provided file system.
         *
         * Retains the partiton's name.
         */
        public int format_and_keep_name (FileSystem fs);

        /**
         * Marks to format the partition with the provided file system.
         *
         * Also removes the partition's name in the process.
         */
        public int format_with (FileSystem fs);

        /**
         * If a pre-existing LVM volume group has been assigned, this will return that group's name.
         */
        public unowned uint8[] get_current_lvm_volume_group ();

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
        public unowned uint8[]? get_label ();

        /**
         * Gets the mount point of the partition.
         */
        public unowned uint8[]? get_mount_point ();

        /**
         * Returns the file system which the partition is formatted with
         */
        public FileSystem get_file_system ();

        /**
         * Checks if the partition is LUKS-encrypted.
         */
        public bool is_encrypted ();

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

        /**
         * Checks if the partition is a EFI partition.
         */
        public bool is_esp ();

        /**
         * Checks if the partition is a swap partition.
         */
        public bool is_swap ();

        /**
         * Checks if Linux may be installed to this partition.
         */
        public bool is_linux_compatible ();
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
        public unowned Partition get_partition (int partition);

        /**
         * Gets the partition by the partition path.
         */
        public unowned Partition get_partition_by_path (string path);

        /**
         * Returns a slice of all partitions on this disk.
         */
        public unowned Partition[] list_partitions ();

        /**
         * Specifies to format a partition at the given partition ID with the specified
         * file system.
         */
        public int format_partition (int partition, FileSystem fs);

        /**
         * Returns the model name of the device, ie: (ATA Samsung 850 EVO)
         */
        public unowned uint8[] get_model();

        /**
         * Returns the serial of the device, ie: (Samsung_SSD_850_EVO_500GB_S21HNXAG806916N)
         */
        public unowned uint8[] get_serial();

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
         * Identifies the type of table that the disk has.
         */
        public PartitionTable get_partition_table ();

        /**
         * Returns true if the device contains a partition mounted at the specified target.
         */
        public bool contains_mount (string mount, Distinst.Disks disks);

        /**
         * Returns true if the device is read-only
         */
        public bool is_read_only ();

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
        public unowned uint8[] get_model ();

        /**
         * If this is not `None`, then LVM is not on the device, and the
         * device contains a file system instead.
         */
        public unowned Partition? get_encrypted_file_system ();

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
         * Gets a logical volume by the volume name.
         */
        public unowned Partition? get_volume (string volume);

        /**
         * Returns true if the device contains a partition mounted at the specified target.
         */
        public bool contains_mount (string mount, Distinst.Disks disks);

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
         * Gets the partition by the partition path.
         */
        public unowned Partition get_partition_by_path (string path);

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
    public struct LuksEncryption {
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
        public static Disks probe ();
        public Disks ();
        public void push (owned Disk disk);

        /**
         * Returns a slice of physical devices in the configuration.
         */
        public unowned Disk[] list ();

        /**
         * Returns a slice of logical devices in the configuration.
         */
        public unowned LvmDevice[] list_logical ();

        /**
         * Obtains a list of encrypted partitions detected in the system.
         */
        public unowned Partition[] get_encrypted_partitions ();

        /**
         * Obtains the logical device with the specified volume group.
         *
         * Will return a null value if the input string is not UTF-8,
         * or the logical device could not be found.
         */
        public unowned LvmDevice? get_logical_device (string volume_group);

        /**
         * Obtains the logical device that is within the specified LUKS
         * physical volume name.
         *
         * Will return a null value if the input string is not UTF-8,
         * or the logical device could not be found.
         */
        public unowned LvmDevice? get_logical_device_within_pv (string volume_group);

        /**
         * Returns the probed partition with the given UUID string.
         */
        public unowned Partition? get_partition_by_uuid (string uuid);

        /**
         * Find the disk that contains the mount.
         */
        public unowned Disk? get_disk_with_mount (string target);

        /**
         * Find the disk that contains the partition.
         */
        public unowned Disk? get_disk_with_partition (Partition? partition);

        /**
         * Obtains the physical device at the specified path.
         *
         * Will return a null value if the input string is not UTF-8,
         * or the physical device could not be found.
         */
        public unowned Disk? get_physical_device (string path);

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
        public int decrypt_partition (string path, LuksEncryption encryption);

        /**
         * Finds the partition block path and associated partition information
         * that is associated with the given target mount point. Scans both physical
         * and logical partitions.
         */
        public PartitionAndDiskPath? find_partition (string target);

        /**
         * True if any partition on the disk is a LUKS partition.
         */
        public bool contains_luks ();
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

    public delegate unowned Region TimezoneCallback ();

    public delegate UserAccountCreate UserAccountCallback ();

    /**
     * Attempts to unset the active mode
     *
     * Returns `false` on an error. See distinst logs for details on the cause of the error.
     */
    bool unset_mode ();

    int log (Distinst.LogCallback callback);

    [Compact]
    [CCode (destroy_function = "distinst_installer_destroy", free_function = "", has_type_id = false)]
    public class Installer {
        public Installer ();
        public void emit_error (Distinst.Error error);
        public void on_error (Distinst.ErrorCallback callback);
        public void emit_status (Distinst.Status error);
        public void on_status (Distinst.StatusCallback callback);
        public void set_timezone_callback (TimezoneCallback callback);
        public void set_user_callback (UserAccountCallback callback);
        public int install (owned Distinst.Disks disks, Distinst.Config config);
    }
}
