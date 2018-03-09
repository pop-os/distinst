#!/bin/sh
FS="tests/filesystem.squashfs"
REMOVE="tests/filesystem.manifest-remove"
RUNS=1

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
    sudo env RUST_BACKTRACE=1 target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -k "us" \
        -l "en_US.UTF-8" \
        -b "$1" \
        -t "$1:gpt" \
        -n "$1:primary:start:512M:fat32:mount=/boot/efi:flags=esp" \
        -n "$1:primary:512M:2048M:lvm=data2" \
        -n "$1:primary:2048M:end:lvm=data2" \
        --logical "data2:root:-4096M:ext4:mount=/" \
        --logical "data2:swap:4096M:swap"

    index=$((index + 1))
done