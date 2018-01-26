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

# Generate a machine ID
dbus-uuidgen > "/var/lib/dbus/machine-id"

# Correctly specify resolv.conf
ln -sf "../run/resolvconf/resolv.conf" "/etc/resolv.conf"

# Update locales
locale-gen --purge "${LANG}"
update-locale --reset "LANG=${LANG}"

# Remove installer packages
apt-get purge -y "${PURGE_PKGS[@]}"
apt-get autoremove -y --purge

# Install grub packages
apt-get install -y "${INSTALL_PKGS[@]}"

# Update bootloader configuration
if [ -d "/boot/efi" ]
then
    CMDLINE="$(mktemp)"
    echo "root=UUID=${ROOT_UUID} ro quiet splash" > "${CMDLINE}"

    mkdir -p "/boot/efi/EFI/Linux"
    objcopy \
       --add-section .osrel="/etc/os-release" --change-section-vma .osrel=0x20000 \
       --add-section .cmdline="${CMDLINE}" --change-section-vma .cmdline=0x30000 \
       --add-section .linux="/vmlinuz" --change-section-vma .linux=0x40000 \
       --add-section .initrd="/initrd.img" --change-section-vma .initrd=0x3000000 \
       "/usr/lib/systemd/boot/efi/linuxx64.efi.stub" \
       "/boot/efi/EFI/Linux/${ROOT_UUID}.efi"

    rm "${CMDLINE}"
else
    update-grub
fi
