# Linux packaging — development candidate (#28)

The first candidate is an **unsigned x86-64 pacman package** for Arch/CachyOS,
the family used for the physical panel checks. It installs the desktop, daemon
and Claude helper in `/usr/bin`, a desktop entry and icon, an opt-in systemd
user unit and the narrow Framework `32ac:0020` active-seat udev rule. It does
not enable a service, launch a process, migrate POC settings or install an
assistant integration. This is not an Ubuntu package or a signed release.

## Build and test without installing

Use the repo's pinned Rust and Node versions, the Tauri Linux build prerequisites,
Arch `base-devel`, Python 3 and `npm ci` in `apps/desktop`. From the checkout:

```sh
bash packaging/arch/build.sh
PKGREL=2 bash packaging/arch/build.sh
bash packaging/arch/check-package.sh target/packages/fw16-ledmatrix-0.1.0-{1,2}-x86_64.pkg.tar.zst
```

The release version comes from the Cargo workspace; `PKGREL` distinguishes
package revisions. The source commit timestamp sets `SOURCE_DATE_EPOCH`. Build
from a known clean commit for distribution. The check needs `fakeroot`,
`desktop-file-utils` and `bubblewrap`; it uses a temporary pacman root/database,
skips dependency installation there, and probes the installed binary with
host libraries and hidden serial devices. This is packaging evidence, not a
fresh OS install or a real desktop-login qualification.

## CI candidates

The **Arch package candidate** GitHub Actions workflow builds changes on `main`
and can be run manually. It passes only the tracked commit to an x86-64 Arch
container, uses the repo's Rust/Node/npm pins and lockfiles, and runs `makepkg`
as an unprivileged user. Two revisions from the same source exercise the pacman
upgrade path, not compatibility across different app versions. The newer
revision is `1000 + 2 × workflow run number`. Successful runs upload only this
tested package and `SHA256SUMS`, in an artifact named with the commit SHA.
Unpack the Actions artifact and run `sha256sum -c SHA256SUMS` there before
installing. This is an unsigned candidate, not a signed pacman repository.
The Arch base and system libraries are rolling; pinned app toolchains and
version metadata do not imply byte-identical rebuilds.

[Run 45287031083](https://github.com/jschraub/fw16-ledmatrix/actions/runs/45287031083)
built and transaction-tested the unsigned `0.1.0-1004` x86-64 candidate from
`ace4452`. After download, `sha256sum -c SHA256SUMS` passed and `pacman -Qp`
reported `fw16-ledmatrix 0.1.0-1004`.

**2026-11-23, CachyOS x86-64:** built `0.1.0-1` and `0.1.0-2` from release
binaries. The scratch-root check passed install, upgrade (`pacman -Qk`: zero
missing files), desktop-entry validation, installed daemon probe with zero
accessible panels, and uninstall with the pre-existing settings fixture intact.
No package was installed on the host and no login-startup setting was changed.

**Installed desktop check (CachyOS, `0.1.0-3`):** The user clicked Enable and
then Disable under Login startup and saw the status text update after each
click. Afterward, systemd independently reported `fw16-ledmatrix.service`
disabled and inactive, with the handoff daemon still active. This verifies the
control in the installed app, not daemon startup at an actual graphical login.

## Installed usage

Install a reviewed local artifact with `sudo pacman -U /path/to/package.pkg.tar.zst`.
Administrative access is for package management only. Launch **Framework LED
Matrix** from the desktop menu. The desktop's **Start daemon** button uses the
binary alongside the desktop; closing the UI leaves rendering running.

For **future graphical logins**, open **Configuration → Login startup → Check
login startup**, then choose Enable or Disable. The preference is independent
of panel drafts and the daemon connection. The desktop rechecks the persistent
systemd unit state after changing it. An absent package, masked service or
unavailable user manager cannot be enabled from this control. CLI equivalent:
`systemctl --user enable fw16-ledmatrix.service`; `disable` reverses it.
`enable` alone does not start another daemon now. The desktop session must
activate `graphical-session.target` and import its session environment. Check
the POC service, qualification units and serial ownership before switching to
installed output; see [HARDWARE-HANDOFF.md](HARDWARE-HANDOFF.md).

Upgrade via `pacman -U` with a newer version or package revision. Stop the
installed service and close the desktop before switching running versions;
also stop an explicitly launched standalone daemon if present. For removal,
disable/stop the installed service before `sudo pacman -R fw16-ledmatrix`.
User configuration and feeds are retained: the package owns no files in home
directories. Optional cleanup is manual after backing up the user's selected
paths. Do not remove assistant settings or the POC backup.

For public distribution, sign release packages with a published, verifiable
maintainer key and document trust/upgrade policy. An unsigned local build or
checksum alone does not establish publisher identity. Clean-user login,
and release signing remain #28 qualification/delivery work.
