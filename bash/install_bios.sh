#!/usr/bin/env bash

if [ ! -f "$1" ]
then
    echo "$0 [squashfs]" >&2
    exit 1
fi
SQUASHFS="$(realpath "$1")"

IMAGE=bios.img

set -ex

dd if=/dev/zero of="${IMAGE}" bs=1G count=8
parted -s "${IMAGE}" mklabel msdos
parted -s "${IMAGE}" print
parted -s "${IMAGE}" mkpart primary ext4 0% 100%
parted -s "${IMAGE}" print

LO="$(sudo losetup --find --partscan --show "${IMAGE}")"

sudo mkfs.ext4 "${LO}p1"

DIR="$(mktemp -d)"

sudo mount "${LO}p1" "$DIR"

sudo unsquashfs -f -d "${DIR}/" "$SQUASHFS"

sudo mount --bind /dev "${DIR}/dev"
sudo mount --bind /proc "${DIR}/proc"
sudo mount --bind /sys "${DIR}/sys"

ROOTDEV="$(sudo chroot "${DIR}/" df --output=source / | sed 1d)"
ROOTUUID="$(lsblk -n -o UUID "${ROOTDEV}")"
echo "# / was on ${ROOTDEV} during installation" | sudo chroot "${DIR}/" tee /etc/fstab
echo "UUID=${ROOTUUID} / ext4 errors=remount-ro 0 1" | sudo chroot "${DIR}/" tee -a /etc/fstab

sudo chroot "${DIR}/" apt install -y xterm grub-pc
sudo chroot "${DIR}/" apt purge -y casper ubiquity
sudo chroot "${DIR}/" apt autoremove -y --purge

sudo chroot "${DIR}/" grub-mkconfig -o /boot/grub/grub.cfg

sudo grub-install --recheck --target=i386-pc --boot-directory="${DIR}/boot/" "${LO}"

sudo umount "${DIR}/dev"
sudo umount "${DIR}/proc"
sudo umount "${DIR}/sys"

sudo umount "${DIR}" || sudo umount -lf "${DIR}"

rmdir "${DIR}"

sudo losetup -d "${LO}"
