"""System volume, via PipeWire's PulseAudio interface.

Volume has no ambient zone — it is takeover-only, so the only thing that matters
here is *the moment it changes*. That means an event source, not a poll: polling
fast enough to catch a keypress would mean spawning `pactl` several times a
second forever, and polling slowly would show the bar after the sound already
changed.

`pactl subscribe` is a long-lived child that prints a line per event. Two things
about it drive this whole module:

**Sink events do not mean the volume changed.** Measured: playing a 0.5s beep
emits two `change on sink` events with the volume untouched. Taking a sink event
as a volume change would pop a full-panel gauge on every notification sound.
So events only ever trigger a *re-read*, and the caller is told "changed" only
when the value actually differs from the last one seen — the same
event-says-look, state-says-what discipline as `sources/udev.py`.

**The child dies whenever PipeWire restarts**, and if it stays dead volume
takeovers silently stop working forever — the failure is invisible, because
everything else keeps running. Hence `ensure_alive()` and a backoff: a dead
child must be respawned, but a PipeWire that is down must not be hammered.

Our own `pactl` calls are safe to make from inside the event handler: they emit
`client` events, never `sink` ones, so re-reading cannot feed back into another
event. That was checked rather than assumed.

One non-problem, recorded so it does not get investigated twice: the keybinds
set volume with `wpctl` while this reads it with `pactl`, and PipeWire's
Pulse compatibility layer is a well-known place for a cubic-versus-linear
scaling mismatch. Measured across 25/50/75/100%, the two agree exactly, so what
the gauge shows is what the keybind set.
"""

from __future__ import annotations

import errno
import os
import re
import subprocess
from dataclasses import dataclass

PACTL = "pactl"

# Every read spawns a process; a wedged PipeWire must not wedge the daemon with
# it. Two seconds is far beyond a healthy call (~5ms) and far below anything a
# user would call responsive.
CALL_TIMEOUT = 2.0

# Backoff for respawning the subscriber. If PipeWire is genuinely down, `pactl
# subscribe` exits immediately, so a bare retry loop would spin a core.
RESPAWN_DELAYS = (1.0, 2.0, 5.0, 10.0, 30.0)

# A child that survives this long is treated as a success, and the backoff
# resets. Without it, a child that dies instantly every time would reset the
# backoff on each spawn and retry forever at the shortest delay — which is the
# spin this backoff exists to prevent, just one second slower.
HEALTHY_AFTER = 30.0

_PERCENT = re.compile(r"(\d+)%")
_EVENT = re.compile(r"^Event '(\w+)' on (\S+)")

# Sink events cover volume and mute; server events cover the default sink moving
# (headphones in, dock connected), which changes the volume being displayed
# without any sink event firing on the sink we were watching.
_INTERESTING = {"sink", "server"}


@dataclass(frozen=True)
class Volume:
    fraction: float  # 0.0-1.0; can exceed 1.0, see read()
    muted: bool


def parse_volume(text: str) -> float | None:
    """Extract a fraction from `pactl get-sink-volume` output.

    The line carries one percentage per channel:

        Volume: front-left: 39960 /  61% / -12.89 dB,  front-right: ... 61% ...

    Takes the **maximum** across channels. A balance offset makes them differ,
    and the loudest channel is what you hear; averaging would under-report a
    hard-panned sink. Note the dB figures are negative and carry no `%`, so they
    cannot be picked up by accident.
    """
    values = [int(m) for m in _PERCENT.findall(text)]
    return max(values) / 100.0 if values else None


def parse_mute(text: str) -> bool | None:
    """Extract mute state from `pactl get-sink-mute` output (`Mute: yes|no`)."""
    lowered = text.strip().lower()
    if lowered.endswith("yes"):
        return True
    if lowered.endswith("no"):
        return False
    return None


def is_interesting(line: str) -> bool:
    """Whether a `pactl subscribe` line warrants re-reading the volume.

    Deliberately ignores `client` events: every `pactl` invocation — including
    our own re-read — creates and destroys a client, so treating those as
    interesting would make this module trigger itself indefinitely.
    """
    match = _EVENT.match(line.strip())
    return bool(match) and match.group(2) in _INTERESTING


def _run(args: list[str]) -> str | None:
    try:
        result = subprocess.run(
            [PACTL, *args],
            capture_output=True,
            text=True,
            timeout=CALL_TIMEOUT,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout if result.returncode == 0 else None


def read() -> Volume | None:
    """Current volume of the default sink, or None if it cannot be read.

    `@DEFAULT_SINK@` is resolved by pactl itself, so the default sink moving
    needs no bookkeeping here.

    The fraction is **not clamped to 1.0**. PulseAudio allows over-amplification
    and reports it honestly; the raise keybind passes `wpctl -l 1` so it will not
    happen from the keyboard, but another application can still do it, and
    silently clamping here would report a value that is not true. Clamping is the
    gauge's job, where the meaning is "how full is the bar".
    """
    volume_text = _run(["get-sink-volume", "@DEFAULT_SINK@"])
    if volume_text is None:
        return None
    fraction = parse_volume(volume_text)
    if fraction is None:
        return None
    muted = parse_mute(_run(["get-sink-mute", "@DEFAULT_SINK@"]) or "")
    return Volume(fraction=fraction, muted=bool(muted))


class Subscriber:
    """Supervised `pactl subscribe` child, as a pollable fd.

    The fd **changes across a respawn and is -1 while there is no child**, so an
    epoll loop cannot register it once and forget it. The reliable rule is to
    compare against what you registered, every iteration — `read_events()` can
    drop the fd to -1 on its own when it discovers the child has exited:

        sub.ensure_alive(now)
        if sub.fileno() != registered:
            unregister(registered); registered = sub.fileno()
            if registered >= 0:
                epoll.register(registered, EPOLLIN)
        ...
        if sub.read_events():              # True => the volume really changed
            takeover(sub.current())

    `ensure_alive()` must be called on every iteration, not only when something
    is readable: once the child is gone there is no fd left to become ready, so
    a loop that waits for readiness before checking would wait forever.
    """

    def __init__(self, command: list[str] | None = None) -> None:
        # Injectable so the supervision logic can be tested against a fake child
        # rather than against PipeWire's willingness to restart on cue.
        self.command = command or [PACTL, "subscribe"]
        self._proc: subprocess.Popen[bytes] | None = None
        self._fd = -1
        self._buffer = b""
        self._retry_at = 0.0
        self._failures = 0
        self._spawned_at = 0.0
        self._died = False
        self.last: Volume | None = None

    def fileno(self) -> int:
        """-1 when there is no live child. Never register -1 with epoll."""
        return self._fd

    @property
    def alive(self) -> bool:
        return self._proc is not None and self._proc.poll() is None

    def ensure_alive(self, now: float) -> bool:
        """Spawn the child if it is gone and the backoff has elapsed.

        Returns True if a new child was started (so `fileno()` is new).
        """
        if self.alive:
            return False
        if self._proc is not None or self._died:
            self._reap()
            self._backoff(now)
        if now < self._retry_at:
            return False
        try:
            self._proc = subprocess.Popen(
                self.command,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                # No line buffering to configure: pactl flushes each event as it
                # writes it, verified through a pipe rather than a tty.
                bufsize=0,
            )
        except OSError:
            self._proc = None
            self._died = True
            return False
        assert self._proc.stdout is not None
        self._fd = self._proc.stdout.fileno()
        os.set_blocking(self._fd, False)
        self._buffer = b""
        self._spawned_at = now
        self._died = False
        # The value at spawn time is the baseline, not a change: without this, the
        # first event after a PipeWire restart would compare against a stale
        # pre-restart value and fire a takeover nobody asked for.
        self.last = read()
        return True

    def read_events(self) -> bool:
        """Drain the child. Returns True if the volume actually changed.

        Never blocks, never raises. A dead child is detected here (read returns
        EOF) and closed immediately: leaving a closed-write-end pipe registered
        with epoll would report it readable forever and spin the loop.
        """
        if self._fd < 0:
            return False
        saw_event = False
        while True:
            try:
                chunk = os.read(self._fd, 65536)
            except OSError as exc:
                if exc.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                    break
                chunk = b""
            if not chunk:  # EOF — the child exited
                self._died = True
                self._reap()
                break
            self._buffer += chunk
            lines = self._buffer.split(b"\n")
            self._buffer = lines.pop()
            for raw in lines:
                if is_interesting(raw.decode("utf-8", "replace")):
                    saw_event = True

        if not saw_event:
            return False
        current = read()
        if current is None or current == self.last:
            return False
        self.last = current
        return True

    def current(self) -> Volume | None:
        """Last known volume. Does not spawn anything."""
        return self.last

    def _backoff(self, now: float) -> None:
        """Schedule the next respawn attempt, lengthening it on repeat failure."""
        if self._spawned_at and now - self._spawned_at >= HEALTHY_AFTER:
            self._failures = 0  # it worked for a while; this is a fresh problem
        delay = RESPAWN_DELAYS[min(self._failures, len(RESPAWN_DELAYS) - 1)]
        self._failures += 1
        self._retry_at = now + delay
        self._died = False

    def _reap(self) -> None:
        """Close the pipe and clear the fd. Idempotent."""
        if self._proc is not None:
            if self._proc.stdout is not None:
                try:
                    self._proc.stdout.close()
                except OSError:
                    pass
            if self._proc.poll() is None:
                self._proc.terminate()
            try:
                self._proc.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                self._proc.kill()
                self._proc.wait(timeout=1.0)
            self._proc = None
        self._fd = -1
        self._buffer = b""

    def close(self) -> None:
        self._reap()

    def __enter__(self) -> Subscriber:
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()
