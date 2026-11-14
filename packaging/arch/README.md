# Arch Linux packages

- `dropshade/PKGBUILD` builds from the source of a release tag.
- `dropshade-bin/PKGBUILD` repackages the release tarball. CI builds it with
  `scripts/package-arch.sh` and attaches the `.pkg.tar.zst` to each release.

release-please bumps `pkgver` in both. The checksums stay `SKIP` here;
`scripts/aur-publish.sh` fills them in when publishing.

## Publishing to the AUR

Each release publishes both packages through the AUR workflow
(`.github/workflows/aur.yml`), once the `AUR_SSH_PRIVATE_KEY` secret is set. Without it the
workflow only leaves a notice.

### One-time setup

1. Register at <https://aur.archlinux.org/register>.
2. Make a key just for publishing, without a passphrase, since CI uses it:

   ```sh
   ssh-keygen -t ed25519 -N "" -C "dropshade AUR" -f ~/.ssh/aur
   ```

3. Paste `~/.ssh/aur.pub` into My Account → SSH Public Key on the AUR.
4. Give the private key to GitHub:

   ```sh
   gh secret set AUR_SSH_PRIVATE_KEY < ~/.ssh/aur
   ```

5. Publish the current release: Actions → AUR → Run workflow. The first push creates both
   packages on the AUR.

### By hand

On Arch, with `pacman-contrib` installed:

```sh
DRY_RUN=1 ./scripts/aur-publish.sh     # fills in checksums and .SRCINFO under target/aur/
./scripts/aur-publish.sh               # the same, then pushes (needs the key in ~/.ssh/config)
./scripts/aur-publish.sh dropshade-bin # only one package
```

To fix a package without a new release, bump `pkgrel` in its PKGBUILD, merge, and run the
workflow by hand. Reset `pkgrel` to 1 with the next version.
