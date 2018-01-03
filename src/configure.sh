# Exit on error and trace commands
set -ex

# Set up environment
export DEBIAN_FRONTEND=noninteractive
export HOME=/root
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

# Generate a machine ID
dbus-uuidgen > /var/lib/dbus/machine-id

# Correctly specify resolv.conf
ln -sf ../run/resolvconf/resolv.conf /etc/resolv.conf

# Create fstab
echo "# /etc/fstab: static file system information." | tee /etc/fstab
echo "# <file system> <mount point> <type> <options> <dump> <pass>" | tee -a /etc/fstab

ROOTDEV="$(df --output=source / | sed 1d)"
ROOTUUID="$(blkid -o value -s UUID "${ROOTDEV}")"
echo "# / was on ${ROOTDEV} during installation" | tee -a /etc/fstab
echo "UUID=${ROOTUUID} / ext4 errors=remount-ro 0 1" | tee -a /etc/fstab

if [ -d /boot/efi/ ]
then
    EFIDEV="$(df --output=source /boot/efi/ | sed 1d)"
    if [ "${EFIDEV}" != "${ROOTDEV}" ]
    then
        EFIUUID="$(blkid -o value -s UUID "${EFIDEV}")"
        echo "# /boot/efi was on ${EFIDEV} during installation" | tee -a /etc/fstab
        echo "UUID=${EFIUUID} /boot/efi vfat umask=0077 0 1" | tee -a /etc/fstab
    fi
fi

# Update locales
locale-gen --purge "${LANG}"
update-locale --reset "LANG=${LANG}"

# Remove installer packages
apt-get purge -y "${PURGE_PKGS[@]}"
apt-get autoremove -y --purge

# Install grub packages
apt-get install -y "${INSTALL_PKGS[@]}"
update-grub