# Carries the MTD/UBI kernel modules for the colima VM's kernel.
#
# `docker build` runs inside the colima VM, so `uname -r` here is the VM's
# kernel -- the modules baked in match the kernel they will be loaded into.
# Nothing is installed on the VM itself; the modules live in this image and are
# inserted at run time, so a `colima restart` returns the VM to stock.
FROM ubuntu:24.04

RUN apt-get update -qq \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      kmod mtd-utils linux-modules-extra-$(uname -r) \
 && uname -r > /baked-kernel \
 && rm -rf /var/lib/apt/lists/*

COPY tools/kmod-load.sh /usr/local/bin/kmod-load
ENTRYPOINT ["/usr/local/bin/kmod-load"]
