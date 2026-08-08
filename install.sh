#!/usr/bin/env bash
#
# install.sh — set up host access to the Framework 16 LED Matrix modules.
#
# Installs a udev rule granting the active-seat user access to the modules via
# a POSIX ACL. Deliberately NOT `usermod -aG uucp`, which would hand out every
# serial device on the machine, permanently, to reach these two.
#
# Also installs the two Claude Code integration scripts into ~/.claude. They are
# inert until settings.json refers to them, and the exact JSON to add is printed
# at the end. Editing settings.json is left to you on purpose: it is your file,
# it may carry anything, and a merge that mangled it would be a poor trade for
# saving one paste.
#
# Idempotent: safe to re-run. Only the udev rule needs root; nothing else here
# does, and there are no Python dependencies.
#
# Usage:
#   ./install.sh              install
#   ./install.sh --dry-run    show what would happen
#   ./install.sh --uninstall  remove the udev rule and the Claude scripts
#   ./install.sh --no-claude  skip the Claude Code integration

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RULE_SRC="$SCRIPT_DIR/udev/60-framework-ledmatrix.rules"
RULE_DST="/etc/udev/rules.d/60-framework-ledmatrix.rules"

CLAUDE_SRC="$SCRIPT_DIR/integration/claude"
CLAUDE_DST="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
CLAUDE_SCRIPTS=(matrix-statusline-tap.sh matrix-session-hook.sh)

info()  { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m::\033[0m %s\n' "$*"; }
ok()    { printf '\033[1;32m::\033[0m %s\n' "$*"; }
error() { printf '\033[1;31m::\033[0m %s\n' "$*" >&2; exit 1; }

DRY_RUN=0
UNINSTALL=0
WITH_CLAUDE=1
for arg in "$@"; do
    case "$arg" in
        --dry-run)   DRY_RUN=1 ;;
        --uninstall) UNINSTALL=1 ;;
        --no-claude) WITH_CLAUDE=0 ;;
        -h|--help)   sed -n '2,23p' "$0"; exit 0 ;;
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

install_claude_scripts() {
    [[ -d "$CLAUDE_SRC" ]] || { warn "missing $CLAUDE_SRC — skipping Claude integration"; return; }
    if [[ ! -d "$CLAUDE_DST" ]]; then
        info "no $CLAUDE_DST — skipping Claude integration (Claude Code not installed?)"
        return
    fi
    for script in "${CLAUDE_SCRIPTS[@]}"; do
        # A symlink here would be a dotfiles-managed file; replace it rather
        # than writing through it and editing someone's repo behind their back.
        [[ -L "$CLAUDE_DST/$script" ]] && run rm -f "$CLAUDE_DST/$script"
        run install -m 0755 "$CLAUDE_SRC/$script" "$CLAUDE_DST/$script"
    done
    ok "Claude Code scripts installed -> $CLAUDE_DST"
}

print_claude_settings() {
    local tap="$CLAUDE_DST/matrix-statusline-tap.sh"
    local hook="$CLAUDE_DST/matrix-session-hook.sh"
    cat <<EOF

  The scripts are installed but do nothing until $CLAUDE_DST/settings.json
  refers to them. Add the following (merge with what is already there):

    "statusLine": {
      "type": "command",
      "command": "$tap <your existing statusline command>"
    },
    "hooks": {
EOF
    local first=1
    for event in SessionStart SessionEnd UserPromptSubmit Stop StopFailure; do
        [[ $first -eq 1 ]] || printf ',\n'
        first=0
        printf '      "%s": [ { "hooks": [ { "type": "command", "command": "[ -x %s ] && %s %s || true" } ] } ]' \
            "$event" "$hook" "$hook" "$event"
    done
    cat <<EOF

    }

  The statusLine entry wraps your existing command rather than replacing it —
  the payload is passed through untouched, so your status line looks the same.
  If you have no status line, drop the trailing argument.

  Without this, everything except the Claude panel still works.
EOF
}

if [[ $UNINSTALL -eq 1 ]]; then
    removed=0
    # Userspace first, deliberately. The udev step needs sudo, and under
    # `set -e` a declined or timed-out password would abort the whole script
    # and leave the rest of the uninstall silently undone.
    for script in "${CLAUDE_SCRIPTS[@]}"; do
        # Only remove regular files this script installed. A symlink is
        # somebody's dotfiles and is not ours to delete.
        if [[ -f "$CLAUDE_DST/$script" && ! -L "$CLAUDE_DST/$script" ]]; then
            info "removing $CLAUDE_DST/$script"
            run rm -f "$CLAUDE_DST/$script"
            removed=1
        fi
    done
    if [[ -e "$RULE_DST" ]]; then
        info "removing $RULE_DST (needs sudo)"
        run sudo rm -f "$RULE_DST"
        reload_udev
        removed=1
    fi
    if [[ $removed -eq 1 ]]; then
        ok "uninstalled"
        warn "settings.json is left alone — remove the statusLine wrapper and"
        warn "the hooks entries by hand if you added them"
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

[[ $WITH_CLAUDE -eq 1 ]] && install_claude_scripts

if [[ $DRY_RUN -eq 1 ]]; then
    [[ $WITH_CLAUDE -eq 1 && -d "$CLAUDE_DST" ]] && print_claude_settings
    exit 0
fi

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

[[ $WITH_CLAUDE -eq 1 && -d "$CLAUDE_DST" ]] && print_claude_settings

exit 0
