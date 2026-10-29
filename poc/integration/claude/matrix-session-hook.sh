#!/usr/bin/env bash
# Session liveness for the keyboard LED matrix daemon (~/code/matrix).
#
# The status line shim in statusline.sh already writes the *values* (context
# percentage, rate limits) to $XDG_RUNTIME_DIR/matrixd/sessions/<id>.json. It
# cannot report whether Claude is currently generating, though: it renders on a
# timer, not on an edge. That is what this adds.
#
#     <id>.state    "working" while a turn is in flight, "idle" otherwise
#
# Invoked as: matrix-session-hook.sh <EventName>, with the hook JSON on stdin.
#
# Exits 0 unconditionally. A non-zero hook exit is surfaced to the user inside
# Claude Code, and nothing about a decorative keyboard display justifies putting
# a warning in someone's session.

set -u

event="${1:-}"
[ -n "$event" ] || exit 0
[ -n "${XDG_RUNTIME_DIR:-}" ] || exit 0

dir="$XDG_RUNTIME_DIR/matrixd/sessions"

{
    input=$(cat)
    id=$(printf '%s' "$input" | jq -r '.session_id // empty')

    # The id becomes a path component, and SessionEnd feeds it to rm -f, so it
    # is checked rather than trusted. Claude Code sends a UUID; a value with a
    # slash in it would put the write -- and the delete -- outside this
    # directory entirely. Verified: an id of "../../escape" lands in
    # $XDG_RUNTIME_DIR without this guard.
    [[ "$id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ && "$id" != *..* ]] || exit 0

    mkdir -p "$dir" || exit 0

    case "$event" in
        UserPromptSubmit)
            printf 'working' > "$dir/$id.state"
            ;;
        Stop|StopFailure)
            printf 'idle' > "$dir/$id.state"
            ;;
        SessionStart)
            printf 'idle' > "$dir/$id.state"
            ;;
        SessionEnd)
            # Remove rather than mark ended: absence is what the daemon reads as
            # "no session", and a lingering file would hold a dead session's
            # context percentage on the panel until it went stale hours later.
            rm -f "$dir/$id.state" "$dir/$id.json"
            ;;
    esac
} >/dev/null 2>&1

exit 0
