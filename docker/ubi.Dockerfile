# Userspace UBI/UBIFS tooling. Alpine's mtd-utils package carries ubiformat,
# ubiattach, ubinize and mkfs.ubifs.
FROM alpine:3.20
RUN apk add --no-cache mtd-utils
