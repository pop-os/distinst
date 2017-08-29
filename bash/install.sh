#!/usr/bin/env bash

if [ ! -f "$1" ]
then
    echo "$0 [squashfs]" >&2
    exit 1
fi
SQUASHFS="$(realpath "$1")"

set -ex

# dd if=/dev/zero of=test.img bs=1G count=8
# parted -s test.img mklabel msdos
# parted -s test.img print
# parted -s test.img mkpart primary ext4 0% 100%
# parted -s test.img print

LO="$(sudo losetup --find --partscan --show test.img)"

# sudo mkfs.ext4 "${LO}p1"

DIR="$(mktemp -d)"

sudo mount "${LO}p1" "$DIR"

# sudo unsquashfs -f -d "${DIR}/" "$SQUASHFS"

sudo mount --bind /dev "${DIR}/dev"
sudo mount --bind /proc "${DIR}/proc"
sudo mount --bind /sys "${DIR}/sys"

sudo chroot "${DIR}/" apt purge -y casper ubiquity
sudo chroot "${DIR}/" apt autoremove -y --purge
ROOTDEV="$(sudo chroot "${DIR}/" df --output=source / | sed 1d)"
ROOTUUID="$(sudo chroot "${DIR}/" blkid -o value -s UUID "${ROOTDEV}")"
echo "UUID=${ROOTUUID} / ext4 defaults,errors=remount-ro,noatime 0 1" | sudo chroot "${DIR}/" tee /etc/fstab
sudo chroot "${DIR}/" grub-mkconfig -o /boot/grub/grub.cfg

sudo grub-install --recheck --target=i386-pc --boot-directory="${DIR}/boot/" "${LO}"

sudo umount "${DIR}/dev"
sudo umount "${DIR}/proc"
sudo umount "${DIR}/sys"

sudo umount "${DIR}"

rmdir "${DIR}"

sudo losetup -d "${LO}"
