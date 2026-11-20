#!/usr/bin/env bash
# Builds the apt and dnf repositories that GitHub Pages serves, from the packages of the last
# published releases, into site/ (or the directory given):
#
#   deb/dists/stable/...       apt metadata        deb/pool/main/*.deb   the packages
#   rpm/repodata/...           dnf metadata        rpm/*.rpm             the packages
#
# Nothing is signed here. The Repository workflow runs this script when a release is published;
# scripts/publish-repo.sh then checks the metadata and signs it on the maintainer's machine, and
# the Repository deploy workflow puts it on GitHub Pages.
#
# Only releases whose SHA256SUMS are signed by the release key go in, the newest $KEEP (default
# 5) of them. Needs gh, sha256sum, minisign or rsign, apt-ftparchive (apt-utils) and createrepo_c.
set -euo pipefail

repo="samuelba/dropshade"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$(realpath -m -- "${1:-$root/site}")"
keep="${KEEP:-5}"
key="$root/packaging/repo/dropshade.asc"

die() {
    printf 'build-repo: %s\n' "$*" >&2
    exit 1
}

[[ -f "$key" ]] || die "no repository key at packaging/repo/dropshade.asc"
for tool in gh sha256sum apt-ftparchive createrepo_c; do
    command -v "$tool" >/dev/null || die "needs $tool"
done

# Replaces only an earlier build or an empty directory, never a mistyped path.
if [[ -e "$out" && ! -f "$out/releases.txt" && -n "$(ls -A -- "$out")" ]]; then
    die "$out is neither empty nor an earlier build"
fi
rm -rf -- "$out"
mkdir -p "$out/deb/pool/main" "$out/rpm"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT

echo "== packages"
releases=()
while read -r tag <&3; do
    [[ ${#releases[@]} -lt $keep ]] || break
    dir="$work/$tag"
    gh release download "$tag" --repo "$repo" --dir "$dir" \
        --pattern SHA256SUMS --pattern SHA256SUMS.minisig 2>/dev/null || true
    if [[ ! -f "$dir/SHA256SUMS.minisig" ]]; then
        echo "$tag: no signed checksums, left out"
        continue
    fi
    "$root/scripts/verify-sums.sh" "$dir" "$tag"
    # sha256sum lines: the hash, a space, a space or "*", and the name.
    grep -E '^[0-9a-f]{64} [ *][^/ ]+\.(deb|rpm)$' "$dir/SHA256SUMS" >"$dir/packages" ||
        die "$tag has no .deb or .rpm"
    gh release download "$tag" --repo "$repo" --dir "$dir" --pattern '*.deb' --pattern '*.rpm'
    (cd "$dir" && sha256sum --check --quiet --strict packages) ||
        die "the packages of $tag do not match their signed checksums"
    while read -r _ name; do
        name="${name#\*}"
        case "$name" in
            *.deb) cp -- "$dir/$name" "$out/deb/pool/main/" ;;
            *.rpm) cp -- "$dir/$name" "$out/rpm/" ;;
        esac
    done <"$dir/packages"
    echo "$tag: $(wc -l <"$dir/packages") packages"
    releases+=("$tag")
done 3< <(gh release list --repo "$repo" --exclude-drafts --exclude-pre-releases --limit 100 \
    --json tagName --jq '.[].tagName')
[[ ${#releases[@]} -gt 0 ]] || die "no published release has signed checksums"
# publish-repo.sh checks the metadata against these releases' signed checksums.
printf '%s\n' "${releases[@]}" >"$out/releases.txt"

echo "== apt repository"
(
    cd "$out/deb"
    # SHA256 only, in Release and in Packages: apt trusts the strongest hash listed, and
    # publish-repo.sh checks just this one against the release's signed checksums.
    sha256_only=(-o APT::FTPArchive::MD5=false -o APT::FTPArchive::SHA1=false
        -o APT::FTPArchive::SHA512=false)
    for arch in amd64 arm64; do
        dir="dists/stable/main/binary-$arch"
        mkdir -p "$dir"
        apt-ftparchive "${sha256_only[@]}" --arch "$arch" packages pool/main >"$dir/Packages"
        gzip -9 --no-name --keep "$dir/Packages"
    done
    apt-ftparchive "${sha256_only[@]}" \
        -o APT::FTPArchive::Release::Origin=Dropshade \
        -o APT::FTPArchive::Release::Label=Dropshade \
        -o APT::FTPArchive::Release::Suite=stable \
        -o APT::FTPArchive::Release::Codename=stable \
        -o APT::FTPArchive::Release::Architectures="amd64 arm64" \
        -o APT::FTPArchive::Release::Components=main \
        -o APT::FTPArchive::Release::Description="Dropshade, a drop-down terminal" \
        release dists/stable >"$work/Release"
    mv -- "$work/Release" dists/stable/Release
)

echo "== dnf repository"
# gzip, so publish-repo.sh reads the metadata with zcat.
createrepo_c --quiet --no-database --general-compress-type=gz "$out/rpm"

cp -- "$key" "$out/dropshade.asc"
cp -- "$root/packaging/repo/index.html" "$out/"
cp -- "$root/packaging/repo/dropshade.sources" "$out/deb/"
cp -- "$root/packaging/repo/dropshade.repo" "$out/rpm/"
find "$out" -type f -not -path '*/pool/*' -not -name '*.rpm' | sort
