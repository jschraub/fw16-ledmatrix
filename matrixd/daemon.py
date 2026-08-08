"""The event loop: one epoll, every input, both panels.

Single-threaded and single-process on purpose. Every input here is already a
file descriptor or a cheap sysfs read, the total event rate is a handful per
second, and the one genuinely slow operation — a 169ms greyscale frame — is a
*write* that does not block. There is nothing for a second thread to do except
introduce a way for two of them to talk to the same panel at once.

Structure:

    inputs  ->  state  ->  frames  ->  panels

State is re-read from authoritative sources rather than accumulated from event
payloads, so a missed or malformed event costs one late update and never a
wrong one. Frames are recomputed from state and compared against what is
actually on each panel, so a redraw only happens when something changed — which
matters because an ambient frame costs 169ms of link time and the clock only
moves once a minute.

**Timing is deadline-driven, not tick-driven.** The loop computes when the next
thing is due, sleeps in `epoll` exactly that long, and wakes early if an fd
becomes ready. Idle cost is one wakeup a second; the 10Hz charging pulse is a
deadline like any other rather than a special case.

What the loop does *not* do is retry forever in tight loops. Every failure path
here — a panel that will not open, a `pactl` child that will not stay up, a
usage endpoint returning 500 — is on a backoff or a fixed interval, because the
failure mode of a daemon is not crashing, it is spinning quietly at 100% of a
core while appearing to work.
"""

from __future__ import annotations

import errno
import logging
import math
import os
import select
import signal
import threading
import time
from dataclasses import dataclass, field

from . import render, transport
from .sources import audio, claude_session, power, screen, udev, usage

log = logging.getLogger("matrixd")

# ── cadences ─────────────────────────────────────────────────────────────────

TICK = 1.0  # ambient re-evaluation; the clock only needs a minute, but
# context % and session liveness want to look responsive
USAGE_INTERVAL = usage.POLL_INTERVAL
PANEL_RETRY = 5.0  # a panel that will not open; udev is the fast path
PRUNE_INTERVAL = 600.0  # sweep dead Claude session snapshots

TAKEOVER_SECONDS = 2.0

# Charging pulse. 10Hz is smooth to the eye and costs 14ms of link time per
# step — about 14% duty, leaving the link overwhelmingly idle for takeovers.
PULSE_PERIOD = 3.0
PULSE_STEP = 0.1
PULSE_FRACTION = 0.35  # amplitude as a share of base brightness
PULSE_MIN_AMPLITUDE = 3  # so a base of 1 still visibly breathes: 1 -> 4 -> 1


def pulse_brightness(base: int, phase: float) -> int:
    """Global brightness for a charging panel at `phase` (0..1) of the breathe.

    Amplitude is proportional to the base level so the pulse stays perceptually
    similar across the range, but floored so that it survives the bottom of it:
    at base 1 there is no room to modulate downward without switching the panel
    off, so the breathe runs 1 -> 4 -> 1 and is biased upward by clamping.

    Pure, so the shape can be asserted rather than watched.
    """
    base = max(0, min(255, int(base)))
    amplitude = max(PULSE_MIN_AMPLITUDE, int(round(base * PULSE_FRACTION)))
    low = max(1, base - amplitude)
    high = min(255, base + amplitude)
    # Raised cosine: starts at `low`, peaks at mid-phase, returns. Smooth at the
    # wrap point, which a triangle or sawtooth would not be.
    t = (1.0 - math.cos(2.0 * math.pi * phase)) / 2.0
    return int(round(low + (high - low) * t))


@dataclass
class Takeover:
    """What a panel is showing instead of its ambient frame, and until when.

    One slot per panel, not a queue. A takeover is feedback about *current
    state*, so six volume taps are one thing whose value changed six times —
    queueing them would spend twelve seconds showing values that stopped being
    true. Each new event overwrites the slot and pushes the expiry out, so
    holding a key keeps the bar up and it lingers TAKEOVER_SECONDS after release.
    """

    frame: render.Frame
    expires_at: float


@dataclass
class State:
    """Everything the frames are computed from."""

    power: power.Power = field(default_factory=lambda: power.Power(None, False, False))
    usage: usage.Usage | None = None
    session: claude_session.Session | None = None
    screen_fraction: float | None = None
    screen_off: bool = False


class Daemon:
    def __init__(self) -> None:
        self.state = State()
        self.panels: dict[str, transport.Panel] = {}
        self.takeovers: dict[str, Takeover] = {}
        self.painted: dict[str, render.Frame] = {}
        self.brightness: dict[str, int] = {}

        self.watcher: udev.Watcher | None = None
        self.volume = audio.Subscriber()

        self._running = True
        self._pulse_phase = 0.0
        self._asleep = False
        self._usage_thread: threading.Thread | None = None
        self._wakeup_r = -1
        self._wakeup_w = -1

        now = time.monotonic()
        # Absolute monotonic times, so they must be seeded from the clock — a
        # literal 0.0 is not "immediately", it is a point hours in the past that
        # every comparison treats as permanently overdue.
        self._panel_retry_at = now
        self._due = {
            "tick": now,
            "usage": now,
            "pulse": now,
            "prune": now + PRUNE_INTERVAL,
        }

    # ── lifecycle ────────────────────────────────────────────────────────────

    def _install_signal_handlers(self) -> None:
        """Make a signal actually interrupt the poll.

        Since PEP 475, `epoll.poll()` retries on EINTR instead of raising, so a
        handler that only sets a flag would not be noticed until the current
        sleep expired — up to a minute of ignoring SIGTERM, at which point
        systemd escalates to SIGKILL and the panels are left lit.

        A self-pipe fixes it: the C-level handler writes a byte, which makes the
        poll return, and the Python handler then runs and sets the flag.
        """
        self._wakeup_r, self._wakeup_w = os.pipe()
        for fd in (self._wakeup_r, self._wakeup_w):
            os.set_blocking(fd, False)
        signal.set_wakeup_fd(self._wakeup_w)
        for sig in (signal.SIGINT, signal.SIGTERM):
            signal.signal(sig, self._on_signal)

    def run(self) -> int:
        self._install_signal_handlers()

        self.watcher = udev.Watcher()
        self.refresh_all()
        self.reconcile_panels()

        epoll = select.epoll()
        registered: dict[int, str] = {}
        try:
            while self._running:
                self._sync_registrations(epoll, registered)
                timeout = max(0.0, self._next_deadline() - time.monotonic())
                try:
                    ready = epoll.poll(timeout)
                except OSError as exc:
                    # A signal arriving mid-poll is normal, not an error.
                    if exc.errno != errno.EINTR:
                        raise
                    ready = []

                for fd, _events in ready:
                    self._handle_ready(registered.get(fd))
                if not self._running:
                    # Do not paint a frame we are about to sleep. Beyond being
                    # pointless, a greyscale frame must then drain before the
                    # Sleep behind it lands — measured at seconds on a cold
                    # link, which is that long with the panels still lit after
                    # the service was told to stop.
                    break
                self._run_due()
                self.paint()
        finally:
            epoll.close()
            self.shutdown()
        return 0

    def _on_signal(self, _signum: int, _frame: object) -> None:
        # Only sets a flag. Doing the work here would run panel I/O inside a
        # signal handler, re-entering code that is mid-write.
        self._running = False

    def shutdown(self) -> None:
        """Leave the hardware in a defensible state.

        Panels are put to sleep rather than left displaying a frozen clock,
        which would be indistinguishable from a running daemon and is the more
        misleading of the two failures.
        """
        for panel in self.panels.values():
            try:
                panel.sleep()
                panel.drain()
            except (transport.PanelGone, OSError):
                pass
            panel.close()
        self.panels.clear()
        self.volume.close()
        if self.watcher is not None:
            self.watcher.close()
        if self._wakeup_w >= 0:
            signal.set_wakeup_fd(-1)
            for fd in (self._wakeup_r, self._wakeup_w):
                try:
                    os.close(fd)
                except OSError:
                    pass
            self._wakeup_r = self._wakeup_w = -1

    # ── fd registration ──────────────────────────────────────────────────────

    def _sync_registrations(self, epoll: select.epoll, registered: dict[int, str]) -> None:
        """Keep epoll's set matching the fds that currently exist.

        The audio fd is not stable: it changes across a respawn and is -1 while
        there is no child. A dead child's pipe also reads ready-at-EOF forever,
        so leaving one registered would spin the loop at 100% of a core.
        """
        if self.volume.ensure_alive(time.monotonic()):
            # A respawned child almost always gets the dead one's fd number
            # back, and closing an fd silently drops it from the epoll set — so
            # an unchanged number does NOT mean it is still registered. Drop the
            # bookkeeping so the loop below registers it again for real.
            for fd, name in list(registered.items()):
                if name == "volume":
                    self._unregister(epoll, registered, fd)

        wanted: dict[int, str] = {}
        if self.watcher is not None:
            wanted[self.watcher.fileno()] = "udev"
        if self.volume.fileno() >= 0:
            wanted[self.volume.fileno()] = "volume"
        if self._wakeup_r >= 0:
            wanted[self._wakeup_r] = "signal"

        for fd in list(registered):
            if registered[fd] != wanted.get(fd):
                self._unregister(epoll, registered, fd)
        for fd, name in wanted.items():
            if fd not in registered:
                try:
                    epoll.register(fd, select.EPOLLIN)
                except FileExistsError:
                    epoll.modify(fd, select.EPOLLIN)
                registered[fd] = name

    @staticmethod
    def _unregister(epoll: select.epoll, registered: dict[int, str], fd: int) -> None:
        try:
            epoll.unregister(fd)
        except OSError:
            pass  # already dropped when the fd was closed
        registered.pop(fd, None)

    def _handle_ready(self, source: str | None) -> None:
        if source == "udev":
            self._handle_udev()
        elif source == "volume":
            if self.volume.read_events():
                self._on_volume_changed()
        elif source == "signal":
            # Drain the wakeup bytes. The Python-level handler has already run
            # and set the flag; this only stops the fd staying readable.
            try:
                os.read(self._wakeup_r, 1024)
            except OSError:
                pass

    # ── deadlines ────────────────────────────────────────────────────────────

    def _panels_incomplete(self) -> bool:
        """Whether discovery should be retried.

        Note the `not self.panels` arm: with no panels found at all there is
        nothing to iterate, so a bare `any(...)` is vacuously False and the
        retry would never fire — the exact case where retrying matters most.
        """
        return not self.panels or any(not p.connected for p in self.panels.values())

    def _next_deadline(self) -> float:
        deadlines = [self._due["tick"], self._due["usage"], self._due["prune"]]
        # Only while charging. Left in unconditionally it would sit permanently
        # in the past once charging stopped, making every poll return
        # immediately and spinning the loop at 100% of a core.
        if self.state.power.charging:
            deadlines.append(self._due["pulse"])
        deadlines += [t.expires_at for t in self.takeovers.values()]
        if self._panels_incomplete():
            deadlines.append(self._panel_retry_at)
        deadlines += [
            p.next_keepalive_at() for p in self.panels.values() if p.connected
        ]
        return min(deadlines)

    def _run_due(self) -> None:
        now = time.monotonic()

        if now >= self._due["tick"]:
            self._due["tick"] = now + TICK
            self.state.session = claude_session.read()
            self.state.screen_off = screen.is_screen_off()

        if now >= self._due["usage"]:
            self._due["usage"] = now + USAGE_INTERVAL
            self._start_usage_fetch()

        if now >= self._due["prune"]:
            self._due["prune"] = now + PRUNE_INTERVAL
            claude_session.prune()

        for side in [s for s, t in self.takeovers.items() if now >= t.expires_at]:
            del self.takeovers[side]

        if self.state.power.charging and now >= self._due["pulse"]:
            # Advance from the deadline, not from `now`, so the phase does not
            # drift when a slow iteration delays the wakeup.
            self._due["pulse"] = max(now, self._due["pulse"] + PULSE_STEP)
            self._pulse_phase = (self._pulse_phase + PULSE_STEP / PULSE_PERIOD) % 1.0

        if now >= self._panel_retry_at and self._panels_incomplete():
            self.reconcile_panels()

        for side, panel in list(self.panels.items()):
            if panel.connected and panel.keepalive_due(now) and not self._asleep:
                self._guard(side, panel.keepalive)

    # ── inputs ───────────────────────────────────────────────────────────────

    def _handle_udev(self) -> None:
        assert self.watcher is not None
        panels_changed = False
        for event in self.watcher.read_events():
            if udev.affects_panels(event):
                panels_changed = True
            elif udev.affects_power(event):
                self._on_power_event()
            elif udev.affects_brightness(event):
                self._on_brightness_event()
        if panels_changed:
            self.reconcile_panels()

    def _on_power_event(self) -> None:
        was_on_ac = self.state.power.on_ac
        self.state.power = power.read()
        # Only the AC *edge* is a takeover. The battery percentage also arrives
        # on these events and changes constantly; taking over for it would put a
        # gauge on the panel every time the charge moved by one point.
        if self.state.power.on_ac != was_on_ac:
            self.take_over("left", self.state.power.battery_pct)

    def _on_brightness_event(self) -> None:
        self.state.screen_fraction = screen.read_fraction()
        if not screen.changed_automatically():
            self.take_over("left", self._fraction_of_screen())

    def _on_volume_changed(self) -> None:
        volume = self.volume.current()
        if volume is None:
            return
        # Muted shows an empty gauge rather than the level behind it: the
        # question a volume takeover answers is "what will I hear", and the
        # answer while muted is nothing.
        self.take_over("left", 0.0 if volume.muted else volume.fraction)

    def _fraction_of_screen(self) -> float | None:
        fraction = self.state.screen_fraction
        if fraction is None:
            return None
        # Rescaled onto the reachable range so the gauge uses its full height:
        # the keybinds floor the screen at 2%, so a raw fraction would never
        # show an empty bar and the bottom of the scale would be wasted.
        floor = screen.SCREEN_FLOOR_FRACTION
        return max(0.0, (fraction - floor) / (1.0 - floor))

    # ── takeovers ────────────────────────────────────────────────────────────

    def take_over(self, side: str, fraction: float | None) -> None:
        """Show a gauge on `side` for TAKEOVER_SECONDS.

        The right panel never takes over — it carries no value you can change,
        so a takeover there could only ever be an interruption. Silently
        ignored rather than raising, since the caller is an event handler and
        this is a policy, not a bug.
        """
        if side != "left" or fraction is None or self.state.screen_off:
            return
        log.debug("takeover %s at %.0f%%", side, fraction * 100)
        self.takeovers[side] = Takeover(
            frame=render.render_gauge(max(0.0, min(1.0, fraction))),
            expires_at=time.monotonic() + TAKEOVER_SECONDS,
        )

    # ── state ────────────────────────────────────────────────────────────────

    def refresh_all(self) -> None:
        self.state.power = power.read()
        self.state.screen_fraction = screen.read_fraction()
        self.state.screen_off = screen.is_screen_off()
        self.state.session = claude_session.read()
        self._start_usage_fetch()

    def _start_usage_fetch(self) -> None:
        """Fetch the rate limits off the loop, in a throwaway thread.

        The one place this daemon is not single-threaded, and the reason is
        specific: every other input is a local fd or a sysfs read measured in
        microseconds, but this is an HTTPS request over a network that may be
        absent, captive, or simply slow. Called inline it would stall the loop
        for up to REQUEST_TIMEOUT — ten seconds during which the clock stops,
        the panels do not follow the screen, and volume takeovers do not appear.

        Safe because the thread touches exactly one attribute and does so with a
        single reference assignment, and because at most one runs at a time.
        """
        if self._usage_thread is not None and self._usage_thread.is_alive():
            return  # a slow request must not pile up behind itself

        def worker() -> None:
            # The source takes its clock as an argument so staleness stays
            # testable; monotonic is the right one, since is_stale() compares
            # against it and a wall-clock jump must not age the data.
            fetched = usage.fetch(time.monotonic())
            if fetched is not None:
                # Rebinding one attribute; no read-modify-write, so no lock.
                self.state.usage = fetched

        self._usage_thread = threading.Thread(
            target=worker, name="usage-fetch", daemon=True
        )
        self._usage_thread.start()

    # ── frames ───────────────────────────────────────────────────────────────

    def ambient_frames(self) -> dict[str, render.Frame]:
        now = time.localtime()
        left = render.render_machine(
            render.MachineState(
                hour=now.tm_hour,
                minute=now.tm_min,
                battery_pct=self.state.power.battery_pct or 0.0,
                charging=self.state.power.charging,
            )
        )

        session = self.state.session
        # Stale numbers are dropped rather than shown. A rate-limit bar that
        # stopped updating hours ago is worse than an empty zone: it looks
        # current, and the whole point of the panel is a glance you can trust.
        current = self.state.usage
        if current is not None and current.is_stale(time.monotonic()):
            current = None
        five = current.five_hour if current else None
        seven = current.seven_day if current else None
        # The status line payload carries the same numbers and is the fallback
        # when the endpoint is unreachable — but only while a session is open,
        # which is why it is not the primary source.
        right = render.render_claude(
            render.ClaudeState(
                five_hour_pct=five.percent if five else (session.five_hour_pct if session else None),
                seven_day_pct=seven.percent if seven else (session.seven_day_pct if session else None),
                context_pct=session.context_pct if session else None,
                working=bool(session and session.working),
                five_hour_severity=five.severity if five else "normal",
                seven_day_severity=seven.severity if seven else "normal",
            )
        )
        return {"left": left, "right": right}

    def target_brightness(self, side: str) -> int:
        base = screen.panel_brightness(self.state.screen_fraction)
        # The pulse is whole-panel global brightness, so the clock breathes
        # along with the battery bar. That is the intended reading: the machine
        # panel indicating a machine state, with exactly one meaning assigned to
        # whole-panel breathing.
        if side == "left" and self.state.power.charging:
            return pulse_brightness(base, self._pulse_phase)
        return base

    # ── output ───────────────────────────────────────────────────────────────

    def paint(self) -> None:
        if self.state.screen_off:
            self._go_dark()
            return
        self._wake_if_needed()

        frames = self.ambient_frames()
        for side, panel in self.panels.items():
            if not panel.connected:
                continue
            self._guard(side, panel.set_brightness, self.target_brightness(side))

            takeover = self.takeovers.get(side)
            frame = takeover.frame if takeover else frames[side]
            if frame == self.painted.get(side):
                continue
            # Takeovers go out over the 25ms 1-bit path: they are a single solid
            # bar with no intensity information to lose, and they are the one
            # thing here with a latency budget you can perceive.
            self._guard(side, panel.draw, frame, fast=takeover is not None)
            if panel.connected:
                self.painted[side] = frame

    def _go_dark(self) -> None:
        """Follow the screen into DPMS off.

        Uses the firmware's Sleep rather than a blank frame: it cuts the LED
        controller instead of writing 306 zeroes, and the fade-out is free.
        """
        if self._asleep:
            return
        for side, panel in self.panels.items():
            if panel.connected:
                self._guard(side, panel.sleep)
        self.painted.clear()
        self.takeovers.clear()
        self._asleep = True

    def _wake_if_needed(self) -> None:
        """Bring the panels back when the screen does.

        Both are woken before waiting, so the fade runs in parallel and the loop
        blocks for one WAKE_SETTLE rather than two. That matters here: this runs
        the moment you touch the machine, and it is the one place the daemon
        stops serving events for a whole second.
        """
        if not self._asleep:
            return
        self._asleep = False
        waking = [(s, p) for s, p in self.panels.items() if p.connected]
        for side, panel in waking:
            self._guard(side, panel.begin_wake)
        if waking:
            time.sleep(transport.WAKE_SETTLE)

    # ── panels ───────────────────────────────────────────────────────────────

    def reconcile_panels(self) -> None:
        """Match the open panels to the ones that actually exist.

        Driven by re-running discovery rather than by event payloads: `remove`
        events carry almost no properties, so identifying a panel from one would
        work in testing and fail exactly when a panel vanished.
        """
        self._panel_retry_at = time.monotonic() + PANEL_RETRY
        found = transport.discover()

        for side in [s for s in self.panels if s not in found]:
            log.info("panel %s disappeared", side)
            self.panels[side].close()
            del self.panels[side]
            self.painted.pop(side, None)
            self.takeovers.pop(side, None)

        for side, path in found.items():
            panel = self.panels.get(side)
            if panel is None:
                panel = transport.Panel(side, path)
                self.panels[side] = panel
            if panel.connected and panel.path == path:
                continue
            panel.path = path
            self.painted.pop(side, None)  # it comes back blank
            if panel.reconnect(brightness=self.target_brightness(side)):
                log.info("panel %s connected on %s", side, path)
            else:
                # Expected before the seat session goes active: the uaccess ACL
                # is not applied until then, so the first open is EACCES.
                log.debug("panel %s not available yet", side)

    def _guard(self, side: str, fn, *args, **kwargs) -> None:
        """Run a panel operation, treating disappearance as normal.

        The write backstop exists because a module can wedge without emitting a
        udev event; udev is the fast path, this is the one that always fires.
        """
        try:
            fn(*args, **kwargs)
        except transport.PanelGone as exc:
            log.info("panel %s: %s", side, exc)
            self.painted.pop(side, None)
            self._panel_retry_at = time.monotonic() + PANEL_RETRY
        except OSError as exc:
            log.warning("panel %s: %s", side, exc)


def main(argv: list[str] | None = None) -> int:
    import argparse

    parser = argparse.ArgumentParser(prog="matrixd", description=__doc__.splitlines()[0])
    parser.add_argument("-v", "--verbose", action="store_true")
    args = parser.parse_args(argv)

    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(levelname)s %(message)s",
    )
    return Daemon().run()
