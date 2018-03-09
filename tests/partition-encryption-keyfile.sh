#!/bin/sh
FS="tests/filesystem.squashfs"
REMOVE="tests/filesystem.manifest-remove"
RUNS=3

if ! test -e "target/debug/distinst"; then
    cargo build --manifest-path cli/Cargo.toml
fi

for block in "${1}" "${2}"; do
    if ! test "${block}"; then
        echo "must provide two block devices as an argument: /dev/DEV /dev/DEVPART"
        exit 1
    fi

    if ! test -b "${block}"; then
        echo "'${block}' is not a block device"
        exit 1
    fi
done

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
            -b "$2" \
            -t "$1:gpt" \
            -t "$2:gpt" \
            -n "$1:primary:start:512M:fat32:mount=/boot/efi:flags=esp" \
            -n "$1:primary:512M:20000M:enc=cryptroot,root,pass=password" \
            -n "$1:primary:20000M:end:enc=cryptdata,data,keyfile=K" \
            -n "$2:primary:start:512M:ext4:keyid=K:mount=/etc/cryptkeys" \
            --logical "root:root:100%:ext4:mount=/" \
            --logical "data:home:-4096M:ext4:mount=/home" \
            --logical "data:swap:4096M:swap"

    index=$((index + 1))
done