#!/usr/bin/env bash
#
# install.sh — set up host access to the Framework 16 LED Matrix modules.
#
# Installs a udev rule granting the active-seat user access to the modules via
# a POSIX ACL. Deliberately NOT `usermod -aG uucp`, which would hand out every
# serial device on the machine, permanently, to reach these two.
#
# Idempotent: safe to re-run. Only the udev rule needs root; nothing else here
# does, and there are no Python dependencies.
#
# Usage:
#   ./install.sh              install
#   ./install.sh --dry-run    show what would happen
#   ./install.sh --uninstall  remove the udev rule

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RULE_SRC="$SCRIPT_DIR/udev/60-framework-ledmatrix.rules"
RULE_DST="/etc/udev/rules.d/60-framework-ledmatrix.rules"

info()  { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m::\033[0m %s\n' "$*"; }
ok()    { printf '\033[1;32m::\033[0m %s\n' "$*"; }
error() { printf '\033[1;31m::\033[0m %s\n' "$*" >&2; exit 1; }

DRY_RUN=0
UNINSTALL=0
for arg in "$@"; do
    case "$arg" in
        --dry-run)   DRY_RUN=1 ;;
        --uninstall) UNINSTALL=1 ;;
        -h|--help)   sed -n '2,16p' "$0"; exit 0 ;;
        *)           error "unknown argument: $arg" ;;
    esac
done

run() {
    if [[ $DRY_RUN -eq 1 ]]; then
        info "[dry-run] $*"
    else
        "$@"
    fi
}

reload_udev() {
    run sudo udevadm control --reload-rules
    # Re-run rules against existing devices so the ACL applies without a replug.
    # The modules are internal, so replugging is not a reasonable ask.
    run sudo udevadm trigger --subsystem-match=tty
}

if [[ $UNINSTALL -eq 1 ]]; then
    if [[ -e "$RULE_DST" ]]; then
        info "removing $RULE_DST"
        run sudo rm -f "$RULE_DST"
        reload_udev
        ok "uninstalled"
    else
        info "not installed; nothing to do"
    fi
    exit 0
fi

[[ -f "$RULE_SRC" ]] || error "missing $RULE_SRC — run from a complete checkout"

if [[ -e "$RULE_DST" ]] && cmp -s "$RULE_SRC" "$RULE_DST"; then
    ok "udev rule already current"
else
    info "installing udev rule -> $RULE_DST (needs sudo)"
    run sudo install -m 0644 "$RULE_SRC" "$RULE_DST"
    reload_udev
    ok "udev rule installed"
fi

[[ $DRY_RUN -eq 1 ]] && exit 0

# Verify rather than assume. The ACL is applied by logind for the active seat,
# so it can legitimately be absent if this runs before the session goes active
# — report that honestly instead of claiming success.
found=0
denied=0
for dev in /dev/serial/by-path/*-usb-*; do
    [[ -e "$dev" ]] || continue
    tty=$(basename "$(readlink -f "$dev")")
    sysdev="/sys/class/tty/$tty/device/.."
    [[ -r "$sysdev/idVendor" ]] || continue
    read -r vid < "$sysdev/idVendor"
    read -r pid < "$sysdev/idProduct"
    [[ "$vid$pid" == "32ac0020" ]] || continue
    found=$((found + 1))
    if [[ -r "$dev" && -w "$dev" ]]; then
        ok "access OK: $tty"
    else
        warn "no access yet: $tty"
        denied=$((denied + 1))
    fi
done

if [[ $found -eq 0 ]]; then
    warn "no LED matrix modules found (looked for 32ac:0020)"
    warn "the rule is installed and will apply when one is plugged in"
elif [[ $denied -gt 0 ]]; then
    warn "rule installed but the ACL is not applied yet — this is normal if the"
    warn "seat session is not active. Log out and back in, then re-check with:"
    warn "  tools/smoke.py probe"
else
    ok "all $found module(s) accessible — try: tools/smoke.py probe"
fi
