#!/bin/sh
FS="tests/filesystem.squashfs"
REMOVE="tests/filesystem.manifest-remove"
RUNS=3

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

echo 'Running resize tests'
index=0; while test ${index} -ne ${RUNS}; do
    sudo target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -k "us" \
        -l "en_US.UTF-8" \
        -b "$1" \
        -t "$1:gpt" \
        -n "$1:primary:start:512M:fat32:mount=/boot/efi:flags=esp" \
        -n "$1:primary:1024M:1536M:ext4:mount=/" \
        -n "$1:primary:2048M:4096M:ext4" \
        -n "$1:primary:-512M:end:swap"

    sudo target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -k "us" \
        -l "en_US.UTF-8" \
        -b "$1" \
        -m "$1:2:1024M:2048M" \
        -m "$1:3:2048M:3584M" \
        -m "$1:4:-1024M:end" \
        -u "$1:1:reuse:mount=/boot/efi:flags=esp" \
        -u "$1:2:reuse:mount=/"

    # If the disk path ends with a number, add a p.
    if test "${1:-1}" -eq "${1:-1}"; then
        DISK="${1}p"
    else
        DISK="${1}"
    fi

    sudo fsck -n "${DISK}1"
    sudo fsck -n "${DISK}2"
    sudo fsck -n "${DISK}3"

    index=$((index + 1))
done