# Arch Linux packages

- `dropshade/PKGBUILD` builds from the source of a release tag.
- `dropshade-bin/PKGBUILD` repackages the release tarball. CI builds it with
  `scripts/package-arch.sh` and attaches the `.pkg.tar.zst` to each release.

release-please bumps `pkgver` in both. The checksums stay `SKIP` here and are filled in when
publishing.

## Publishing to the AUR

After a release, for each of `dropshade` and `dropshade-bin`:

```sh
git clone ssh://aur@aur.archlinux.org/dropshade-bin.git ~/aur/dropshade-bin
cp packaging/arch/dropshade-bin/PKGBUILD ~/aur/dropshade-bin/
cd ~/aur/dropshade-bin
updpkgsums                                # from pacman-contrib
makepkg --printsrcinfo > .SRCINFO
makepkg -si                               # test the install
git add PKGBUILD .SRCINFO && git commit -m "Update to <version>" && git push
```

Reset `pkgrel` to 1 for a new version; bump it to rebuild the same version.
