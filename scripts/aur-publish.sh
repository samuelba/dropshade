#!/usr/bin/env bash
# Pushes packaging/arch/<package>/PKGBUILD to the AUR, with checksums and .SRCINFO filled in for
# the release it names. Defaults to both packages. Needs makepkg, updpkgsums (pacman-contrib),
# git and an SSH key for aur@aur.archlinux.org; DRY_RUN=1 stops before the push and needs no key.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$root/target/aur"
packages=("$@")
[[ ${#packages[@]} -eq 0 ]] && packages=(dropshade dropshade-bin)

srcinfo_field() {
    sed -n "s/^\t$1 = //p" .SRCINFO
}

rm -rf -- "$work"
mkdir -p "$work"

for package in "${packages[@]}"; do
    echo "== $package"
    repo="$work/$package"
    git clone --quiet "https://aur.archlinux.org/$package.git" "$repo"
    cp "$root/packaging/arch/$package/PKGBUILD" "$repo/"
    (
        cd "$repo"
        updpkgsums
        makepkg --printsrcinfo > .SRCINFO
        git add PKGBUILD .SRCINFO
        if git diff --cached --quiet; then
            echo "$package is up to date"
            exit 0
        fi
        version="$(srcinfo_field pkgver)-$(srcinfo_field pkgrel)"
        git commit --quiet --message "Update to $version"
        git show --stat HEAD
        if [[ "${DRY_RUN:-0}" == "1" ]]; then
            echo "DRY_RUN: not pushing $package $version"
        else
            git push "ssh://aur@aur.archlinux.org/$package.git" HEAD:master
        fi
    )
done
