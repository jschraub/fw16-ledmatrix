#!/usr/bin/env bash
#
# install.sh — set up host access to the Framework 16 LED Matrix modules.
#
# Installs a udev rule granting the active-seat user access to the modules via
# a POSIX ACL. Deliberately NOT `usermod -aG uucp`, which would hand out every
# serial device on the machine, permanently, to reach these two.
#
# Also installs a systemd *user* service that runs the daemon from this
# checkout, and the two Claude Code integration scripts into ~/.claude. The
# scripts are inert until settings.json refers to them, and the exact JSON to
# add is printed at the end. Editing settings.json is left to you on purpose:
# it is your file, it may carry anything, and a merge that mangled it would be
# a poor trade for saving one paste.
#
# Idempotent: safe to re-run. Only the udev rule needs root; nothing else here
# does, and there are no Python dependencies.
#
# Usage:
#   ./install.sh               install
#   ./install.sh --dry-run     show what would happen
#   ./install.sh --uninstall   remove everything this installed
#   ./install.sh --no-claude   skip the Claude Code integration
#   ./install.sh --no-service  skip the systemd user service

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RULE_SRC="$SCRIPT_DIR/udev/60-framework-ledmatrix.rules"
RULE_DST="/etc/udev/rules.d/60-framework-ledmatrix.rules"

CLAUDE_SRC="$SCRIPT_DIR/integration/claude"
CLAUDE_DST="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
CLAUDE_SCRIPTS=(matrix-statusline-tap.sh matrix-session-hook.sh)

UNIT_NAME="matrixd.service"
UNIT_SRC="$SCRIPT_DIR/systemd/$UNIT_NAME"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT_DST="$UNIT_DIR/$UNIT_NAME"

info()  { printf '\033[1;34m::\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m::\033[0m %s\n' "$*"; }
ok()    { printf '\033[1;32m::\033[0m %s\n' "$*"; }
error() { printf '\033[1;31m::\033[0m %s\n' "$*" >&2; exit 1; }

DRY_RUN=0
UNINSTALL=0
WITH_CLAUDE=1
WITH_SERVICE=1
for arg in "$@"; do
    case "$arg" in
        --dry-run)    DRY_RUN=1 ;;
        --uninstall)  UNINSTALL=1 ;;
        --no-claude)  WITH_CLAUDE=0 ;;
        --no-service) WITH_SERVICE=0 ;;
        # Print the header block rather than a hardcoded line range, which
        # goes stale the first time anyone edits the comment above.
        -h|--help)    awk 'NR>1 && /^#/ { sub(/^# ?/, ""); print; next } NR>1 { exit }' "$0"; exit 0 ;;
        *)            error "unknown argument: $arg" ;;
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

# Whether there is a user manager to talk to at all. `systemctl --user` fails
# with a confusing error over a bare ssh session or in a container, and under
# `set -e` that would abort the whole install over an optional extra.
user_systemd_ok() {
    command -v systemctl >/dev/null 2>&1 &&
        systemctl --user show-environment >/dev/null 2>&1
}

install_service() {
    [[ -f "$UNIT_SRC" ]] || { warn "missing $UNIT_SRC — skipping the service"; return; }

    # The unit runs the daemon out of this checkout rather than copying code
    # anywhere, so `git pull` is the whole update procedure.
    local rendered
    rendered=$(sed "s|@INSTALL_DIR@|$SCRIPT_DIR|g" "$UNIT_SRC")

    local changed=1
    if [[ -f "$UNIT_DST" ]] && [[ "$rendered" == "$(cat "$UNIT_DST")" ]]; then
        changed=0
        ok "service unit already current"
    else
        info "installing $UNIT_DST"
        if [[ $DRY_RUN -eq 1 ]]; then
            info "[dry-run] write $UNIT_DST with WorkingDirectory=$SCRIPT_DIR"
        else
            mkdir -p "$UNIT_DIR"
            printf '%s\n' "$rendered" > "$UNIT_DST"
        fi
    fi

    if ! user_systemd_ok; then
        warn "no user systemd manager reachable — unit written but not enabled"
        warn "enable it from a login session with:"
        warn "  systemctl --user enable --now $UNIT_NAME"
        return
    fi

    [[ $changed -eq 1 ]] && run systemctl --user daemon-reload
    run systemctl --user enable "$UNIT_NAME"
    # `enable --now` starts a stopped service but will not restart a running
    # one, so a re-run after an edit would leave the old code live.
    if [[ $changed -eq 1 ]] && systemctl --user is-active --quiet "$UNIT_NAME"; then
        run systemctl --user restart "$UNIT_NAME"
    else
        run systemctl --user start "$UNIT_NAME"
    fi
    ok "service enabled and started — logs: journalctl --user -u $UNIT_NAME -f"
}

uninstall_service() {
    [[ -f "$UNIT_DST" ]] || return 0
    info "removing $UNIT_DST"
    if user_systemd_ok; then
        run systemctl --user disable --now "$UNIT_NAME" || true
    fi
    run rm -f "$UNIT_DST"
    user_systemd_ok && run systemctl --user daemon-reload
    removed=1
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
    #
    # The service goes first of all, so the daemon is stopped before the files
    # it reads start disappearing underneath it.
    uninstall_service
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
    [[ $WITH_SERVICE -eq 1 ]] && install_service
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

# Last, so the daemon starts with the rule installed and the ACL applied
# rather than spending its first seconds retrying an EACCES it need not hit.
[[ $WITH_SERVICE -eq 1 ]] && install_service

[[ $WITH_CLAUDE -eq 1 && -d "$CLAUDE_DST" ]] && print_claude_settings

exit 0
