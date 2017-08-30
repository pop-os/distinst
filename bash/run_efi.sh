#!/usr/bin/env bash

set -e

cp /usr/share/OVMF/OVMF_VARS.fd efivars.img
qemu-system-x86_64 -enable-kvm -m 2048 -vga qxl -hda efi.img -boot c \
	-drive if=pflash,format=raw,readonly,file=/usr/share/OVMF/OVMF_CODE.fd \
	-drive if=pflash,format=raw,file=efivars.img
