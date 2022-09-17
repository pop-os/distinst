#!/bin/bash
#
# Install the nvidia DRM drivers in initramfs.
# This script is nearly identical to the .postinstall script
# In the pop-os nvidia driver packaging.
#
# Copyright (C) 2022 System76
# Authors: Brock Szuszczewicz
set -e

# Make sure 120M is available in ESP
is_enough_esp_space () {
    efi=($(df | grep /boot/efi)) \
    && echo $((${efi[3]} >= 120000)) \
    || echo 0
}

is_nvidia_system () {
    nvidia-detector | grep -q None
    echo $?
}

# Return true if initramfs module is not added, or added incorrectly
is_driver_added () {
    [[ -r /usr/share/initramfs-tools/modules.d/system76-nvidia-initramfs.conf ]] \
    && [[ "$(cat /usr/share/initramfs-tools/modules.d/system76-nvidia-initramfs.conf)" == $(echo -e "# Added by System76 Distinst install script\nnvidia-drm\n") ]] \
    && echo 1 \
    || echo 0
}

if [[ `is_nvidia_system` = 1 ]] && [[ `is_enough_esp_space` = 1 ]] && [[ `is_driver_added` = 0 ]]; then
    echo $'# Added by System76 Distinst install script\nnvidia-drm\n' > "/usr/share/initramfs-tools/modules.d/system76-nvidia-initramfs.conf"
    update-initramfs -u
fi
