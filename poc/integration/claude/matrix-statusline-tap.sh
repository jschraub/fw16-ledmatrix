#!/usr/bin/env bash
# Snapshot the Claude Code status line payload for the LED matrix daemon.
#
#   Usage: matrix-statusline-tap.sh [your-real-statusline-command [args...]]
#
# Claude Code allows exactly one statusLine command, and everyone's is
# different, so this does not replace yours — it wraps it. The payload is
# written to a snapshot file and then handed to your command unchanged on
# stdin, so your status line renders exactly as it did before.
#
#   "statusLine": {
#     "type": "command",
#     "command": "~/.claude/matrix-statusline-tap.sh ~/.claude/statusline.sh"
#   }
#
# With no downstream command it prints nothing, which is a valid status line if
# you only want the panel.
#
# Why here at all: the context-window percentage is piped to the status line and
# exposed nowhere else — no file on disk holds it, and no API reports it,
# because it is a property of a live conversation rather than of an account.
# This payload is already produced several times a second, so tapping it is free.
#
# The snapshot is one file per session id, since several Claude Code sessions
# can be open at once and a single shared file would only ever show whichever
# rendered last. See matrix-session-hook.sh for the liveness half.

set -u

input=$(cat)

{
    if [ -n "${XDG_RUNTIME_DIR:-}" ] && command -v jq >/dev/null 2>&1; then
        dir="$XDG_RUNTIME_DIR/matrixd/sessions"
        id=$(printf '%s' "$input" | jq -r '.session_id // empty')

        # The id becomes a path component, and the hook script feeds the same
        # value to rm -f, so it is checked rather than trusted. Claude Code
        # sends a UUID; verified that an id of "../../escape" lands outside
        # this directory without the guard.
        if [[ "$id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ && "$id" != *..* ]] &&
            mkdir -p "$dir"; then
            # Temp name then rename, so a reader polling this directory can
            # never catch a half-written file.
            printf '%s' "$input" > "$dir/.$id.tmp" &&
                mv -f "$dir/.$id.tmp" "$dir/$id.json"
        fi
    fi
} >/dev/null 2>&1

# Hand the payload on untouched. Everything above is best-effort and silenced:
# a status line must never break, or even flicker, because a side channel had a
# bad day.
if [ "$#" -gt 0 ]; then
    printf '%s' "$input" | "$@"
fi

exit 0
