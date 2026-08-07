"""Serial transport for the LED Matrix modules.

Owns the protocol and the connection lifecycle. Everything here is shaped by
measurements rather than guesses — see README for the numbers:

- **Ports stay open for the process lifetime.** Opening costs ~0.2s of CDC-ACM
  line-state settling before the device accepts anything, and a wake costs
  another ~0.4s of LED fade during which commands are silently dropped. That is
  more than the entire takeover latency budget, so reopening per frame is not an
  option.
- **The firmware sleeps on a ~60s idle timer**, reset by any command. An
  always-on display must therefore send traffic more often than that or the
  firmware blanks it. Hence `keepalive()`.
- **Two draw paths, deliberately.** Greyscale is 169ms and carries per-pixel
  intensity; 1-bit DrawBW is 25ms and does not. Ambient frames change rarely so
  they can afford greyscale; takeovers and post-reconnect repaints cannot.

**Writes do not block.** The tty buffers them, so `draw()` returns in ~0.1ms
while the data takes its measured time to reach the device — 165ms for a
greyscale frame, 25ms for DrawBW, 13ms for a brightness change. Two consequences
the daemon has to respect:

- A frame is **not on screen when `draw()` returns.** Nothing here waits for it;
  use `drain()` if you genuinely need that.
- Anything written behind a queued greyscale frame is delayed by up to 165ms.
  A takeover landing mid-frame appears late even though its own write is 25ms.
  `is_busy()` exposes whether anything is still draining, so the caller can
  skip an ambient redraw rather than deepen it. Flushing the queue is *not*
  offered: truncating a StageCol mid-payload would desync the module's command
  parser, which is far worse than a late frame.

In practice the collision window is small — ambient frames are rare (the clock
ticks once a minute) so the odds of a takeover landing inside a 165ms window are
well under 1%.
"""

from __future__ import annotations

import errno
import fcntl
import glob
import os
import struct
import termios
import time

from . import render

# ── protocol ─────────────────────────────────────────────────────────────────

MAGIC = b"\x32\xAC"

CMD_BRIGHTNESS = 0x00
CMD_SLEEP = 0x03
CMD_DRAW_BW = 0x06
CMD_STAGE_COL = 0x07
CMD_FLUSH_COLS = 0x08
CMD_VERSION = 0x20

VENDOR_ID = "32ac"
PRODUCT_ID = "0020"

# Which physical bay each USB port path corresponds to. Cannot be derived from
# USB topology — found by lighting one panel and looking. The lower-numbered
# port is the RIGHT bay, which is the opposite of what you would guess.
SIDE_BY_USB_PORT = {"3.3": "right", "4.2": "left"}

# ── timings, all measured ────────────────────────────────────────────────────

OPEN_SETTLE = 0.2  # CDC-ACM line state; commands before this are dropped
WAKE_SETTLE = 0.4  # LED fade-in; the module ignores commands during it
IDLE_TIMEOUT = 60.0  # firmware sleeps after this, reset by any command
KEEPALIVE_INTERVAL = 30.0  # comfortably inside IDLE_TIMEOUT

# Linux ioctl for the tty output queue. Note the units are driver-specific —
# see Panel.output_queue().
TIOCOUTQ = 0x5411

# Errors that mean the device is gone rather than merely unhappy.
_GONE = frozenset({errno.ENODEV, errno.EIO, errno.ENXIO, errno.EBADF, errno.EPIPE})


class PanelGone(Exception):
    """The module disappeared mid-write. Caller should reconnect."""


def discover() -> dict[str, str]:
    """Map side name -> /dev/serial/by-path device, for panels present now.

    Enumerates by USB topology rather than by-id: both modules ship with the
    same USB serial number, so /dev/serial/by-id/ collapses to a single symlink
    and cannot distinguish them. by-path is also stable across the ttyACM*
    renumbering that can happen over a suspend cycle, which would otherwise
    silently mirror the entire layout.

    Vendor/product are confirmed via sysfs rather than trusted from the path, so
    an unrelated CDC-ACM device can never be mistaken for a panel.
    """
    found: dict[str, str] = {}
    for path in sorted(glob.glob("/dev/serial/by-path/*-usb-*")):
        tty = os.path.basename(os.path.realpath(path))
        sysdev = f"/sys/class/tty/{tty}/device/.."
        try:
            with open(os.path.join(sysdev, "idVendor")) as f:
                vid = f.read().strip()
            with open(os.path.join(sysdev, "idProduct")) as f:
                pid = f.read().strip()
            with open(os.path.join(sysdev, "devpath")) as f:
                port = f.read().strip()
        except OSError:
            continue
        if (vid, pid) != (VENDOR_ID, PRODUCT_ID):
            continue
        side = SIDE_BY_USB_PORT.get(port)
        if side:
            found[side] = path
    return found


class Panel:
    """One LED matrix module.

    Not thread-safe: one owner per panel. Reconnection is driven by the caller
    (udev is authoritative and immediate), with write errors as a backstop for
    the case where a device wedges without emitting an event.
    """

    def __init__(self, side: str, path: str) -> None:
        self.side = side
        self.path = path
        self._fd: int | None = None
        self._brightness = 0
        self._last_write = 0.0
        # Kept so a reconnect can repaint immediately rather than waiting for
        # the next render tick — the modules come back blank.
        self._last_frame: render.Frame | None = None

    # ── lifecycle ────────────────────────────────────────────────────────────

    @property
    def connected(self) -> bool:
        return self._fd is not None

    def open(self) -> bool:
        """Open and settle the port. False if unavailable — never raises.

        EACCES is expected rather than exceptional: uaccess ACLs are applied by
        logind when the seat session becomes active, so a daemon started before
        that will legitimately fail here and should retry.
        """
        if self._fd is not None:
            return True
        try:
            fd = os.open(self.path, os.O_RDWR | os.O_NOCTTY)
        except OSError:
            return False

        try:
            iflag, oflag, cflag, lflag, _i, _o, cc = termios.tcgetattr(fd)
            cc = list(cc)
            cc[termios.VMIN] = 0
            cc[termios.VTIME] = 5
            termios.tcsetattr(
                fd,
                termios.TCSANOW,
                [
                    0,
                    0,
                    termios.CS8 | termios.CREAD | termios.CLOCAL,
                    0,
                    termios.B115200,
                    termios.B115200,
                    cc,
                ],
            )
            termios.tcflush(fd, termios.TCIOFLUSH)
        except OSError:
            os.close(fd)
            return False

        time.sleep(OPEN_SETTLE)
        self._fd = fd
        self._brightness = 0
        return True

    def close(self) -> None:
        if self._fd is not None:
            try:
                os.close(self._fd)
            except OSError:
                pass
            self._fd = None

    # ── raw I/O ──────────────────────────────────────────────────────────────

    def _send(self, cmd: int, payload: bytes = b"") -> None:
        if self._fd is None:
            raise PanelGone(f"{self.side}: not connected")
        try:
            os.write(self._fd, MAGIC + bytes([cmd]) + payload)
        except OSError as exc:
            if exc.errno in _GONE:
                self.close()
                raise PanelGone(f"{self.side}: {exc.strerror}") from exc
            raise
        self._last_write = time.monotonic()

    # ── commands ─────────────────────────────────────────────────────────────

    def wake(self) -> None:
        """Wake and wait out the fade.

        The module does not service commands while fading in, so anything sent
        during WAKE_SETTLE is silently lost. This is the single most common way
        to conclude a working module is broken.
        """
        self._send(CMD_SLEEP, bytes([0]))
        time.sleep(WAKE_SETTLE)

    def sleep(self) -> None:
        """Power down the LED controller, with the firmware's own fade.

        Preferred over drawing a blank frame when going dark — this actually
        cuts the controller rather than setting every pixel to zero, and the
        fade is free.
        """
        self._send(CMD_SLEEP, bytes([1]))
        self._brightness = 0

    def set_brightness(self, value: int) -> None:
        """Global level for the whole panel, 0-255.

        Composes with per-pixel greyscale rather than overriding it: the
        effective output is the product, which is why visibility is governed by
        `global x greyscale` (see render.VISIBILITY_THRESHOLD).
        """
        value = max(0, min(255, int(value)))
        if value != self._brightness:
            self._send(CMD_BRIGHTNESS, bytes([value]))
            self._brightness = value

    def draw(self, frame: render.Frame, *, fast: bool = False) -> None:
        """Render a frame. `fast` uses the 25ms 1-bit path instead of 169ms."""
        if fast:
            self._send(CMD_DRAW_BW, render.to_drawbw(frame))
        else:
            for payload in render.to_columns(frame):
                self._send(CMD_STAGE_COL, payload)
            self._send(CMD_FLUSH_COLS)
        self._last_frame = frame

    def drain(self) -> None:
        """Block until everything written has actually reached the device."""
        if self._fd is not None:
            termios.tcdrain(self._fd)

    def output_queue(self) -> int:
        """Opaque measure of how much is still queued. **Not bytes.**

        TIOCOUTQ on cdc-acm reports `writesize x URBs in flight`, not a byte
        count: measured here as 1280 per outstanding write, so a greyscale frame
        (ten separate writes) reads 12800 for ~345 bytes of payload. It also
        saturates at 16 URBs (20480) however much more you queue.

        Useful only for comparison against 0 and against itself. Prefer
        `is_busy()`.
        """
        if self._fd is None:
            return 0
        buf = fcntl.ioctl(self._fd, TIOCOUTQ, struct.pack("I", 0))
        return struct.unpack("I", buf)[0]

    def is_busy(self) -> bool:
        """Whether anything is still draining to the device.

        Lets the caller skip an ambient redraw rather than stack it behind one
        already in flight, which is what would make a takeover appear late.
        """
        return self.output_queue() > 0

    def version(self, attempts: int = 3) -> str | None:
        """Firmware version as a string, e.g. "0.20", or None if no answer.

        Returns a formatted string rather than the raw bytes on purpose. Bytes
        0-1 are USB bcdDevice and are **BCD, not decimal**: 0x00 0x20 is version
        0.20, and handing back the integers invites a caller to print 0.32 —
        which is exactly the bug this docstring used to describe while the
        return type made it easy to reproduce.

        Retries because the first query after a wake is unreliable even with the
        fade accounted for: measured 2/3 on cold modules. The query is
        idempotent, so a retry costs nothing.
        """
        for _ in range(attempts):
            self._send(CMD_VERSION)
            if self._fd is None:
                return None
            buf = b""
            deadline = time.monotonic() + 2.0
            while len(buf) < 3 and time.monotonic() < deadline:
                chunk = os.read(self._fd, 3 - len(buf))
                if chunk:
                    buf += chunk
            if len(buf) == 3:
                pre = " (pre-release)" if buf[2] else ""
                return f"{buf[0]:x}.{buf[1]:02x}{pre}"
            time.sleep(0.3)
        return None

    # ── keepalive ────────────────────────────────────────────────────────────

    def keepalive_due(self, now: float | None = None) -> bool:
        now = time.monotonic() if now is None else now
        return now - self._last_write >= KEEPALIVE_INTERVAL

    def keepalive(self) -> None:
        """Cheapest possible traffic to reset the firmware idle timer.

        Re-sending brightness costs 14ms and changes nothing visible, versus
        169ms to redraw a frame that has not changed.
        """
        self._send(CMD_BRIGHTNESS, bytes([self._brightness]))

    # ── reconnect ────────────────────────────────────────────────────────────

    def reconnect(self, brightness: int | None = None) -> bool:
        """Reopen after a disconnect and repaint immediately.

        Modules come back blank and possibly asleep after a suspend cycle, so a
        bare reopen leaves a dark panel. Repaint goes out over the fast 1-bit
        path first so something is on screen within ~25ms; the caller can follow
        with a full greyscale render at its leisure.
        """
        self.close()
        if not self.open():
            return False
        try:
            self.wake()
            self.set_brightness(
                self._brightness if brightness is None else brightness
            )
            if self._last_frame is not None:
                self.draw(self._last_frame, fast=True)
        except PanelGone:
            return False
        return True
