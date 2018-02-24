# distinst

**Warning!** *This code is not ready for general use. It is not compatible with all Ubuntu-based distributions. Release is targeted for April, 2018.*

Distinst is a Rust-based software library that handles Linux distribution installer installation details. It has been built specifically to be used in the construction of Linux distribution installers, so that installers can spend more time improving their UI, and less time worrying about some of the more complicated implementation details, such as partition management.

## Frontends

At the moment, elementary's installer is the primary target for distinst. However, distinst also ships with a CLI application (also called distinst) that serves as a fully-functioning test bed for the distinst library.

### CLI

- [distinst](https://github.com/pop-os/distinst/) (Rust)

### GTK

 - [elementary Installer](https://github.com/elementary/installer) (Vala)

## Capabilities

### Disk Partitioning & Formatting

Distinst provides a Rust, C, and Vala API for probing disk and partition information, as well as the ability to create and manipulate partitions. In addition to partitioning the disk via the libparted bindings, distinst will also handle disk partitioning using `mkfs`, provided that you have installed the corresponding packages for each file system type that you want to support in your installer.

Implementors of the library should note that distinst utilizes in-memory partition management logic to determine whether changes that are being specified will be valid or not. Changes specified will be applied by distinst during the `install` method, which is where you will pass your disk configurations into. This configuration will further be validated by distinst before finally making the changes with libparted (which also performs similar measures of its own).

> LVM support & disk/partition encryption has not yet been implemented.

#### Rust Example

See the source code for the [distinst](https://github.com/pop-os/distinst/blob/master/src/main.rs) CLI application.

#### Vala Example

```vala
if (disk.mklabel (bootloader) != 0) {
    stderr.printf ("unable to write GPT partition table to /dev/sda");
    exit (1);
}

var efi_sector = Sector() {
    flag = SectorKind.MEGABYTE,
    value = 512
};

var start = disk.get_sector (Sector.start());
var end = disk.get_sector (efi_sector);

int result = disk.add_partition(
    new PartitionBuilder (start, end, FileSystemType.FAT32)
        .set_partition_type (PartitionType.PRIMARY)
        .add_flag (PartitionFlag.ESP)
        .set_mount ("/boot/efi")
);

if (result != 0) {
    stderr.printf ("unable to add EFI partition to disk");
    exit (1);
}

start = disk.get_sector (efi_sector);
end = disk.get_sector (Sector.end ());

result = disk.add_partition (
    new PartitionBuilder (start, end, FileSystemType.EXT4)
        .set_partition_type (PartitionType.PRIMARY)
        .set_mount ("/")
);

if (result != 0) {
    stderr.printf ("unable to add / partition to disk");
    exit (1);
}

Disks disks = Disks.with_capacity (1);
disks.push (disk);
installer.install (disks, config);
```

### Extracting, Chrooting, & Configuring

The implementor of the library should provide a squashfs file that contains a base image that the installer will extract during installation, as well as the accompanying `.manifest-remove` file. These can be found on the Pop!_OS ISOs, as an example. Once this image has been extracted, the installer will chroot into the new install and then configure the image using the configuration script located at `src/configure.sh`.

### Bootloader

Based on whether the image is running on a system that is EFI or not, the bootloader will be configured using either systemd-boot or GRUB, thereby allowing the user to be capable of booting into install once the system is rebooted.

## Build Instructions

In order to build `distinst` on Ubuntu, you will need to follow these instructions:

```
# Install libparted
sudo apt install libparted-dev

# Fetch libparted Rust bindings
git submodule update --init libparted

# Install Rust
curl https://sh.rustup.rs -sSf | sh

# Build in release mode
make

# Install
sudo make install

# Uninstall
sudo make uninstall
```

The following files will be generated:

- CLI app: `target/release/distinst`
- Library: `target/release/libdistinst.so`
- Header: `target/include/distinst.h`
- pkg-config: `target/pkg-config/distinst.pc`

These files will be placed in /usr/local when installed, and `pkg-config --cflags distinst` or `pkg-config --libs distinst` can then be used to find them.

In order to produce a source package, you must run the following commands:

```
# Install cargo-vendor
cargo install cargo-vendor

# Download vendored sources
make vendor
```
