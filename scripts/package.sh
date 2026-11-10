#!/usr/bin/env bash
# Builds the release artifacts for this machine's architecture into dist/: a binary tarball,
# .deb, .rpm, AppImage, the GNOME Shell extension zip and SHA256SUMS. CI runs the same script.
# Needs cargo-deb, cargo-generate-rpm, zip and linuxdeploy (or $LINUXDEPLOY) on the PATH;
# SKIP_APPIMAGE=1 leaves out the AppImage.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
app_id="io.github.samuelba.Dropshade"
uuid="dropshade@samuelba.github.io"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
arch="$(uname -m)"
dist="$root/dist"
stage="$root/target/package"

rm -rf -- "$dist" "$stage"
mkdir -p "$dist" "$stage"
cargo build --release --locked -p dropshade
binary="$root/target/release/dropshade"

echo "== GNOME Shell extension"
mkdir -p "$stage/extension/schemas"
cp gnome-extension/*.js gnome-extension/metadata.json gnome-extension/LICENSE "$stage/extension/"
cp gnome-extension/schemas/*.gschema.xml "$stage/extension/schemas/"
(cd "$stage/extension" && zip -qr "$dist/$uuid.shell-extension.zip" .)

echo "== tarball"
bundle="dropshade-$version-$arch-linux"
mkdir -p "$stage/$bundle"
cp "$binary" README.md LICENSE "$stage/$bundle/"
cp -r packaging "$stage/$bundle/packaging"
cp -r "$stage/extension" "$stage/$bundle/gnome-extension"
tar -C "$stage" -czf "$dist/$bundle.tar.gz" "$bundle"

echo "== deb and rpm"
cargo deb -p dropshade --no-build --output "$dist/"
cargo generate-rpm -p crates/dropshade --output "$dist/"

if [[ "${SKIP_APPIMAGE:-0}" == "1" ]]; then
    (cd "$dist" && sha256sum -- * > "SHA256SUMS-$arch")
    ls -l "$dist"
    exit 0
fi

echo "== AppImage"
appdir="$stage/AppDir"
install -Dm755 "$binary" "$appdir/usr/bin/dropshade"
install -Dm644 "packaging/icons/hicolor/symbolic/apps/$app_id-symbolic.svg" \
    "$appdir/usr/share/icons/hicolor/symbolic/apps/$app_id-symbolic.svg"
install -Dm644 "packaging/$app_id.metainfo.xml" "$appdir/usr/share/metainfo/$app_id.appdata.xml"
# winit loads these with dlopen, so linuxdeploy cannot find them on its own.
libraries=()
for library in libxkbcommon.so.0 libwayland-client.so.0 libwayland-cursor.so.0; do
    path="$(ldconfig -p | awk -v lib="$library" '$1 == lib { print $NF; exit }')"
    [[ -n "$path" ]] && libraries+=(--library "$path")
done
(
    cd "$stage"
    APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$arch" LINUXDEPLOY_OUTPUT_VERSION="$version" \
        "${LINUXDEPLOY:-linuxdeploy}" --appdir "$appdir" \
        --desktop-file "$root/packaging/$app_id.desktop" \
        --icon-file "$root/packaging/icons/hicolor/scalable/apps/$app_id.svg" \
        "${libraries[@]}" --output appimage
)
mv "$stage"/Dropshade-*.AppImage "$dist/Dropshade-$version-$arch.AppImage"

(cd "$dist" && sha256sum -- * > "SHA256SUMS-$arch")
ls -l "$dist"
