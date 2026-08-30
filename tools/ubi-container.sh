#!/bin/sh
# Runs inside the `ubi` container (see docker-compose.yml).
#
#   docker compose run --rm ubi
#
# Mounts both UBI partitions the way the camera does:
#
#   ~/viofo            <- 07-rootfs.ubi   (ubi0_0, from mtd0)
#   ~/viofo/mnt/app    <- 09-app.ubi      (ubi1_0, from mtd1)
#
# /mnt/app is where /etc/init.d/S07_APP_Overlay mounts the app volume on the
# device, and it attaches as ubi1_0 there too, so this is the real layout.
#
# Edit under ~/viofo, then run `rebuild` to write the .ubi images back.
set -e

ROOT=${VIOFO_MNT:-$HOME/viofo}
APP_SUBDIR=${APP_SUBDIR:-mnt/app}
APPMNT="$ROOT/$APP_SUBDIR"

ROOTFS_IMG=${ROOTFS_IMG:-/work/unpacked/07-rootfs.ubi}
APP_IMG=${APP_IMG:-/work/unpacked/09-app.ubi}

VID_OFF=${VID_OFF:-2048}   # camera puts the VID header in page 1
COMPR=${COMPR:-lzo}        # UBIFS default_compr of the stock images

command -v ubiformat >/dev/null || apk add --no-cache mtd-utils >/dev/null
[ -c /dev/mtd0 ] && [ -c /dev/mtd1 ] || {
    echo "need /dev/mtd0 and /dev/mtd1 -- did the kmod service run?" >&2; exit 1; }

# Tear down anything left over from a previous session.
umount "$APPMNT" 2>/dev/null || true
umount "$ROOT"   2>/dev/null || true
ubidetach -m 0 2>/dev/null || true
ubidetach -m 1 2>/dev/null || true

attach() {   # attach <mtd number> <image>
    ubiformat "/dev/mtd$1" -f "$2" -O "$VID_OFF" -y -q
    ubiattach -m "$1" -O "$VID_OFF" >/dev/null
}

echo ">> $(basename "$ROOTFS_IMG") -> mtd0"; attach 0 "$ROOTFS_IMG"
echo ">> $(basename "$APP_IMG") -> mtd1";    attach 1 "$APP_IMG"

# Every rebuild parameter comes from the attached volume, not from hardcoded
# numbers, so this stays correct if an image's geometry ever differs.
params() {   # params <ubi device number>
    ubinfo "/dev/ubi$1_0" | awk -F': *' '/^Name:/{printf "%s ", $2}'
    ubinfo "/dev/ubi$1_0" | awk '/^Size:/{printf "%s ", $2}'
    ubinfo "/dev/ubi$1"   | awk '/^Logical eraseblock size:/{printf "%s ", $4}'
    ubinfo "/dev/ubi$1"   | awk '/^Minimum input\/output unit size:/{print $5}'
}
set -- $(params 0); R_VOL=$1 R_LEBS=$2 R_LEB=$3 R_MINIO=$4
set -- $(params 1); A_VOL=$1 A_LEBS=$2 A_LEB=$3 A_MINIO=$4
PEB=$(cat /sys/class/mtd/mtd0/erasesize)

mkdir -p "$ROOT"
mount -t ubifs /dev/ubi0_0 "$ROOT"
mount -t ubifs /dev/ubi1_0 "$APPMNT"     # the mountpoint lives in the rootfs

# --- rebuild ------------------------------------------------------------
cat > /usr/local/bin/rebuild <<REBUILD
#!/bin/sh
# rebuild [rootfs|app|all]   -- turn the live mounts back into .ubi images
set -e

pack() {   # pack <mountpoint> <volname> <lebs> <lebsize> <minio> <outfile>
    tmp=\$(mktemp -d)
    echo ">> mkfs.ubifs -r \$1 -m \$5 -e \$4 -c \$3 -x $COMPR"
    mkfs.ubifs -r "\$1" -m "\$5" -e "\$4" -c "\$3" -x $COMPR -o "\$tmp/vol.ubifs"
    cat > "\$tmp/ubinize.cfg" <<CFG
[\$2]
mode=ubi
image=\$tmp/vol.ubifs
vol_id=0
vol_type=dynamic
vol_name=\$2
vol_size=\$((\$3 * \$4))
CFG
    echo ">> ubinize -m \$5 -p $PEB -s \$5 -> \$6"
    ubinize -o "\$6" -m "\$5" -p $PEB -s "\$5" "\$tmp/ubinize.cfg"
    rm -rf "\$tmp"
    ls -l "\$6"
}

do_app() {
    pack "$APPMNT" "$A_VOL" "$A_LEBS" "$A_LEB" "$A_MINIO" "\${APP_OUT:-$APP_IMG}"
}

do_rootfs() {
    # The app volume is mounted inside the rootfs tree; mkfs.ubifs has no
    # exclude option, so drop it for the duration or its files land in the
    # rootfs image.
    umount "$APPMNT"
    pack "$ROOT" "$R_VOL" "$R_LEBS" "$R_LEB" "$R_MINIO" "\${ROOTFS_OUT:-$ROOTFS_IMG}" || {
        mount -t ubifs /dev/ubi1_0 "$APPMNT"; return 1; }
    mount -t ubifs /dev/ubi1_0 "$APPMNT"
}

case "\${1:-all}" in
    rootfs) do_rootfs ;;
    app)    do_app ;;
    all)    do_app; do_rootfs ;;
    *)      echo "usage: rebuild [rootfs|app|all]" >&2; exit 1 ;;
esac
echo ">> now on the host:  ./target/release/viofo-fw pack unpacked -o FWA329S.bin"
REBUILD
chmod +x /usr/local/bin/rebuild

printf '\n'
printf '  %-28s <- %-7s %4s LEBs x %s B, min I/O %s\n' "$ROOT"   "$R_VOL" "$R_LEBS" "$R_LEB" "$R_MINIO"
printf '  %-28s <- %-7s %4s LEBs x %s B, min I/O %s\n' "$APPMNT" "$A_VOL" "$A_LEBS" "$A_LEB" "$A_MINIO"
cat <<BANNER

  both read-write, PEB $PEB, compression $COMPR

  edit under $ROOT, then run:
      rebuild           both images
      rebuild rootfs    just $(basename "$ROOTFS_IMG")
      rebuild app       just $(basename "$APP_IMG")
  exit when done.

BANNER

[ -n "$NONINTERACTIVE" ] && exit 0
cd "$ROOT"
exec /bin/sh
