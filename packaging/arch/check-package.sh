#!/usr/bin/env bash
# Exercise pacman transactions against a scratch root, never the host database.
set -euo pipefail
isolated=false
if [[ ${1:-} == --isolated-container ]]; then
    # Docker builds have no panel devices and do not grant nested mount privileges.
    [[ -f /.dockerenv || -f /run/.containerenv || ${container:-} == docker ]] \
        || { echo 'Expected an isolated package build container' >&2; exit 2; }
    isolated=true
    shift
fi
if (( $# != 2 )); then
    echo "usage: bash $0 OLD.pkg.tar.zst NEW.pkg.tar.zst" >&2
    exit 2
fi
old=$(realpath "$1")
new=$(realpath "$2")
version() { pacman -Qp "$1" | awk '$1 == "fw16-ledmatrix" {print $2}'; }
oldver=$(version "$old"); newver=$(version "$new")
[[ -n $oldver && -n $newver && $(vercmp "$newver" "$oldver") == 1 ]]
root=$(mktemp -d)
trap 'rm -rf -- "$root"' EXIT
mkdir -p "$root"/{var/lib/pacman,var/cache/pacman/pkg,home/test/.config/fw16-matrix,etc}
echo 'retention fixture' > "$root/home/test/.config/fw16-matrix/config.json"
cat > "$root/etc/pacman.conf" <<'EOF'
[options]
Architecture = auto
LocalFileSigLevel = Never
EOF
pm() {
    fakeroot pacman --config "$root/etc/pacman.conf" --root "$root" \
        --dbpath "$root/var/lib/pacman" --cachedir "$root/var/cache/pacman/pkg" \
        --logfile "$root/pacman.log" --noconfirm "$@"
}
pm -Udd "$old"
test "$(pm -Q fw16-ledmatrix)" = "fw16-ledmatrix $oldver"
pm -Udd "$new"
test "$(pm -Q fw16-ledmatrix)" = "fw16-ledmatrix $newver"
pm -Qk fw16-ledmatrix
for binary in matrix-desktop matrix-daemon matrix-claude; do test -x "$root/usr/bin/$binary"; done
desktop-file-validate "$root/usr/share/applications/fw16-ledmatrix.desktop"
test -f "$root/usr/lib/systemd/user/fw16-ledmatrix.service"
test -f "$root/usr/lib/udev/rules.d/60-framework-ledmatrix.rules"
# Smoke-test the installed binary with host libraries; hide the actual panels.
probe=("$root/usr/bin/matrix-daemon" --probe)
if ! "$isolated"; then
    probe=(bwrap --ro-bind / / --dev /dev --tmpfs /sys/class/tty --chdir "$root" -- "${probe[@]}")
fi
"${probe[@]}" | python3 -c \
    'import json,sys; r=json.load(sys.stdin); assert r["os"]=="linux" and r["panel_count"]==0'
pm -R fw16-ledmatrix
test ! -e "$root/usr/bin/matrix-daemon"
test ! -e "$root/usr/lib/systemd/user/fw16-ledmatrix.service"
test "$(cat "$root/home/test/.config/fw16-matrix/config.json")" = 'retention fixture'
echo "PASS: staged install $oldver → upgrade $newver → uninstall; user settings retained"
