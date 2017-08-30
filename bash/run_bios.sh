#!/usr/bin/env bash

set -e

qemu-system-x86_64 -enable-kvm -vga qxl -m 2G -hda bios.img -boot c
