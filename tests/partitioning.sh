#!/bin/sh

FS="tests/filesystem.squashfs"
REMOVE="tests/filesystem.manifest-remove"

if ! test -e "target/debug/distinst"; then
    cargo build --bin distinst
fi

for file in $FS $REMOVE; do
    if ! test -e "${file}"; then
        echo "failed to find ${file}"
        exit 1
    fi
done

set -x

echo 'Running new partitioning tests'
index=0; while test $index -ne 3; do
    sudo target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -k "us" \
        -l "en_US.UTF-8" \
        -b "$1" \
        -t "$1:gpt" \
        -n "$1:primary:start:512M:fat32:/boot/efi:esp" \
        -n "$1:primary:512M:-512M:ext4:/" \
        -n "$1:primary:-512M:end:swap"
    index=$((index + 1))
done

echo 'Running re-use partition tests'
index=0; while test $index -ne 3; do
    sudo target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -k "us" \
        -l "en_US.UTF-8" \
        -b "$1" \
        -u "$1:1:reuse:/boot/efi:esp" \
        -u "$1:2:ext4:/" \
        -u "$1:3:swap"
    index=$((index + 1))
done

echo 'Running partition removal tests'
index=0; while test $index -ne 3; do
    sudo target/debug/distinst --test \
        -s "${FS}" \
        -r "${REMOVE}" \
        -h "pop-testing" \
        -k "us" \
        -l "en_US.UTF-8" \
        -b "$1" \
        -d "$1:1:3" \
        -u "$1:2:reuse:/" \
        -n "$1:primary:start:512M:fat32:/boot/efi:esp" \
        -n "$1:primary:-512M:end:swap"
    index=$((index + 1))
done