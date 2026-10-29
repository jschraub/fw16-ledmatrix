"""Read metadata-only snapshots from integration/opencode/matrix-session.js.

File mtime is producer health; updated_at is meaningful conversation activity.
Keeping these separate prevents heartbeats from changing the selected session.
"""

from __future__ import annotations

import glob
import json
import math
import os
import time

from .claude_session import Session, _percent

STALE_AFTER = 60.0
RUNTIME_SUBDIR = "matrixd/opencode"


def snapshot_dir() -> str | None:
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    return os.path.join(runtime, RUNTIME_SUBDIR) if runtime else None


def read(now: float | None = None) -> Session | None:
    directory = snapshot_dir()
    if not directory:
        return None
    now = time.time() if now is None else now
    newest = None
    for path in glob.glob(os.path.join(directory, "*.json")):
        try:
            with open(path) as f:
                if not 0 <= now - os.fstat(f.fileno()).st_mtime <= STALE_AFTER:
                    continue
                payload = json.load(f)
        except (OSError, ValueError):
            continue
        if not isinstance(payload, dict) or payload.get("version") != 1:
            continue
        sessions = payload.get("sessions")
        if not isinstance(sessions, list):
            continue
        for raw in sessions:
            if not isinstance(raw, dict) or raw.get("provider_id") != "openai":
                continue
            sid, updated = raw.get("session_id"), raw.get("updated_at")
            if not isinstance(sid, str) or not sid or raw.get("parent_id"):
                continue
            if isinstance(updated, bool) or not isinstance(updated, (int, float)):
                continue
            if not 0 < updated <= now or not math.isfinite(updated):
                continue
            session = Session(sid, _percent(raw.get("context_pct")),
                              raw.get("working") is True, float(updated))
            if newest is None or (session.updated_at, sid) > (newest.updated_at, newest.session_id):
                newest = session
    return newest


def prune(now: float | None = None) -> int:
    directory = snapshot_dir()
    if not directory:
        return 0
    now = time.time() if now is None else now
    removed = 0
    for path in glob.glob(os.path.join(directory, "*.json")):
        try:
            if now - os.stat(path).st_mtime > STALE_AFTER:
                os.unlink(path)
                removed += 1
        except OSError:
            pass
    return removed
