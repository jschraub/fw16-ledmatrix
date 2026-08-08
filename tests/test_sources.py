"""Tests for the data sources.

The parsing and mapping are pure and tested here. The I/O — sysfs reads, the
HTTPS call — is exercised against the live machine by tools/, not mocked into
something that proves nothing.
"""

import json
import os
import select
import tempfile
import time
import unittest
import unittest.mock

from matrixd import render as r
from matrixd.sources import audio, claude_session, power, screen, usage

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

    def test_the_mapping_keeps_every_element_visible(self):
        """This used to assert that separator rules appeared around 5% screen
        brightness and were invisible below it. The layout has no rules any
        more — padding rows separate the zones — so the property inverts: there
        is nothing left that is allowed to vanish, and DATA must clear the
        threshold at every level the mapping can produce. If the mapping, the
        floor, or DATA changes, this is where it shows up."""
        for pct in (None, 0.0, 0.01, 0.02, 0.05, 0.5, 1.0):
            self.assertTrue(
                r.is_visible(screen.panel_brightness(pct), r.DATA),
                f"DATA is invisible at screen brightness {pct}",
            )


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


class TestVolumeParsing(unittest.TestCase):
    # Copied verbatim from this machine rather than written from memory.
    LIVE = (
        "Volume: front-left: 39960 /  61% / -12.89 dB,   "
        "front-right: 39960 /  61% / -12.89 dB\n        balance 0.00\n"
    )

    def test_parses_live_output(self):
        self.assertAlmostEqual(audio.parse_volume(self.LIVE), 0.61)

    def test_negative_db_figures_are_not_mistaken_for_levels(self):
        """-12.89 dB is the loudest-looking number on the line and carries no
        '%'. A looser number match would read the volume as 1289%."""
        self.assertAlmostEqual(audio.parse_volume(self.LIVE), 0.61)

    def test_takes_the_loudest_channel(self):
        """A balance offset makes the channels differ; you hear the loudest.
        Averaging would under-report a hard-panned sink."""
        text = "Volume: front-left: 0 / 0% / -inf dB,  front-right: 65536 / 100% / 0.00 dB"
        self.assertAlmostEqual(audio.parse_volume(text), 1.0)

    def test_over_amplification_is_reported_honestly(self):
        """PulseAudio allows >100%. Clamping here would report a false value;
        clamping belongs to the gauge, where 'full' is the question."""
        text = "Volume: mono: 98304 / 150% / 7.02 dB"
        self.assertAlmostEqual(audio.parse_volume(text), 1.5)

    def test_unparseable_returns_none(self):
        for text in ("", "Failure: No such entity", "Volume: mono: unknown"):
            self.assertIsNone(audio.parse_volume(text), text)

    def test_mute(self):
        self.assertIs(audio.parse_mute("Mute: yes"), True)
        self.assertIs(audio.parse_mute("Mute: no\n"), False)
        self.assertIsNone(audio.parse_mute("Failure: No such entity"))


class TestEventFiltering(unittest.TestCase):
    def test_sink_and_server_events_are_interesting(self):
        self.assertTrue(audio.is_interesting("Event 'change' on sink #55"))
        # Default sink moving (headphones in) changes the volume on show
        # without any event on the sink we were watching.
        self.assertTrue(audio.is_interesting("Event 'change' on server"))

    def test_client_events_are_ignored(self):
        """Every pactl invocation creates and destroys a client — including the
        re-read this module does in response to an event. Treating those as
        interesting would make the source trigger itself forever."""
        for line in (
            "Event 'new' on client #108",
            "Event 'change' on client #108",
            "Event 'remove' on client #108",
        ):
            self.assertFalse(audio.is_interesting(line), line)

    def test_stream_volume_is_not_sink_volume(self):
        """sink-input is one application's own fader, not the system volume."""
        self.assertFalse(audio.is_interesting("Event 'change' on sink-input #12"))

    def test_garbage_never_raises(self):
        for line in ("", "\n", "Got SIGINT, exiting.", "Event", "'''"):
            self.assertFalse(audio.is_interesting(line))


class TestSubscriberSupervision(unittest.TestCase):
    """The subscriber's whole reason to exist is surviving a PipeWire restart.

    Driven with an injected child and an injected clock, so these test the
    supervision logic rather than PipeWire's willingness to restart on cue.
    """

    DIES_AT_ONCE = ["sh", "-c", "exit 0"]
    STAYS_UP = ["sh", "-c", "sleep 30"]

    def _drain_until_dead(self, sub: audio.Subscriber) -> None:
        deadline = time.monotonic() + 5.0
        while sub.fileno() >= 0 and time.monotonic() < deadline:
            select.select([sub.fileno()], [], [], 0.2)
            sub.read_events()
        self.assertEqual(sub.fileno(), -1, "child never died")

    def _advance_to_respawn(self, sub: audio.Subscriber, now: float) -> float:
        """Walk the injected clock forward until the backoff lets it respawn.

        Guarded rather than a bare `while`: a supervision bug that never
        respawns should fail the test, not hang the suite.
        """
        limit = now + 10 * max(audio.RESPAWN_DELAYS) + audio.HEALTHY_AFTER
        while not sub.ensure_alive(now):
            now += 0.5
            self.assertLess(now, limit, "never respawned")
        return now

    def test_death_is_noticed_and_the_fd_is_dropped(self):
        """A dead child's pipe stays readable-at-EOF forever. Left registered,
        it would spin the event loop at 100% of a core."""
        with audio.Subscriber(self.DIES_AT_ONCE) as sub:
            self.assertTrue(sub.ensure_alive(0.0))
            self.assertGreaterEqual(sub.fileno(), 0)
            self._drain_until_dead(sub)
            self.assertEqual(sub.fileno(), -1)
            self.assertFalse(sub.alive)

    def test_a_child_that_dies_instantly_is_not_respawned_instantly(self):
        """The spin this guards against is subtle: the spawn succeeds, so the
        error path never runs, and only the immediate exit reveals the problem."""
        with audio.Subscriber(self.DIES_AT_ONCE) as sub:
            sub.ensure_alive(0.0)
            self._drain_until_dead(sub)
            self.assertFalse(sub.ensure_alive(0.1), "respawned with no delay")
            self.assertTrue(sub.ensure_alive(0.0 + audio.RESPAWN_DELAYS[0] + 0.2))

    def test_backoff_lengthens_across_repeated_failures(self):
        delays = []
        with audio.Subscriber(self.DIES_AT_ONCE) as sub:
            now = 0.0
            for _ in range(4):
                now = self._advance_to_respawn(sub, now)
                started = now
                self._drain_until_dead(sub)
                sub.ensure_alive(now)  # notices the death, applies the backoff
                now = self._advance_to_respawn(sub, now)
                delays.append(now - started)
        self.assertEqual(delays, sorted(delays), f"backoff did not grow: {delays}")
        self.assertGreater(delays[-1], delays[0])

    def test_a_child_that_ran_a_while_resets_the_backoff(self):
        """A PipeWire restart after a week of uptime is a fresh problem, not the
        fifth failure in a row. Without this the daemon would sit in a 30s
        backoff after a single unrelated blip."""
        with audio.Subscriber(self.DIES_AT_ONCE) as sub:
            now = 0.0
            for _ in range(3):  # build up some failures
                now = self._advance_to_respawn(sub, now)
                self._drain_until_dead(sub)
                sub.ensure_alive(now)
            # Now a child that lived a long time before dying.
            now = self._advance_to_respawn(sub, now)
            self._drain_until_dead(sub)
            now += audio.HEALTHY_AFTER + 1.0
            sub.ensure_alive(now)  # applies backoff, should have reset
            self.assertTrue(
                sub.ensure_alive(now + audio.RESPAWN_DELAYS[0] + 0.1),
                "backoff was not reset after a long-lived child",
            )

    def test_an_unspawnable_command_backs_off_instead_of_raising(self):
        with audio.Subscriber(["/nonexistent/pactl", "subscribe"]) as sub:
            self.assertFalse(sub.ensure_alive(0.0))
            self.assertEqual(sub.fileno(), -1)
            self.assertFalse(sub.ensure_alive(0.01))  # not hammered

    def test_close_is_idempotent(self):
        sub = audio.Subscriber(self.STAYS_UP)
        sub.ensure_alive(0.0)
        sub.close()
        sub.close()
        self.assertEqual(sub.fileno(), -1)


class TestSubscriberChangeDetection(unittest.TestCase):
    """Events mean 'look again', never 'the volume changed'."""

    def setUp(self):
        self._real_read = audio.read
        self.value = audio.Volume(0.61, False)
        audio.read = lambda: self.value
        self.addCleanup(setattr, audio, "read", self._real_read)

    def _subscriber(self, *lines: str) -> audio.Subscriber:
        script = "".join(f"printf '%s\\n' \"{line}\"; " for line in lines) + "sleep 30"
        sub = audio.Subscriber(["sh", "-c", script])
        self.addCleanup(sub.close)
        sub.ensure_alive(0.0)
        return sub

    def _wait(self, sub: audio.Subscriber) -> bool:
        deadline = time.monotonic() + 5.0
        while time.monotonic() < deadline:
            if select.select([sub.fileno()], [], [], 0.2)[0]:
                return sub.read_events()
        self.fail("the fake child produced nothing")

    def test_a_sink_event_with_an_unchanged_volume_is_not_a_change(self):
        """Measured: playing a 0.5s beep emits two 'change on sink' events with
        the volume untouched. Trusting the event would fire a full-panel gauge
        on every notification sound."""
        sub = self._subscriber("Event 'change' on sink #55")
        self.assertFalse(self._wait(sub))

    def test_a_sink_event_with_a_new_volume_is_a_change(self):
        sub = self._subscriber("Event 'change' on sink #55")
        self.value = audio.Volume(0.75, False)
        self.assertTrue(self._wait(sub))
        self.assertEqual(sub.current(), audio.Volume(0.75, False))

    def test_mute_alone_counts_as_a_change(self):
        """Mute is the one case where the number does not move but the state
        the takeover reports does."""
        sub = self._subscriber("Event 'change' on sink #55")
        self.value = audio.Volume(0.61, True)
        self.assertTrue(self._wait(sub))

    def test_the_value_at_spawn_is_a_baseline_not_a_change(self):
        """Otherwise the first event after a PipeWire restart compares against a
        stale pre-restart value and pops a takeover nobody asked for."""
        sub = self._subscriber("Event 'change' on sink #55")
        self.assertEqual(sub.current(), audio.Volume(0.61, False))
        self.assertFalse(self._wait(sub))

    def test_uninteresting_events_do_not_even_re_read(self):
        reads = []
        audio.read = lambda: (reads.append(1), self.value)[1]
        sub = self._subscriber("Event 'new' on client #1")
        before = len(reads)
        self.assertFalse(self._wait(sub))
        self.assertEqual(len(reads), before, "re-read on a client event")


class TestClaudeSessionParsing(unittest.TestCase):
    # Captured from a live status line payload, trimmed to the parts read here.
    LIVE_PAYLOAD = {
        "session_id": "17c4504f-de07-4d00-8bcb-1ebe58fd6093",
        "model": {"id": "claude-opus-5[1m]", "display_name": "Opus 5 (1M context)"},
        "context_window": {
            "total_input_tokens": 105420,
            "context_window_size": 1000000,
            "used_percentage": 11,
            "remaining_percentage": 89,
        },
        "rate_limits": {
            "five_hour": {"used_percentage": 5, "resets_at": 1786180200},
            "seven_day": {"used_percentage": 5, "resets_at": 1786460400},
        },
    }

    def test_extracts_all_three_percentages(self):
        self.assertEqual(claude_session.parse(self.LIVE_PAYLOAD), (11.0, 5.0, 5.0))

    def test_reads_used_not_remaining(self):
        """The payload carries both, and they are complements. Reading the wrong
        one shows a bar that empties as the context fills."""
        context, _, _ = claude_session.parse(self.LIVE_PAYLOAD)
        self.assertEqual(context, 11.0)
        self.assertNotEqual(context, 89.0)

    def test_missing_sections_yield_none_not_zero(self):
        """Zero would render an empty bar, which reads as 'plenty of context
        left' — the opposite of 'no idea'. Absence must stay absent."""
        self.assertEqual(claude_session.parse({}), (None, None, None))
        self.assertEqual(claude_session.parse({"context_window": {}}), (None, None, None))

    def test_hostile_shapes_never_raise(self):
        """This is an undocumented internal format that moves with releases, and
        it feeds a daemon also driving the battery indicator."""
        for payload in (
            None, [], "", 0,
            {"context_window": None},
            {"context_window": "11%"},
            {"context_window": {"used_percentage": "eleven"}},
            {"context_window": {"used_percentage": None}},
            {"rate_limits": [1, 2, 3]},
            {"rate_limits": {"five_hour": "nope"}},
        ):
            self.assertEqual(len(claude_session.parse(payload)), 3, payload)

    def test_booleans_are_not_percentages(self):
        """True is an int in Python and would silently render as 1%."""
        self.assertIsNone(claude_session.parse({"context_window": {"used_percentage": True}})[0])

    def test_nonsense_numbers_are_rejected_or_clamped(self):
        nan = float("nan")
        self.assertIsNone(claude_session.parse({"context_window": {"used_percentage": nan}})[0])
        inf = float("inf")
        self.assertIsNone(claude_session.parse({"context_window": {"used_percentage": inf}})[0])
        self.assertEqual(claude_session.parse({"context_window": {"used_percentage": 140}})[0], 100.0)
        self.assertEqual(claude_session.parse({"context_window": {"used_percentage": -5}})[0], 0.0)


class TestClaudeSessionReading(unittest.TestCase):
    """The daemon and the status line shim meet at a directory layout, so the
    layout is what gets tested — not a mock of one half talking to the other."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        patcher = unittest.mock.patch.dict(
            os.environ, {"XDG_RUNTIME_DIR": self.tmp.name}
        )
        patcher.start()
        self.addCleanup(patcher.stop)
        self.dir = os.path.join(self.tmp.name, claude_session.RUNTIME_SUBDIR)
        os.makedirs(self.dir, exist_ok=True)

    def _write(self, session_id, context_pct, *, age=0.0, state=None):
        path = os.path.join(self.dir, f"{session_id}.json")
        with open(path, "w") as f:
            json.dump({"context_window": {"used_percentage": context_pct}}, f)
        if state is not None:
            with open(os.path.join(self.dir, f"{session_id}.state"), "w") as f:
                f.write(state)
        if age:
            when = time.time() - age
            os.utime(path, (when, when))
        return path

    def test_no_sessions_means_no_session(self):
        self.assertIsNone(claude_session.read())

    def test_reads_a_single_session(self):
        self._write("aaa", 42)
        session = claude_session.read()
        self.assertEqual(session.session_id, "aaa")
        self.assertEqual(session.context_pct, 42.0)

    def test_picks_the_most_recently_active_of_several(self):
        """With several sessions open, the one whose status line rendered last
        is the one being looked at — start order is irrelevant."""
        self._write("old", 10, age=600)
        self._write("newest", 77)
        self._write("middle", 30, age=60)
        self.assertEqual(claude_session.read().session_id, "newest")

    def test_a_stale_snapshot_is_not_a_running_session(self):
        """A session killed hard enough to skip SessionEnd must not hold a
        frozen percentage on the panel forever."""
        self._write("dead", 90, age=claude_session.STALE_AFTER + 60)
        self.assertIsNone(claude_session.read())

    def test_an_idle_session_is_still_a_session(self):
        """Left open over lunch is not the same as gone, and its context
        percentage is still true.

        Deliberately an absolute duration rather than a fraction of
        STALE_AFTER: a test written relative to the constant passes for every
        value of it, including one hopelessly too small, and so tests nothing
        about the threshold it exists to pin.
        """
        self._write("lunch", 55, age=45 * 60)
        self.assertIsNotNone(claude_session.read(), "45 minutes idle is not dead")

    def test_a_session_untouched_all_day_is_gone(self):
        """The other side of the same threshold, also pinned absolutely."""
        self._write("yesterday", 55, age=24 * 3600)
        self.assertIsNone(claude_session.read())

    def test_working_comes_from_the_state_file(self):
        self._write("aaa", 42, state="working")
        self.assertTrue(claude_session.read().working)

    def test_idle_and_absent_state_both_read_as_not_working(self):
        """A stuck-on working indicator never resolves, so absence must default
        to off rather than to last-known."""
        self._write("aaa", 42, state="idle")
        self.assertFalse(claude_session.read().working)
        os.remove(os.path.join(self.dir, "aaa.state"))
        self.assertFalse(claude_session.read().working)

    def test_a_half_written_snapshot_is_ignored_not_fatal(self):
        """The shim writes to a temp name and renames, so this should not happen
        — but the reader is not the right place to find out it did."""
        with open(os.path.join(self.dir, "torn.json"), "w") as f:
            f.write('{"context_window": {"used_per')
        self.assertIsNone(claude_session.read())

    def test_missing_runtime_dir_is_not_an_error(self):
        """A daemon started outside a user session has no XDG_RUNTIME_DIR."""
        with unittest.mock.patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(claude_session.snapshot_dir())
            self.assertIsNone(claude_session.read())
            self.assertEqual(claude_session.prune(), 0)

    def test_prune_removes_dead_sessions_and_their_state(self):
        self._write("dead", 90, age=claude_session.STALE_AFTER + 60, state="working")
        self._write("alive", 20, state="idle")
        self.assertEqual(claude_session.prune(), 1)
        self.assertFalse(os.path.exists(os.path.join(self.dir, "dead.json")))
        self.assertFalse(
            os.path.exists(os.path.join(self.dir, "dead.state")),
            "orphaned state file left behind",
        )
        self.assertTrue(os.path.exists(os.path.join(self.dir, "alive.json")))


if __name__ == "__main__":
    unittest.main()
