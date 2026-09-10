"""OpenAI subscription quotas, authenticated through OpenCode (read-only).

The internal endpoint is the one used by Codex. This reads OpenCode's OAuth
login, not an API key or Codex's auth.json, and never refreshes credentials.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from datetime import datetime, timezone

from .usage import REQUEST_TIMEOUT, Usage, Window

USAGE_URL = "https://chatgpt.com/backend-api/wham/usage"
STALE_AFTER = 300.0


@dataclass(frozen=True)
class Credentials:
    access: str = field(repr=False)
    account_id: str | None = None

    @property
    def identity(self) -> str:
        # Without an account ID, conservatively invalidate on token rotation.
        return self.account_id or hashlib.sha256(self.access.encode()).hexdigest()


def _header(value: object) -> bool:
    return isinstance(value, str) and bool(value) and all(32 <= ord(c) < 127 for c in value)


def read_credentials() -> Credentials | None:
    try:
        override = os.environ.get("OPENCODE_AUTH_CONTENT")
        if override is not None:
            payload = json.loads(override)
        else:
            data_home = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
            with open(os.path.join(data_home, "opencode", "auth.json")) as f:
                payload = json.load(f)
        auth = payload.get("openai") if isinstance(payload, dict) else None
        if not isinstance(auth, dict) or auth.get("type") != "oauth":
            return None
        access, account = auth.get("access"), auth.get("accountId")
        if not _header(access) or (account is not None and not _header(account)):
            return None
        return Credentials(access, account)
    except (OSError, ValueError):
        return None


def parse(payload: object, fetched_at: float) -> Usage | None:
    """Match windows by duration, never by primary/secondary position.

Additional model-specific buckets and credits are not these two allowances.
An absent, unrecognized, or malformed window stays blank.
"""
    if not isinstance(payload, dict) or not isinstance(payload.get("rate_limit"), dict):
        return None
    windows = {}
    for name in ("primary_window", "secondary_window"):
        raw = payload["rate_limit"].get(name)
        if not isinstance(raw, dict):
            continue
        duration, percent = raw.get("limit_window_seconds"), raw.get("used_percent")
        if not isinstance(duration, (int, float)) or duration not in (18000, 604800):
            continue
        if isinstance(percent, bool) or not isinstance(percent, (int, float)):
            continue
        if not 0 <= percent <= 100 or not math.isfinite(percent):
            continue
        reset = raw.get("reset_at")
        resets_at = None
        if isinstance(reset, (int, float)) and not isinstance(reset, bool):
            try:
                resets_at = datetime.fromtimestamp(reset, timezone.utc)
            except (ValueError, OverflowError, OSError):
                pass
        windows[duration] = Window(float(percent), resets_at)
    if not windows:
        return None
    return Usage(windows.get(18000), windows.get(604800), fetched_at, STALE_AFTER)


def fetch(fetched_at: float, credentials: Credentials) -> Usage | None:
    headers = {"Authorization": f"Bearer {credentials.access}", "User-Agent": "matrixd"}
    if credentials.account_id:
        headers["ChatGPT-Account-Id"] = credentials.account_id
    request = urllib.request.Request(USAGE_URL, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=REQUEST_TIMEOUT) as response:
            return parse(json.load(response), fetched_at)
    except urllib.error.HTTPError as error:
        error.close()
        return None
    except (urllib.error.URLError, OSError, ValueError):
        return None


class Source:
    """A small account-bound cache, accessed only by the daemon's quota worker."""

    def __init__(self) -> None:
        self._identity: str | None = None
        self._cached: Usage | None = None

    def fetch(self, fetched_at: float) -> Usage | None:
        credentials = read_credentials()
        identity = credentials.identity if credentials else None
        if identity != self._identity:
            self._identity, self._cached = identity, None
        if credentials is None:
            return None
        result = fetch(fetched_at, credentials)
        # Auth can change while the request is in flight. Do not publish an old
        # account's result after the user has switched accounts or logged out.
        latest = read_credentials()
        if latest is None or latest.identity != identity:
            self._identity, self._cached = None, None
            return None
        if result is not None:
            self._cached = result
        if self._cached is not None and self._cached.is_stale(fetched_at):
            self._cached = None
        return self._cached
