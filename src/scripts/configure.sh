# Exit on error and trace commands
set -ex

# Load OS information variables
source "/etc/os-release"

# Set up environment
export DEBIAN_FRONTEND="noninteractive"
export HOME="/root"
export LC_ALL="${LANG}"
export PATH="/usr/sbin:/usr/bin:/sbin:/bin"

# Parse arguments
PURGE_PKGS=()
INSTALL_PKGS=()

for arg in "$@"
do
    if [[ "${arg:0:1}" == "-" ]]
    then
        PURGE_PKGS+=("${arg:1}")
    else
        INSTALL_PKGS+=("${arg}")
    fi
done

# Set the hostname
echo "${HOSTNAME}" > "/etc/hostname"

# Set the host within the hosts file
echo "127.0.0.1	localhost
::1		localhost
127.0.1.1	${HOSTNAME}.localdomain	${HOSTNAME}" > /etc/hosts

# Generate a machine ID
dbus-uuidgen > "/var/lib/dbus/machine-id"

# Correctly specify resolv.conf
ln -sf "../run/resolvconf/resolv.conf" "/etc/resolv.conf"

# Update locales
locale-gen --purge "${LANG}"
update-locale --reset "LANG=${LANG}"

# Set keyboard settings system-wide
localectl set-x11-keymap "${KBD_LAYOUT}" "${KBD_MODEL}" "${KBD_VARIANT}"

# Remove installer packages
apt-get purge -y "${PURGE_PKGS[@]}"
apt-get autoremove -y --purge

# Add the cdrom to APT, if it exists.
APT_OPTIONS=()
if [ -d "/cdrom" ]
then
    APT_OPTIONS+=(-o "Acquire::cdrom::AutoDetect=0")
    APT_OPTIONS+=(-o "Acquire::cdrom::mount=/cdrom")
    APT_OPTIONS+=(-o "APT::CDROM::NoMount=1")
    apt-cdrom "${APT_OPTIONS[@]}" add
fi

# Install bootloader packages
apt-get install -y "${APT_OPTIONS[@]}" "${INSTALL_PKGS[@]}"

# Disable APT cdrom
if [ -d "/cdrom" ]
then
    sed -i 's/deb cdrom:/# deb cdrom:/g' /etc/apt/sources.list
fi

echo "ROOT_UUID = $ROOT_UUID"

BOOT_OPTIONS="quiet loglevel=0 systemd.show_status=false splash"

# Update bootloader configuration
if hash kernelstub
then
    kernelstub \
        --esp-path "/boot/efi" \
        --kernel-path "/vmlinuz" \
        --initrd-path "/initrd.img" \
        --options "${BOOT_OPTIONS}" \
        --loader \
        --manage-only \
        --force-update \
        --verbose
else
    update-grub
fi

RECOVERY_UUID="$(findmnt -n -o UUID /recovery)" || true

# Prepare recovery partition, if it exists
if [ -d "/boot/efi" -a -d "/cdrom" -a -n ${RECOVERY_UUID} ]
then
    EFI_UUID="$(findmnt -n -o UUID /boot/efi)"

    CDROM_UUID="$(findmnt -n -o UUID /cdrom)"

    CASPER="casper-${RECOVERY_UUID}"
    RECOVERY="Recovery-${RECOVERY_UUID}"

    if [ $RECOVERY_UUID != $CDROM_UUID ]; then
        # Copy .disk, dists, and pool
        rsync -KLav "/cdrom/.disk" "/cdrom/dists" "/cdrom/pool" "/recovery"

        #    Copy casper to special path
        rsync -KLav "/cdrom/casper/" "/recovery/${CASPER}"
    fi
    #
    # # Make a note that the device is a recovery partition
    # # The installer will check for this file's existence.
    # touch "/recovery/recovery"

    # Create configuration file
    cat > "/recovery/recovery.conf" << EOF
HOSTNAME=${HOSTNAME}
LANG=${LANG}
KBD_LAYOUT=${KBD_LAYOUT}
KBD_MODEL=${KBD_MODEL}
KBD_VARIANT=${KBD_VARIANT}
EFI_UUID=${EFI_UUID}
RECOVERY_UUID=${RECOVERY_UUID}
ROOT_UUID=${ROOT_UUID}
LUKS_UUID=${LUKS_UUID}
OEM_MODE=0
EOF

    # Copy initrd and vmlinuz to EFI partition
    mkdir -p "/boot/efi/EFI/${RECOVERY}"
    cp -v "/recovery/${CASPER}/initrd.gz" "/boot/efi/EFI/${RECOVERY}/initrd.gz"
    cp -v "/recovery/${CASPER}/vmlinuz.efi" "/boot/efi/EFI/${RECOVERY}/vmlinuz.efi"

    # Create bootloader configuration
    cat > "/boot/efi/loader/entries/${RECOVERY}.conf" << EOF
title ${NAME} Recovery
linux /EFI/${RECOVERY}/vmlinuz.efi
initrd /EFI/${RECOVERY}/initrd.gz
options ${BOOT_OPTIONS} boot=casper hostname=recovery userfullname=Recovery username=recovery live-media-path=/${CASPER} noprompt
EOF

fi

# This is allowed to fail
if [ $DISABLE_NVIDIA ]; then
    systemctl disable nvidia-fallback.service || true
fi

# Update the chroot's initramfs
update-initramfs -u

# Fix an issue with keyboard locales not being set in the initramfs.
sudo ln -s /etc/console-setup/cached_UTF-8_del.kmap.gz /etc/console-setup/cached.kmap.gz
update-initramfs -u
