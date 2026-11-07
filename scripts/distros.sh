#!/bin/sh
# Runs the 系统基础信息 item in containers of many distributions (design system-info, S-05).
# In each, the 系统 line must equal the image's own os-release PRETTY_NAME and the item must
# complete. CI runs it on both architectures; README "Gates" documents it.
#
# Usage: scripts/distros.sh <path to a static lowendaff-bench>
# Needs Docker. Images are pinned by digest. They come from AWS ECR Public, except openSUSE,
# which publishes on its own registry; Arch Linux has no aarch64 image.
set -eu

fail() {
    echo "distros: $*" >&2
    exit 1
}

[ $# -eq 1 ] || fail "usage: scripts/distros.sh <path to lowendaff-bench>"
bin=$(realpath "$1")
ecr=public.ecr.aws/docker/library
images="
$ecr/debian:11@sha256:6f519a81440354a85eb592c5f32109ab80605f6b892455983a6f618bf87fabe9
$ecr/debian:13@sha256:9c6e381421d0f48d1b8133e573f33c185ee16cbf505eda9c4a8a4791b25ea8a6
$ecr/ubuntu:20.04@sha256:8feb4d8ca5354def3d8fce243717141ce31e2c428701f6682bd2fafe15388214
$ecr/ubuntu:24.04@sha256:fe81560990fae86dc4743202b2cb4d3df98d0bcf949173cecef15ce1127ef852
$ecr/centos:7@sha256:be65f488b7764ad3638f236b7b515b3678369a5124c47b8d32916d6487418ea4
$ecr/rockylinux:9@sha256:d7be1c094cc5845ee815d4632fe377514ee6ebcf8efaed6892889657e5ddaaa6
$ecr/almalinux:8@sha256:2d0f2eb2a83cc09dbc2eb92fade4dbe8f709628149e81aa00aec70630406603b
$ecr/fedora:45@sha256:a7d72ba2745b100a3351087472e5ee7b2450af7cf331e7673007e3817612839c
$ecr/alpine:3.22@sha256:bfe7ceb9f09ec0566c48fb93356e45a47a00be5a1dd14fa5b909e7709c66b034
registry.opensuse.org/opensuse/leap:16.0@sha256:9c88da273b41600dfd12790849dfb198ddefed3a109eaf42ada944afb4282a9d
public.ecr.aws/amazonlinux/amazonlinux:2023@sha256:381352c430f36d8aa6b8b670de06e7e4e5642fe0cb9dbd463bb8d1e79aefc0c7
$ecr/oraclelinux:9@sha256:06f1c40f41e4026172cc2065103e313e454747ca48d40d165097c5297d764b38
"
if [ "$(uname -m)" = x86_64 ]; then
    images="$images $ecr/archlinux:latest@sha256:b1c2fe6e717bfd5673af8fc217e1e1c3ce0dcbb1d53c3414c8425e7ce79fc6d9"
fi

for image in $images; do
    name=${image%@*}
    expected=$(docker run --rm --entrypoint sh "$image" -c '. /etc/os-release && printf %s "$PRETTY_NAME"') ||
        fail "$name: cannot read os-release"
    output=$(docker run --rm -v "$bin:/usr/local/bin/lowendaff-bench:ro" -w /root "$image" \
        lowendaff-bench --only system.container 2>&1) || fail "$name: exited with $?: $output"
    printf '%s\n' "$output" | grep -qx '运行环境：Docker 容器' ||
        fail "$name: the platform was not detected as Docker: $output"
    reported=$(printf '%s\n' "$output" | sed -n 's/^  系统  *//p')
    [ "$reported" = "$expected" ] || fail "$name: 系统 is \"$reported\", expected \"$expected\""
    printf '%s\n' "$output" | grep -q '^  完成，用时' || fail "$name: the item did not complete: $output"
    echo "distros: $name: $reported"
done
echo "distros: ok"
