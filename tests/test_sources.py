"""Tests for the data sources.

The parsing and mapping are pure and tested here. The I/O — sysfs reads, the
HTTPS call — is exercised against the live machine by tools/, not mocked into
something that proves nothing.
"""

import unittest

from matrixd import render as r
from matrixd.sources import power, screen, usage

# Shape of a real response, trimmed. Note the top-level five_hour/seven_day
# objects carry no severity — that lives in limits[] under DIFFERENT names.
LIVE_SHAPE = {
    "five_hour": {
        "utilization": 17.0,
        "resets_at": "2026-08-07T15:29:59.928085+00:00",
    },
    "seven_day": {
        "utilization": 5.0,
        "resets_at": "2026-08-11T14:59:59.928108+00:00",
    },
    "limits": [
        {"kind": "session", "percent": 17, "severity": "normal", "is_active": True},
        {"kind": "weekly_all", "percent": 5, "severity": "normal", "is_active": False},
        {"kind": "weekly_scoped", "percent": 0, "severity": "normal"},
    ],
}


class TestUsageParsing(unittest.TestCase):
    def test_extracts_both_windows(self):
        u = usage.parse(LIVE_SHAPE, fetched_at=100.0)
        self.assertIsNotNone(u)
        self.assertEqual(u.five_hour.percent, 17.0)
        self.assertEqual(u.seven_day.percent, 5.0)
        self.assertIsNotNone(u.five_hour.resets_at)

    def test_severity_comes_from_limits_under_different_names(self):
        """The trap: limits[] calls the 5-hour window "session" and the 7-day
        one "weekly_all", matching neither top-level key."""
        payload = dict(LIVE_SHAPE)
        payload["limits"] = [
            {"kind": "session", "severity": "critical"},
            {"kind": "weekly_all", "severity": "warning"},
        ]
        u = usage.parse(payload, 0.0)
        self.assertEqual(u.five_hour.severity, "critical")
        self.assertEqual(u.seven_day.severity, "warning")

    def test_missing_limits_defaults_to_normal(self):
        payload = {k: v for k, v in LIVE_SHAPE.items() if k != "limits"}
        u = usage.parse(payload, 0.0)
        self.assertEqual(u.five_hour.severity, "normal")

    def test_unknown_severity_is_preserved_not_normalised(self):
        """render._intensity treats anything non-"normal" as noteworthy, so the
        parser must not flatten an unrecognised value into "normal"."""
        payload = dict(LIVE_SHAPE)
        payload["limits"] = [{"kind": "session", "severity": "some-future-value"}]
        u = usage.parse(payload, 0.0)
        self.assertEqual(u.five_hour.severity, "some-future-value")
        self.assertEqual(r._intensity(u.five_hour.severity), r.EMPHASIS)

    def test_garbage_returns_none_rather_than_raising(self):
        """The endpoint is undocumented; a shape change must drop the zone, not
        take down the daemon."""
        for junk in (None, [], "", 42, {}, {"five_hour": "not a dict"}):
            self.assertIsNone(usage.parse(junk, 0.0))

    def test_one_window_missing_is_still_usable(self):
        payload = {"five_hour": {"utilization": 3.0}}
        u = usage.parse(payload, 0.0)
        self.assertIsNotNone(u)
        self.assertIsNone(u.seven_day)

    def test_unparseable_reset_time_does_not_lose_the_window(self):
        payload = {"five_hour": {"utilization": 3.0, "resets_at": "not a date"}}
        u = usage.parse(payload, 0.0)
        self.assertEqual(u.five_hour.percent, 3.0)
        self.assertIsNone(u.five_hour.resets_at)

    def test_staleness(self):
        u = usage.parse(LIVE_SHAPE, fetched_at=0.0)
        self.assertFalse(u.is_stale(60.0))
        self.assertTrue(u.is_stale(usage.STALE_AFTER + 1))

    def test_poll_interval_is_not_wasteful(self):
        """Utilization is reported in whole percent; polling faster than a
        minute burns authenticated requests for no new information."""
        self.assertGreaterEqual(usage.POLL_INTERVAL, 60.0)


class TestBrightnessMapping(unittest.TestCase):
    def test_anchored_on_the_calibrated_points(self):
        """screen 2% is the floor because brightnessctl -n2 makes it the lowest
        reachable value. Anchoring at 0 instead puts the panel at 8 when the
        screen is at minimum — nearly 3x the calibrated floor."""
        self.assertEqual(screen.panel_brightness(0.02), r.AMBIENT_FLOOR)
        self.assertEqual(screen.panel_brightness(1.00), r.AMBIENT_CEILING)

    def test_below_the_floor_clamps_rather_than_going_dark(self):
        self.assertEqual(screen.panel_brightness(0.0), r.AMBIENT_FLOOR)
        self.assertEqual(screen.panel_brightness(-1.0), r.AMBIENT_FLOOR)

    def test_above_the_ceiling_clamps(self):
        self.assertEqual(screen.panel_brightness(5.0), r.AMBIENT_CEILING)

    def test_unreadable_screen_falls_back_to_the_floor(self):
        """Failing dim is right: the daemon should not blast the panels because
        it could not read a sysfs file."""
        self.assertEqual(screen.panel_brightness(None), r.AMBIENT_FLOOR)

    def test_monotonic(self):
        prev = -1
        for i in range(0, 101):
            value = screen.panel_brightness(i / 100)
            self.assertGreaterEqual(value, prev)
            prev = value

    def test_rules_become_visible_in_the_expected_range(self):
        """Documented behaviour: rules appear around 5% screen. If the mapping
        or the floor changes, this is where it shows up."""
        self.assertFalse(r.is_visible(screen.panel_brightness(0.02), r.RULE))
        self.assertTrue(r.is_visible(screen.panel_brightness(0.10), r.RULE))


class TestPower(unittest.TestCase):
    def test_read_never_raises(self):
        p = power.read()
        self.assertIsInstance(p.charging, bool)
        self.assertIsInstance(p.on_ac, bool)
        if p.battery_pct is not None:
            self.assertGreaterEqual(p.battery_pct, 0.0)
            self.assertLessEqual(p.battery_pct, 100.0)

    def test_only_charging_counts_as_charging(self):
        """Charging drives a panel-wide pulse, so every other status must be
        false. "Full" on mains would otherwise pulse forever, and "Not charging"
        is what a charge-limit threshold reports."""
        self.assertTrue(power.is_charging("Charging"))
        for status in ("Discharging", "Full", "Not charging", "Unknown", None, ""):
            self.assertFalse(power.is_charging(status), status)


if __name__ == "__main__":
    unittest.main()
