"""Tests for the transport layer that do not need hardware attached.

The protocol framing and the connection state machine are testable against a
plain pipe; the timing constants are testable as relationships. What is left —
the settle, the wake fade, the actual draw — is only meaningful against a real
module and lives in tools/smoke.py instead.
"""

import errno
import os
import time
import unittest

from matrixd import render as r
from matrixd import transport as t


class FakePanel(t.Panel):
    """A Panel whose fd is the write end of a pipe, so writes are inspectable."""

    def __init__(self):
        super().__init__("left", "/dev/null")
        self._read_fd, self._fd = os.pipe()

    def written(self) -> bytes:
        os.set_blocking(self._read_fd, False)
        try:
            return os.read(self._read_fd, 65536)
        except BlockingIOError:
            return b""


class TestFraming(unittest.TestCase):
    def test_every_command_carries_the_magic_prefix(self):
        p = FakePanel()
        p._send(t.CMD_BRIGHTNESS, bytes([42]))
        self.assertEqual(p.written(), b"\x32\xAC\x00\x2A")

    def test_sleep_and_wake_are_distinct(self):
        p = FakePanel()
        p.sleep()
        self.assertEqual(p.written(), b"\x32\xAC\x03\x01")

    def test_drawbw_sends_one_command_of_39_bytes(self):
        p = FakePanel()
        p.draw(r.blank(), fast=True)
        out = p.written()
        self.assertEqual(len(out), 3 + 39)
        self.assertEqual(out[:3], b"\x32\xAC\x06")

    def test_greyscale_sends_nine_columns_then_a_flush(self):
        """Ten writes, not one — which is why a greyscale frame costs 169ms on
        the wire while DrawBW costs 25ms."""
        p = FakePanel()
        p.draw(r.blank())
        out = p.written()
        self.assertEqual(len(out), 9 * (3 + 35) + 3)
        self.assertEqual(out[-3:], b"\x32\xAC\x08")  # FlushCols last
        for col in range(9):
            off = col * 38
            self.assertEqual(out[off : off + 3], b"\x32\xAC\x07")
            self.assertEqual(out[off + 3], col)  # column index leads payload


class TestBrightness(unittest.TestCase):
    def test_repeated_values_are_not_resent(self):
        """Brightness is on the keepalive path; resending it needlessly would
        double traffic for no visible change."""
        p = FakePanel()
        p.set_brightness(40)
        first = p.written()
        p.set_brightness(40)
        self.assertEqual(first, b"\x32\xAC\x00\x28")
        self.assertEqual(p.written(), b"")

    def test_clamped_to_a_byte(self):
        p = FakePanel()
        p.set_brightness(9999)
        self.assertEqual(p.written()[-1], 255)
        p.set_brightness(-5)
        self.assertEqual(p.written()[-1], 0)


class TestConnectionState(unittest.TestCase):
    def test_send_without_a_connection_raises_panel_gone(self):
        p = t.Panel("left", "/dev/null")
        with self.assertRaises(t.PanelGone):
            p.set_brightness(10)

    def test_write_error_marks_the_panel_gone(self):
        """The backstop for a device that wedges without a udev event."""
        p = FakePanel()
        os.close(p._read_fd)  # writing now raises EPIPE
        with self.assertRaises(t.PanelGone):
            for _ in range(100):
                p._send(t.CMD_BRIGHTNESS, bytes([1]))
        self.assertFalse(p.connected)

    def test_epipe_is_treated_as_gone(self):
        self.assertIn(errno.EPIPE, t._GONE)
        self.assertIn(errno.ENODEV, t._GONE)
        self.assertIn(errno.EIO, t._GONE)

    def test_open_returns_false_rather_than_raising(self):
        """EACCES is expected before the seat session goes active."""
        p = t.Panel("left", "/nonexistent/device")
        self.assertFalse(p.open())
        self.assertFalse(p.connected)


class TestKeepalive(unittest.TestCase):
    def test_interval_is_inside_the_firmware_idle_timeout(self):
        """If this fails the firmware will blank the panels between keepalives.

        The module sleeps after IDLE_TIMEOUT with no traffic, so the keepalive
        must fire comfortably inside it — not merely before it.
        """
        self.assertLess(t.KEEPALIVE_INTERVAL, t.IDLE_TIMEOUT)
        self.assertLessEqual(t.KEEPALIVE_INTERVAL, t.IDLE_TIMEOUT / 2)

    def test_due_only_after_the_interval(self):
        p = FakePanel()
        p.set_brightness(10)  # stamps _last_write
        now = time.monotonic()
        self.assertFalse(p.keepalive_due(now))
        self.assertFalse(p.keepalive_due(now + t.KEEPALIVE_INTERVAL - 1))
        self.assertTrue(p.keepalive_due(now + t.KEEPALIVE_INTERVAL + 1))

    def test_keepalive_uses_the_cheapest_command(self):
        """13ms for a brightness resend versus 169ms to redraw an unchanged
        frame."""
        p = FakePanel()
        p.set_brightness(40)
        p.written()
        p.keepalive()
        self.assertEqual(len(p.written()), 4)


class TestDiscovery(unittest.TestCase):
    def test_side_mapping_is_the_measured_one(self):
        """The lower-numbered USB port is the RIGHT bay. This is the opposite of
        the intuitive guess and can only be found by lighting a panel."""
        self.assertEqual(t.SIDE_BY_USB_PORT["3.3"], "right")
        self.assertEqual(t.SIDE_BY_USB_PORT["4.2"], "left")

    def test_discover_returns_known_sides_only(self):
        for side in t.discover():
            self.assertIn(side, ("left", "right"))


if __name__ == "__main__":
    unittest.main()
