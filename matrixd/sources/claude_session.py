"""Claude Code session state — context percentage and whether one is running.

Context percentage exists in exactly one place: the JSON Claude Code pipes to
its status line. No file on disk carries it, and no API reports it — it is a
property of a live conversation, not of an account. So the status line is the
tap, and it is a good one: that payload is already produced several times a
second, so reading it costs nothing and is never stale while you are working.

**This module only reads.** The producing half lives in the dotfiles repo, next
to the status line and hook settings it belongs to, and the two halves meet at a
directory layout rather than at code:

    $XDG_RUNTIME_DIR/matrixd/sessions/
        <session-id>.json     the status line payload, rewritten continuously
        <session-id>.state    "working" or "idle", written by hooks

Splitting it that way means neither repo has to be installed for the other to
work. Without dotfiles the directory is empty and the Claude zone shows nothing;
without this daemon the files are simply never read.

Why two files rather than one: the payload gives *values*, but it cannot say
whether Claude is currently generating — the status line renders on a timer, not
on an edge. Liveness comes from hooks instead (`UserPromptSubmit` starts work,
`Stop`/`StopFailure` ends it, `SessionEnd` removes both files).

Being under `XDG_RUNTIME_DIR` is deliberate: it is tmpfs and is cleared when the
last user session ends, so a session killed hard enough to skip `SessionEnd`
cannot leave a frozen percentage on the panel past a reboot. `STALE_AFTER` is
the belt to that pair of braces.
"""

from __future__ import annotations

import glob
import json
import os
import time
from dataclasses import dataclass

RUNTIME_SUBDIR = "matrixd/sessions"

# How long after its last update a snapshot stops counting as a live session.
# Generous on purpose: a session you left open over lunch is still a session,
# and its context percentage is still true. This exists to clear sessions that
# died without firing SessionEnd, not to detect idleness.
STALE_AFTER = 4 * 3600.0


@dataclass(frozen=True)
class Session:
    session_id: str
    context_pct: float | None
    working: bool
    updated_at: float  # wall-clock mtime of the snapshot

    # Present in the same payload, and the panel wants them. Treated as a
    # fallback, never the source: they exist only while a session is running,
    # whereas sources/usage.py reports them whether or not Claude Code is open.
    five_hour_pct: float | None = None
    seven_day_pct: float | None = None


def snapshot_dir() -> str | None:
    """The directory the producing half writes to, or None if there is none."""
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    return os.path.join(runtime, RUNTIME_SUBDIR) if runtime else None


def _percent(value: object) -> float | None:
    """Coerce a percentage, rejecting nonsense rather than propagating it."""
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    if value != value or value in (float("inf"), float("-inf")):  # NaN / inf
        return None
    return max(0.0, min(100.0, float(value)))


def parse(payload: object) -> tuple[float | None, float | None, float | None]:
    """Pull (context, five_hour, seven_day) percentages out of a status payload.

    Pure, and tolerant of every shape but the one it wants: this is an
    undocumented internal format that changes with Claude Code releases, and the
    right response to an unexpected shape is a blank zone, never a crash in a
    daemon that is also driving the battery indicator.
    """
    if not isinstance(payload, dict):
        return None, None, None

    context = payload.get("context_window")
    context_pct = _percent(context.get("used_percentage")) if isinstance(context, dict) else None

    limits = payload.get("rate_limits")
    limits = limits if isinstance(limits, dict) else {}

    def window(name: str) -> float | None:
        value = limits.get(name)
        return _percent(value.get("used_percentage")) if isinstance(value, dict) else None

    return context_pct, window("five_hour"), window("seven_day")


def _read_state(path: str) -> bool:
    """Whether the hooks say this session is currently generating."""
    try:
        with open(path) as f:
            return f.read().strip() == "working"
    except OSError:
        # No state file means hooks are not installed, or none has fired yet.
        # Reporting "not working" is the safe default: a stuck-on working
        # indicator is worse than a missing one, since it never resolves.
        return False


def read(now: float | None = None) -> Session | None:
    """The most recently active session, or None if nothing is running.

    Picks by modification time rather than by start time. With several sessions
    open, the one whose status line rendered last is the one being looked at —
    which is the one whose context percentage is worth a panel.
    """
    directory = snapshot_dir()
    if not directory:
        return None
    now = time.time() if now is None else now

    newest_path, newest_mtime = None, 0.0
    for path in glob.glob(os.path.join(directory, "*.json")):
        try:
            mtime = os.stat(path).st_mtime
        except OSError:
            continue  # vanished between the glob and the stat; SessionEnd won
        if mtime > newest_mtime:
            newest_path, newest_mtime = path, mtime

    if newest_path is None or now - newest_mtime > STALE_AFTER:
        return None

    try:
        with open(newest_path) as f:
            payload = json.load(f)
    except (OSError, ValueError):
        return None

    context_pct, five_hour, seven_day = parse(payload)
    session_id = os.path.basename(newest_path)[: -len(".json")]
    return Session(
        session_id=session_id,
        context_pct=context_pct,
        working=_read_state(os.path.join(directory, f"{session_id}.state")),
        updated_at=newest_mtime,
        five_hour_pct=five_hour,
        seven_day_pct=seven_day,
    )


def prune(now: float | None = None) -> int:
    """Delete snapshots past STALE_AFTER. Returns how many were removed.

    Only matters for sessions that died without firing `SessionEnd`. Cheap to
    call on a slow timer; not required for correctness, since read() already
    ignores stale files.
    """
    directory = snapshot_dir()
    if not directory:
        return 0
    now = time.time() if now is None else now
    removed = 0
    for path in glob.glob(os.path.join(directory, "*.json")):
        try:
            if now - os.stat(path).st_mtime <= STALE_AFTER:
                continue
            os.unlink(path)
            removed += 1
            state = path[: -len(".json")] + ".state"
            if os.path.exists(state):
                os.unlink(state)
        except OSError:
            continue
    return removed
