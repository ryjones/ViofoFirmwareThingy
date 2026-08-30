#!/bin/sh
# Load the MTD/UBI modules into the colima VM kernel and create a NAND
# simulator laid out like the camera's flash. Runs in the `kmod` container.
set -e

BAKED=$(cat /baked-kernel)
RUNNING=$(uname -r)
if [ "$BAKED" != "$RUNNING" ]; then
    echo "kernel mismatch: image has modules for $BAKED, VM is running $RUNNING" >&2
    echo "rebuild with: docker compose build --no-cache kmod" >&2
    exit 1
fi

modprobe ubi
modprobe ubifs

# Two MTD partitions sized exactly like the camera's NAND slots, in erase
# blocks of 128 KiB: rootfs 0x6920000 = 840, app 0xDA0000 = 109. Whatever is
# left of the 128 MiB device becomes a third, unused partition.
#
# Using the real sizes means an image that attaches here also fits on the
# camera -- if it does not, ubiformat says so.
PARTS=${PARTS:-840,109}
WANT_MTDS=$(( $(echo "$PARTS" | tr ',' '\n' | wc -l) + 1 ))

reload=1
if lsmod | grep -q '^nandsim'; then
    if [ "$(grep -c '^mtd' /proc/mtd)" -eq "$WANT_MTDS" ]; then
        reload=0
    else
        echo ">> nandsim is loaded with a different layout; reloading"
        i=0
        while [ $i -lt "$WANT_MTDS" ]; do ubidetach -m $i 2>/dev/null || true; i=$((i+1)); done
        rmmod nandsim || {
            echo "nandsim is busy (a volume is still attached or mounted)." >&2
            echo "close any running 'ubi' container, or run: colima restart" >&2
            exit 1
        }
    fi
fi

# 0x20,0xf1,0x00,0x1d = 128 MiB, 2048-byte pages, 128 KiB erase blocks, 64 B OOB
[ "$reload" -eq 0 ] || modprobe nandsim \
    first_id_byte=0x20 second_id_byte=0xf1 third_id_byte=0x00 fourth_id_byte=0x1d \
    parts="$PARTS"

echo "nandsim ready:"
grep '^mtd' /proc/mtd
