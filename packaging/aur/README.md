# Matrix Studio AUR recipe

`fw16-ledmatrix` builds the Linux daemon, desktop and helper from source. It
pins the public upstream commit for version 1.0.0 and verifies the archive
checksum. It does not depend on a GitHub Actions artifact or a binary release.
Installation does not start a service or enable subscription polling.

## Try the recipe before submission

On x86-64 Arch Linux with `base-devel`, run as a normal user in a disposable
build directory:

```bash
git clone https://github.com/jschraub/fw16-ledmatrix.git
cd fw16-ledmatrix/packaging/aur
makepkg --verifysource
makepkg --syncdeps --cleanbuild --clean
```

This produces a local package; `--syncdeps` may ask pacman to install build
dependencies. To test install/upgrade/uninstall without touching a running
daemon, use `packaging/arch/check-package.sh` at the repository root with an
older package and the new package as its two arguments. Local source builds
can embed local paths; do not distribute the resulting binary as an upstream
release.

## Maintainer: submit or update

1. Build and inspect the recipe and check that its version is newer than the
   existing AUR version. For upstream updates change `pkgver`, `_commit`, and
   the source checksum; for packaging-only changes increment `pkgrel`.
2. Regenerate `.SRCINFO` using `makepkg --printsrcinfo > .SRCINFO` in this
   directory. Commit the reviewed recipe and metadata together.
3. With an [AUR account and registered SSH key](https://wiki.archlinux.org/title/AUR_submission_guidelines),
   clone `ssh://aur@aur.archlinux.org/fw16-ledmatrix.git` into a separate
   directory. Copy **only** `PKGBUILD` and `.SRCINFO` into that checkout.
   Review the diff, commit it and push `master`. Do not push the application
   repository, source archives or compiled packages to AUR.
4. Verify the [AUR listing](https://aur.archlinux.org/packages/fw16-ledmatrix)
   and `paru -Si --aur fw16-ledmatrix` before advertising
   `paru -S fw16-ledmatrix` to users.
