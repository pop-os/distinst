#!/bin/sh

set -e -x

DISK="tests/loopback.bin"

if [ ! -e "$DISK" ]; then
    dd if=/dev/zero of="$DISK" bs=1G count=16 status=progress
fi

LO="$(sudo losetup --find "$DISK" --show --partscan)"

./tests/install.sh "$LO"

sudo losetup --detach "$LO"
