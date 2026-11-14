#!/usr/bin/env bash
# Builds the dropshade-bin pacman package from the tarball that scripts/package.sh left in dist/,
# so it ships the same binary as the other packages. Run it on Arch (not as root); CI runs it in
# an archlinux:base-devel container.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
arch="$(uname -m)"
dist="$root/dist"
stage="$root/target/package/arch"
pkgbuild="packaging/arch/dropshade-bin/PKGBUILD"
tarball="dropshade-$version-$arch-linux.tar.gz"

pkgver="$(sed -n 's/^pkgver=\([^ ]*\).*$/\1/p' "$pkgbuild")"
if [[ "$pkgver" != "$version" ]]; then
    echo "$pkgbuild has pkgver $pkgver, Cargo.toml has $version" >&2
    exit 1
fi
if [[ ! -f "$dist/$tarball" ]]; then
    echo "missing $dist/$tarball: run scripts/package.sh first" >&2
    exit 1
fi

rm -rf -- "$stage"
mkdir -p "$stage"
cp "$pkgbuild" "$dist/$tarball" "$stage/"
(cd "$stage" && PKGDEST="$dist" makepkg --force --cleanbuild --skipinteg --nodeps)

package="$(ls "$dist"/dropshade-bin-"$version"-*-"$arch".pkg.tar.zst)"
if command -v namcap > /dev/null; then
    namcap "$stage/PKGBUILD" "$package" || true
fi
(cd "$dist" && sha256sum -- "$(basename "$package")" > "SHA256SUMS-archlinux-$arch")
ls -l "$package"
