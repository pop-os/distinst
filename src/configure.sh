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

# Update locales
locale-gen --purge "${LANG}"
update-locale --reset "LANG=${LANG}"

# Remove installer packages
apt-get purge -y "${PURGE_PKGS[@]}"
apt-get autoremove -y --purge

# Install grub packages
apt-get install -y "${INSTALL_PKGS[@]}"
update-grub