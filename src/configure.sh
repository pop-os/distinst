#!/usr/bin/env bash

set -ex

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

locale-gen --purge "${LANG}"

apt-get purge -y casper ubiquity
apt-get autoremove -y --purge

apt-get install -y "$@"

update-grub
