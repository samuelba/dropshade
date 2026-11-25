#!/usr/bin/env bash
# Builds a bundle for trying Dropshade on a test machine, such as a VM with GNOME 51, KDE or
# COSMIC, without a release:
#
#   scripts/test-bundle.sh                       build in an Ubuntu 22.04 container, like a release
#   scripts/test-bundle.sh --base ubuntu:24.04   build in another distribution's container
#   scripts/test-bundle.sh --native              build with this machine's toolchain and glibc
#
# Writes dist/test/dropshade-<version>-<change>-<arch>-test.tar.gz. On the test machine:
#
#   tar -xzf dropshade-*-test.tar.gz && cd dropshade-*-test
#   ./install.sh [--gnome-extension]   the real installer, from this folder instead of GitHub
#   ./uninstall.sh                     the real uninstaller; the configuration stays
#   ./prune.sh                         uninstall, then remove everything Dropshade left behind
#
# TESTING.md in the bundle lists what to try on each desktop.
#
# The container build needs podman. Its image is dropshade-build:<base>, made on first use
# (--rebuild-image makes it again), and its crate downloads live in the volume dropshade-cargo.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
base="ubuntu:22.04"
native=0
rebuild_image=0

die() {
    printf 'test-bundle: %s\n' "$*" >&2
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --base)
            [[ $# -ge 2 ]] || die "--base needs an image, e.g. ubuntu:24.04"
            base="$2"
            shift
            ;;
        --native) native=1 ;;
        --rebuild-image) rebuild_image=1 ;;
        -h | --help)
            sed -n '2,19s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"
            exit 0
            ;;
        *) die "unknown option $1" ;;
    esac
    shift
done

cd "$root"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
arch="$(uname -m)"
if command -v jj >/dev/null 2>&1 && change="$(jj log -r @ --no-graph -T 'change_id.short(8)' 2>/dev/null)"; then
    [[ -z "$(jj diff -r @ --summary 2>/dev/null)" ]] || change="$change-dirty"
else
    change="$(git rev-parse --short=8 HEAD)"
    git diff --quiet HEAD || change="$change-dirty"
fi

build_in_container() {
    command -v podman >/dev/null 2>&1 || die "needs podman, or use --native"
    local image="localhost/dropshade-build:${base//[:\/]/-}"
    if [[ "$rebuild_image" = 1 ]] || ! podman image exists "$image"; then
        local context
        context="$(mktemp -d)"
        cat >"$context/Containerfile" <<EOF
FROM docker.io/library/$base
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \\
    && apt-get install -y --no-install-recommends ca-certificates curl build-essential \\
        pkg-config libxkbcommon-dev libwayland-dev \\
    && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:\$PATH
RUN curl -fsSL --proto '=https' --tlsv1.2 https://sh.rustup.rs \\
    | sh -s -- -y --profile minimal --default-toolchain stable --component rustfmt,clippy
EOF
        podman build -t "$image" "$context"
        rm -rf -- "$context"
    fi
    target="target/${base//[:\/]/-}"
    # Labels stay off rather than relabelling the source tree for SELinux.
    podman run --rm --security-opt label=disable \
        -v "$root:/src" -w /src \
        -v dropshade-cargo:/opt/cargo/registry \
        -e CARGO_TARGET_DIR="/src/$target" \
        "$image" cargo build --release --locked -p dropshade
}

if [[ "$native" = 1 ]]; then
    target="target"
    cargo build --release --locked -p dropshade
else
    build_in_container
fi
binary="$root/$target/release/dropshade"
[[ -x "$binary" ]] || die "the build left no binary at $binary"

name="dropshade-$version-$change-$arch-test"
stage="$root/target/test-bundle"
out="$root/dist/test"
rm -rf -- "$stage"
mkdir -p "$stage/$name" "$out"

cp "$binary" README.md LICENSE "$stage/$name/"
cp -r packaging "$stage/$name/packaging"
rm -rf -- "$stage/$name/packaging/minisign.pub" "$stage/$name/packaging/arch" \
    "$stage/$name/packaging/repo"
cp scripts/install.sh "$stage/$name/installer.sh"
cp scripts/test-bundle/install.sh scripts/test-bundle/uninstall.sh scripts/test-bundle/prune.sh \
    scripts/test-bundle/TESTING.md "$stage/$name/"
chmod 755 "$stage/$name/"*.sh

rm -f -- "$out/$name.tar.gz"
tar -C "$stage" -czf "$out/$name.tar.gz" "$name"

if command -v objdump >/dev/null 2>&1; then
    glibc="$(objdump -T "$binary" | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -n 1)"
    echo "The binary needs glibc ${glibc#GLIBC_} or newer (Ubuntu 22.04 has 2.35, 24.04 has 2.39)."
fi
echo "Wrote $out/$name.tar.gz"
echo "Copy it to the VM, e.g.: scp $out/$name.tar.gz user@vm:"
