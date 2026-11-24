#!/usr/bin/env bash
# Build a local, unsigned Arch package. Never install or enable anything.
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$repo"
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml","rb"))["workspace"]["package"]["version"])')
release=${PKGREL:-1}
[[ $version =~ ^[0-9]+(\.[0-9]+){2}$ && $release =~ ^[1-9][0-9]*$ ]] || exit 2
export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}
target=$(cargo metadata --no-deps --format-version=1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
out="$target/packages"
mkdir -p "$out"
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

cargo build --locked --release -p matrix-daemon -p matrix-claude
(cd apps/desktop && npm run tauri -- build --no-bundle -- --locked)

usr="$work/payload/usr"
for binary in matrix-daemon matrix-desktop matrix-claude; do
    install -Dm755 "$target/release/$binary" "$usr/bin/$binary"
    strip --strip-unneeded "$usr/bin/$binary"
done
install -Dm644 packaging/linux/fw16-ledmatrix.desktop "$usr/share/applications/fw16-ledmatrix.desktop"
install -Dm644 apps/desktop/src-tauri/icons/icon.png "$usr/share/icons/hicolor/512x512/apps/fw16-ledmatrix.png"
install -Dm644 apps/desktop/src-tauri/icons/source.svg "$usr/share/icons/hicolor/scalable/apps/fw16-ledmatrix.svg"
install -Dm644 packaging/linux/fw16-ledmatrix.service "$usr/lib/systemd/user/fw16-ledmatrix.service"
install -Dm644 poc/udev/60-framework-ledmatrix.rules "$usr/lib/udev/rules.d/60-framework-ledmatrix.rules"
install -Dm644 LICENSE "$usr/share/licenses/fw16-ledmatrix/LICENSE"
for guide in INTEGRATIONS MIGRATION PACKAGING; do
    install -Dm644 "docs/$guide.md" "$usr/share/doc/fw16-ledmatrix/$guide.md"
done
cd "$work"
tar --sort=name --mtime="@$SOURCE_DATE_EPOCH" --owner=0 --group=0 --numeric-owner -cf payload.tar payload
sum=$(sha256sum payload.tar | cut -d' ' -f1)
sed -e "s/@VERSION@/$version/" -e "s/@RELEASE@/$release/" -e "s/@SHA256@/$sum/" \
    "$repo/packaging/arch/PKGBUILD.in" > PKGBUILD
PKGDEST="$out" makepkg --clean --noconfirm --nosign --force
