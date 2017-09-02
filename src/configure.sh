#!/usr/bin/env bash

set -ex

ROOTDEV="$(df --output=source / | sed 1d)"
ROOTUUID="$(blkid -o value -s UUID "${ROOTDEV}")"
echo "# / was on ${ROOTDEV} during installation" | tee /etc/fstab
echo "UUID=${ROOTUUID} / ext4 errors=remount-ro 0 1" | tee -a /etc/fstab

if [ -d /boot/efi/ ]
then
    EFIDEV="$(df --output=source /boot/efi/ | sed 1d)"
    EFIUUID="$(blkid -o value -s UUID "${EFIDEV}")"
    echo "# /boot/efi was on ${EFIDEV} during installation" | tee -a /etc/fstab
    echo "UUID=${EFIUUID} /boot/efi vfat umask=0077 0 1" | tee -a /etc/fstab
fi

locale-gen --purge "${LANG}"

apt-get purge -y casper ubiquity
apt-get autoremove -y --purge

if [ -d /boot/efi/ ]
then
    apt-get install -y grub-efi-amd64-signed
else
    apt-get install -y grub-pc
fi

grub-mkconfig -o /boot/grub/grub.cfg
