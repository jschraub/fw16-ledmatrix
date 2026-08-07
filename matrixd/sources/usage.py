"""Claude Code subscription rate limits — the 5-hour and weekly windows.

There is no supported API for this. The Admin and Analytics APIs report
organisation token spend and need an admin key; they cannot see a personal
Pro/Max subscription's rolling windows at all. Claude Code itself does not
persist the numbers anywhere on disk.

What does work is the endpoint claude.ai and Claude Code use internally:

    GET https://api.anthropic.com/api/oauth/usage
    Authorization: Bearer <token from ~/.claude/.credentials.json>
    anthropic-beta: oauth-2025-04-20

It is **undocumented**, so every failure mode here resolves to "return None and
let the caller show nothing", never to an exception. If the shape changes one
day the panel should quietly drop the Claude zone, not crash the daemon.

Credentials are read **read-only and never refreshed**. Refreshing would rotate
the refresh token and rewrite a file Claude Code owns; a concurrent refresh
would log you out of Claude Code. The access token lasts ~12h and Claude Code
renews it in normal use, so re-reading the file each poll is enough. On 401 the
data simply goes stale.
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone

USAGE_URL = "https://api.anthropic.com/api/oauth/usage"
CREDENTIALS_PATH = os.path.expanduser("~/.claude/.credentials.json")

POLL_INTERVAL = 60.0  # utilization moves in whole percent; faster buys nothing
REQUEST_TIMEOUT = 10.0

# Beyond this, treat the numbers as too old to show rather than misleading.
STALE_AFTER = 12 * 3600.0


@dataclass(frozen=True)
class Window:
    """One rate-limit window."""

    percent: float
    resets_at: datetime | None
    severity: str = "normal"


@dataclass(frozen=True)
class Usage:
    five_hour: Window | None
    seven_day: Window | None
    fetched_at: float  # time.monotonic() when this was read

    def is_stale(self, now: float) -> bool:
        return now - self.fetched_at > STALE_AFTER


def read_token(path: str = CREDENTIALS_PATH) -> str | None:
    """Read the OAuth access token. Never writes, never refreshes."""
    try:
        with open(path) as f:
            return json.load(f)["claudeAiOauth"]["accessToken"]
    except (OSError, KeyError, ValueError):
        return None


def _parse_time(value: object) -> datetime | None:
    if not isinstance(value, str):
        return None
    try:
        return datetime.fromisoformat(value).astimezone(timezone.utc)
    except ValueError:
        return None


def parse(payload: object, fetched_at: float) -> Usage | None:
    """Turn a decoded response into a Usage. Pure — no I/O, so it is testable.

    Severity comes from the `limits[]` array rather than the top-level
    five_hour/seven_day objects, which do not carry it. Entries are matched on
    `kind`: "session" is the 5-hour window and "weekly_all" the 7-day one. The
    names do not match the top-level keys, which is a trap worth naming.

    Anything unrecognised degrades to None rather than raising: this endpoint is
    undocumented and may change shape without warning.
    """
    if not isinstance(payload, dict):
        return None

    severities: dict[str, str] = {}
    limits = payload.get("limits")
    if isinstance(limits, list):
        for entry in limits:
            if isinstance(entry, dict) and isinstance(entry.get("kind"), str):
                sev = entry.get("severity")
                severities[entry["kind"]] = sev if isinstance(sev, str) else "normal"

    def window(key: str, limit_kind: str) -> Window | None:
        raw = payload.get(key)
        if not isinstance(raw, dict):
            return None
        pct = raw.get("utilization")
        if not isinstance(pct, (int, float)):
            return None
        return Window(
            percent=float(pct),
            resets_at=_parse_time(raw.get("resets_at")),
            severity=severities.get(limit_kind, "normal"),
        )

    five = window("five_hour", "session")
    seven = window("seven_day", "weekly_all")
    if five is None and seven is None:
        return None
    return Usage(five_hour=five, seven_day=seven, fetched_at=fetched_at)


def fetch(fetched_at: float, token: str | None = None) -> Usage | None:
    """Poll the endpoint. Returns None on any failure, including auth.

    `fetched_at` is passed in rather than read here so the caller controls the
    clock — which keeps staleness logic testable.
    """
    token = token or read_token()
    if not token:
        return None

    req = urllib.request.Request(
        USAGE_URL,
        headers={
            "Authorization": f"Bearer {token}",
            "anthropic-beta": "oauth-2025-04-20",
            "anthropic-version": "2023-06-01",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=REQUEST_TIMEOUT) as resp:
            payload = json.load(resp)
    except (urllib.error.URLError, OSError, ValueError):
        # Includes 401 after a token expires with no Claude Code session to
        # renew it. Stale, not fatal.
        return None
    return parse(payload, fetched_at)
