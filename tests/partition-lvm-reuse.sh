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
    echo "'${1}' is not a block device"
    exit 1
fi

for file in "$FS" "$REMOVE"; do
    if ! test -e "${file}"; then
        echo "failed to find ${file}"
        exit 1
    fi
done

set -e -x

echo 'Running LVM on LUKS test'
index=0; while test ${index} -ne ${RUNS}; do
    sudo target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -b "$1" \
        -t "$1:gpt" \
        -n "$1:primary:start:512M:fat32:mount=/boot/efi:flags=esp" \
        -n "$1:primary:512M:2048M:lvm=data" \
        -n "$1:primary:2048M:end:lvm=data" \
        --logical "data:root:-4096M:ext4:mount=/" \
        --logical "data:swap:4080M:swap"

    sudo target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -b "$1" \
        -u "$1:1:reuse:mount=/boot/efi:flags=esp" \
        --logical-modify "data:root:fs=xfs:mount=/" \
        --logical-remove "data:swap" \
        --logical "data:other:4080M:ext4"

    index=$((index + 1))
done