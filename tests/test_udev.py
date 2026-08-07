"""Tests for the netlink uevent parser.

The wire format is tested against synthetic messages built to the same layout as
the real ones captured from the kernel. Malformed input matters here: this is a
socket carrying messages from outside the process, and a parser that throws
would take the daemon down with it.

NOT covered: the panel add/remove path end to end. Generating a tty add/remove
event requires deauthorising a USB device, which needs root, and the modules are
internal so they cannot be unplugged. The parse and filter halves are tested;
the "does a real disconnect produce a real reconnect" half is not, and is the
most likely place for a latent bug.
"""

import struct
import unittest

from matrixd.sources import udev


def message(properties: dict[str, str], *, magic: int = 0xFEEDCAFE) -> bytes:
    """Build a libudev netlink datagram matching the captured layout.

    prefix(8) | magic BE(4) | header_size(4) | props_off(4) | props_len(4) |
    16 bytes of further header fields | properties
    """
    body = b"".join(f"{k}={v}".encode() + b"\0" for k, v in properties.items())
    header = (
        b"libudev\0"
        + struct.pack(">I", magic)
        + struct.pack("=III", 40, 40, len(body))
        + b"\0" * 16
    )
    return header + body


class TestParsing(unittest.TestCase):
    def test_parses_a_realistic_message(self):
        raw = message(
            {
                "ACTION": "add",
                "DEVPATH": "/devices/pci0000:00/usb3/3-3/3-3.3/tty/ttyACM0",
                "SUBSYSTEM": "tty",
                "DEVNAME": "/dev/ttyACM0",
                "ID_VENDOR_ID": "32ac",
            }
        )
        e = udev.parse_message(raw)
        self.assertEqual(e.action, "add")
        self.assertEqual(e.subsystem, "tty")
        self.assertEqual(e.properties["ID_VENDOR_ID"], "32ac")

    def test_values_containing_equals_survive(self):
        """Real properties do contain '=' — SYSTEMD_WANTS carries a unit name
        with an '@' and a ':', and partition() must not split on the wrong one.
        """
        raw = message({"ACTION": "change", "SUBSYSTEM": "backlight", "X": "a=b=c"})
        self.assertEqual(udev.parse_message(raw).properties["X"], "a=b=c")

    def test_rejects_non_libudev_framing(self):
        """Kernel-group messages use 'ACTION@DEVPATH\\0...' framing instead."""
        self.assertIsNone(udev.parse_message(b"add@/devices/foo\0SUBSYSTEM=tty\0"))

    def test_rejects_bad_magic(self):
        self.assertIsNone(udev.parse_message(message({"ACTION": "add"}, magic=0xDEAD)))

    def test_rejects_truncated_input(self):
        raw = message({"ACTION": "add", "SUBSYSTEM": "tty"})
        for cut in (0, 4, 8, 12, 39, len(raw) - 1):
            self.assertIsNone(udev.parse_message(raw[:cut]), f"cut at {cut}")

    def test_rejects_message_without_an_action(self):
        self.assertIsNone(udev.parse_message(message({"SUBSYSTEM": "tty"})))

    def test_garbage_never_raises(self):
        for junk in (b"", b"\0" * 40, b"libudev\0" + b"\xff" * 64, bytes(range(256))):
            udev.parse_message(junk)  # must not raise


class TestClassification(unittest.TestCase):
    def _event(self, action: str, subsystem: str) -> udev.Event:
        return udev.parse_message(
            message({"ACTION": action, "SUBSYSTEM": subsystem, "DEVPATH": "/x"})
        )

    def test_tty_add_and_remove_trigger_a_panel_reconcile(self):
        for action in ("add", "remove", "bind", "unbind"):
            self.assertTrue(udev.affects_panels(self._event(action, "tty")))

    def test_tty_change_does_not(self):
        """A 'change' on an already-present tty is not a connect or disconnect,
        and reconciling on it would mean redrawing for no reason."""
        self.assertFalse(udev.affects_panels(self._event("change", "tty")))

    def test_other_subsystems_do_not_trigger_a_panel_reconcile(self):
        for subsystem in ("backlight", "power_supply", "usb", "block"):
            self.assertFalse(udev.affects_panels(self._event("add", subsystem)))

    def test_power_supply_events_trigger_a_power_reread(self):
        for action in ("add", "remove", "change"):
            self.assertTrue(udev.affects_power(self._event(action, "power_supply")))

    def test_backlight_is_not_treated_as_power(self):
        """Backlight emits uevents indistinguishable from a user keypress, which
        is exactly why brightness comes from a keybind hook instead — hypridle
        writes brightness on idle, and a udev-driven takeover would fire as you
        walked away."""
        self.assertFalse(udev.affects_power(self._event("change", "backlight")))
        self.assertFalse(udev.affects_panels(self._event("change", "backlight")))


class TestWatcher(unittest.TestCase):
    def test_binds_unprivileged_and_exposes_a_pollable_fd(self):
        with udev.Watcher() as w:
            self.assertGreaterEqual(w.fileno(), 0)
            self.assertEqual(w.read_events(), [])  # non-blocking when idle

    def test_default_subsystems_cover_both_jobs(self):
        with udev.Watcher() as w:
            self.assertIn("tty", w.subsystems)  # panel connect/disconnect
            self.assertIn("power_supply", w.subsystems)  # AC plug/unplug


if __name__ == "__main__":
    unittest.main()
