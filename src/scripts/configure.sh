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

echo "ROOT_UUID = $ROOT_UUID"

# Update bootloader configuration
if [ -d "/boot/efi" ]
then
    kernelstub \
        --esp-path "/boot/efi" \
        --kernel-path "/vmlinuz" \
        --initrd-path "/initrd.img" \
        --options "quiet loglevel=0 vga=current" \
        --loader \
        --manage-only \
        --verbose
else
    update-grub
fi
