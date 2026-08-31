# Reverse-engineering shell for the extracted binaries.
#
# macOS ships an LLVM objdump that refuses cardv outright ("invalid section
# index: 32" -- its section table has an out-of-range link). GNU binutils reads
# the same file without complaint, which is the main reason this image exists.
# python3-capstone is here for scripted disassembly; the helpers in tools/re
# need nothing but the standard library. The aarch64 cross toolchain builds the
# LD_PRELOAD shim in tools/cfgapply.
FROM debian:bookworm-slim

RUN apt-get update -qq \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      binutils python3 python3-capstone file xxd less \
      gcc-aarch64-linux-gnu libc6-dev-arm64-cross make \
 && rm -rf /var/lib/apt/lists/*

ENV CARDV=/work/re/cardv
WORKDIR /work
CMD ["/bin/bash"]
