#!/bin/sh

set -e -x

DISK="tests/loopback.bin"

if [ ! -e "${DISK}" ]; then
    echo "loopback.sh must be run first to generate ${DISK}"
    exit 1
fi

kvm \
    -m 2G \
    -bios /usr/share/ovmf/OVMF.fd \
    -vga qxl \
    -drive file="${DISK}",format=raw
