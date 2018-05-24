#!/bin/sh
FS="/cdrom/casper/filesystem.squashfs"
REMOVE="/cdrom/casper/filesystem.manifest-remove"

if ! test -e "target/debug/distinst"; then
    cargo build --manifest-path cli/Cargo.toml
fi

if ! test "${1}"; then
    echo "must provide a block device as an argument"
    exit 1
fi

if ! test -b "${1}"; then
    echo "provided argument is not a block device"
    exit 1
fi

for file in "$FS" "$REMOVE"; do
    if ! test -e "${file}"; then
        echo "failed to find ${file}"
        exit 1
    fi
done

set -e -x

# Install
sudo target/debug/distinst \
    -s "${FS}" \
    -r "${REMOVE}" \
    -h "pop-testing" \
    -k "us" \
    -l "en_US.UTF-8" \
    --force-bios \
    -b "$1" \
    -t "$1:msdos" \
    -n "$1:primary:start:512M:ntfs" \
    -n "$1:primary:512M:51200M:ntfs:mount=/win" \
    -n "$1:primary:51200M:71680M:ext4:mount=/" \
    -n "$1:logical:71682M:102400M:ext4" \
    -n "$1:logical:102400M:-4096M:ext4" \
    -n "$1:logical:-4096M:end:swap"

# Reinstall
sudo target/debug/distinst \
    -s "${FS}" \
    -r "${REMOVE}" \
    -h "pop-testing" \
    -k "us" \
    -l "en_US.UTF-8" \
    --force-bios \
    -b "$1" \
    -u "$1:2:ntfs:mount=/win" \
    -u "$1:3:ext4:mount=/" \
    -u "$1:6:ext4:mount=/home" \
    -u "$1:7:swap"
