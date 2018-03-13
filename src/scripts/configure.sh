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

# Add the cdrom to APT, if it exists.
if [ -d "/tmp/cdrom" ]
then
    apt-cdrom -d "/tmp/cdrom" -r
fi

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

# Set keyboard
localectl set-keymap "${KBD}"

# Remove installer packages
apt-get purge -y "${PURGE_PKGS[@]}"
apt-get autoremove -y --purge

# Install grub packages
apt-get install -y "${INSTALL_PKGS[@]}"

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
