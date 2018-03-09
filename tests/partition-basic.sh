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

echo 'Running new partitioning tests'
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
        -n "$1:primary:512M:-4096M:ext4:mount=/" \
        -n "$1:primary:-4096M:end:swap"
    index=$((index + 1))
done

echo 'Running re-use partition tests'
index=0; while test ${index} -ne ${RUNS}; do
    sudo target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -k "us" \
        -l "en_US.UTF-8" \
        -b "$1" \
        -u "$1:1:reuse:mount=/boot/efi:flags=esp" \
        -u "$1:2:ext4:mount=/" \
        -u "$1:3:swap"
    index=$((index + 1))
done

echo 'Running partition removal tests'
index=0; while test ${index} -ne ${RUNS}; do
    sudo target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -k "us" \
        -l "en_US.UTF-8" \
        -b "$1" \
        -d "$1:1:3" \
        -u "$1:2:reuse:mount=/" \
        -n "$1:primary:start:512M:fat32:mount=/boot/efi:flags=esp" \
        -n "$1:primary:-512M:end:swap"
    index=$((index + 1))
done