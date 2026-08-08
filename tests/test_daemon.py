"""Tests for the event loop's decisions.

What is worth testing here is not "does epoll work" but the judgements the loop
makes: when a deadline is due, when a takeover is allowed, when a frame is worth
the 169ms it costs to send. Those are the places a bug is silent — a daemon that
spins at 100% of a core or never redraws still looks like it is running.

The loop is driven a step at a time rather than started, so nothing here needs
hardware, a timer, or a sleep.
"""

import time
import unittest
import unittest.mock

from matrixd import daemon, render, transport
from matrixd.sources import audio, claude_session, power, usage


class FakePanel:
    """Stands in for transport.Panel, recording what it was asked to do."""

    def __init__(self, side="left", *, dies=False):
        self.side = side
        self.path = f"/dev/fake-{side}"
        self.connected = True
        self.brightness = None
        self.frames = []
        self.fast_flags = []
        self.slept = False
        self.woke = False
        self.keepalives = 0
        self._dies = dies
        self._next_keepalive = time.monotonic() + transport.KEEPALIVE_INTERVAL

    def _maybe_die(self):
        if self._dies:
            self.connected = False
            raise transport.PanelGone(f"{self.side}: gone")

    def set_brightness(self, value):
        self._maybe_die()
        self.brightness = value

    def draw(self, frame, *, fast=False):
        self._maybe_die()
        self.frames.append(frame)
        self.fast_flags.append(fast)

    def sleep(self):
        self._maybe_die()
        self.slept = True

    def wake(self):
        self._maybe_die()
        self.woke = True

    def begin_wake(self):
        self._maybe_die()
        self.woke = True

    def keepalive(self):
        self._maybe_die()
        self.keepalives += 1

    def next_keepalive_at(self):
        return self._next_keepalive

    def keepalive_due(self, now=None):
        return (time.monotonic() if now is None else now) >= self._next_keepalive

    def drain(self):
        pass

    def close(self):
        self.connected = False


def make_daemon(**state):
    """A daemon with no real sources attached."""
    with unittest.mock.patch.object(audio, "read", return_value=None):
        d = daemon.Daemon()
    for key, value in state.items():
        setattr(d.state, key, value)
    return d


class TestPulse(unittest.TestCase):
    def test_breathes_between_one_and_four_at_the_floor(self):
        """The documented shape. At base 1 there is no room to modulate
        downward without switching the panel off, so the pulse is biased up."""
        values = [daemon.pulse_brightness(1, p / 30) for p in range(31)]
        self.assertEqual(min(values), 1)
        self.assertEqual(max(values), 4)

    def test_never_goes_dark(self):
        """A pulse that reached 0 would read as the panel dying, not charging."""
        for base in range(0, 256):
            for step in range(20):
                self.assertGreaterEqual(daemon.pulse_brightness(base, step / 20), 1)

    def test_never_exceeds_the_hardware_range(self):
        for base in (0, 1, 128, 250, 255, 300):
            for step in range(20):
                self.assertLessEqual(daemon.pulse_brightness(base, step / 20), 255)

    def test_amplitude_scales_with_base(self):
        """So the pulse reads as similar at every brightness rather than
        vanishing at the top and swamping the panel at the bottom."""
        def swing(base):
            v = [daemon.pulse_brightness(base, s / 20) for s in range(20)]
            return max(v) - min(v)

        self.assertGreater(swing(100), swing(10))

    def test_is_continuous_across_the_wrap(self):
        """A discontinuity at phase 1.0 would show as a visible hitch once every
        three seconds — which reads as a glitch, not a breath."""
        self.assertEqual(
            daemon.pulse_brightness(100, 0.0), daemon.pulse_brightness(100, 1.0)
        )
        near_end = daemon.pulse_brightness(100, 0.99)
        self.assertLess(abs(near_end - daemon.pulse_brightness(100, 0.0)), 5)

    def test_peaks_mid_phase(self):
        self.assertEqual(
            daemon.pulse_brightness(100, 0.5), max(
                daemon.pulse_brightness(100, s / 50) for s in range(50)
            )
        )


class TestDeadlines(unittest.TestCase):
    """A deadline stuck in the past makes every poll return immediately, which
    is a busy loop that looks exactly like a working daemon.

    A freshly built daemon is *legitimately* overdue — it has not ticked yet —
    so the invariant these assert is not "the deadline is always in the future"
    but "handling what is due pushes it into the future". That distinction is
    the whole difference between a loop that runs once and one that spins.
    """

    def settle(self, d, rounds=3):
        """Run the due work, asserting the loop would sleep afterwards."""
        with unittest.mock.patch.object(daemon.usage, "fetch", return_value=None), \
             unittest.mock.patch.object(daemon.transport, "discover", return_value={}):
            for _ in range(rounds):
                d._run_due()
                self.assertGreater(
                    d._next_deadline(),
                    time.monotonic(),
                    "deadline never advanced — the loop would spin",
                )

    def test_idle_daemon_sleeps_rather_than_spinning(self):
        d = make_daemon()
        d.panels = {"left": FakePanel("left"), "right": FakePanel("right")}
        self.settle(d)

    def test_a_daemon_with_no_panels_retries_without_spinning(self):
        """Retrying immediately on the first pass is correct; doing it on every
        pass is a busy loop."""
        d = make_daemon()
        self.assertTrue(d._panels_incomplete())
        self.settle(d)

    def test_a_charging_daemon_does_not_spin_either(self):
        """The pulse is a 10Hz deadline, which is the one cadence fast enough
        that a mistake in it would look like an idle loop."""
        d = make_daemon(power=power.Power(50.0, True, True))
        d.panels = {"left": FakePanel("left")}
        self.settle(d)

    def test_the_pulse_deadline_is_ignored_when_not_charging(self):
        """The pulse deadline is only advanced while charging, so leaving it in
        the calculation would pin the timeout at zero forever the moment the
        charger came out."""
        d = make_daemon(power=power.Power(50.0, False, False))
        d.panels = {"left": FakePanel("left")}
        self.settle(d)
        d._due["pulse"] = time.monotonic() - 3600  # long past
        self.assertGreater(d._next_deadline(), time.monotonic())

    def test_the_pulse_deadline_is_honoured_when_charging(self):
        d = make_daemon(power=power.Power(50.0, True, True))
        d.panels = {"left": FakePanel("left")}
        soon = time.monotonic() + 0.05
        d._due["pulse"] = soon
        self.assertLessEqual(d._next_deadline(), soon)

    def test_a_pending_takeover_wakes_the_loop_to_clear_it(self):
        """Without this the gauge lingers past its two seconds, until whatever
        unrelated wakeup happens next.

        The daemon is settled first and the expiry deliberately placed sooner
        than every other deadline. Skip that and the already-overdue startup
        tick wins the `min()`, and the assertion passes whether or not takeover
        expiries are considered at all.
        """
        d = make_daemon()
        d.panels = {"left": FakePanel("left")}
        self.settle(d)
        floor = d._next_deadline()
        expiry = time.monotonic() + 0.05
        self.assertLess(expiry, floor, "test setup: expiry must be the soonest deadline")
        d.takeovers["left"] = daemon.Takeover(render.blank(), expiry)
        self.assertAlmostEqual(d._next_deadline(), expiry, places=6)

    def test_missing_panels_are_retried_even_when_none_were_ever_found(self):
        """`any(...)` over an empty dict is False, so a bare check would never
        retry in exactly the case that needs it most — nothing discovered."""
        d = make_daemon()
        self.assertEqual(d.panels, {})
        self.assertTrue(d._panels_incomplete())

    def test_a_disconnected_panel_counts_as_incomplete(self):
        d = make_daemon()
        panel = FakePanel("left")
        panel.connected = False
        d.panels["left"] = panel
        self.assertTrue(d._panels_incomplete())

    def test_all_connected_panels_are_complete(self):
        d = make_daemon()
        d.panels["left"] = FakePanel("left")
        self.assertFalse(d._panels_incomplete())


class TestTakeovers(unittest.TestCase):
    def test_the_right_panel_never_takes_over(self):
        """It carries nothing you can change, so a takeover there could only be
        an interruption."""
        d = make_daemon()
        d.take_over("right", 0.5)
        self.assertNotIn("right", d.takeovers)

    def test_the_left_panel_does(self):
        d = make_daemon()
        d.take_over("left", 0.5)
        self.assertIn("left", d.takeovers)

    def test_a_new_event_replaces_rather_than_queues(self):
        """Six volume taps are one thing whose value changed six times. A queue
        would spend twelve seconds showing values that are no longer true."""
        d = make_daemon()
        d.take_over("left", 0.2)
        first = d.takeovers["left"]
        d.take_over("left", 0.8)
        second = d.takeovers["left"]
        self.assertEqual(len(d.takeovers), 1)
        self.assertIsNot(first.frame, second.frame)
        self.assertGreaterEqual(second.expires_at, first.expires_at)

    def test_the_timer_runs_from_the_last_event_not_the_first(self):
        """So holding a volume key keeps the bar up continuously and it lingers
        exactly TAKEOVER_SECONDS after release."""
        d = make_daemon()
        d.take_over("left", 0.2)
        first = d.takeovers["left"].expires_at
        time.sleep(0.01)
        d.take_over("left", 0.3)
        self.assertGreater(d.takeovers["left"].expires_at, first)

    def test_no_takeover_while_the_screen_is_off(self):
        """Lighting a panel because hypridle changed the brightness of an
        already-blank screen is the exact behaviour the panels are meant to
        follow, not fight."""
        d = make_daemon(screen_off=True)
        d.take_over("left", 0.5)
        self.assertEqual(d.takeovers, {})

    def test_an_unknown_value_is_not_a_takeover(self):
        """None means "could not read", which is not a value worth showing."""
        d = make_daemon()
        d.take_over("left", None)
        self.assertEqual(d.takeovers, {})

    def test_muting_shows_an_empty_gauge_not_the_level_behind_it(self):
        d = make_daemon()
        d.volume.last = audio.Volume(0.8, True)
        d._on_volume_changed()
        self.assertEqual(d.takeovers["left"].frame, render.render_gauge(0.0))

    def test_unmuted_shows_the_level(self):
        d = make_daemon()
        d.volume.last = audio.Volume(0.8, False)
        d._on_volume_changed()
        self.assertEqual(d.takeovers["left"].frame, render.render_gauge(0.8))

    def test_expired_takeovers_are_cleared(self):
        d = make_daemon()
        d.takeovers["left"] = daemon.Takeover(render.blank(), time.monotonic() - 1)
        d._run_due()
        self.assertNotIn("left", d.takeovers)


class TestPowerEdges(unittest.TestCase):
    def test_plugging_in_takes_over(self):
        d = make_daemon(power=power.Power(50.0, False, False))
        with unittest.mock.patch.object(
            power, "read", return_value=power.Power(50.0, True, True)
        ):
            d._on_power_event()
        self.assertIn("left", d.takeovers)

    def test_a_battery_percentage_change_alone_does_not(self):
        """These events fire constantly as the charge moves. Taking over for
        each would put a gauge on the panel every percentage point."""
        d = make_daemon(power=power.Power(50.0, False, False))
        with unittest.mock.patch.object(
            power, "read", return_value=power.Power(49.0, False, False)
        ):
            d._on_power_event()
        self.assertEqual(d.takeovers, {})
        self.assertEqual(d.state.power.battery_pct, 49.0, "state still updated")


class TestBrightness(unittest.TestCase):
    def test_a_deliberate_change_takes_over(self):
        d = make_daemon()
        with unittest.mock.patch.object(daemon.screen, "read_fraction", return_value=0.5), \
             unittest.mock.patch.object(daemon.screen, "changed_automatically", return_value=False):
            d._on_brightness_event()
        self.assertIn("left", d.takeovers)

    def test_an_automatic_change_updates_the_level_but_does_not_take_over(self):
        """hypridle dimming as you walk away must not light the panel up."""
        d = make_daemon()
        with unittest.mock.patch.object(daemon.screen, "read_fraction", return_value=0.1), \
             unittest.mock.patch.object(daemon.screen, "changed_automatically", return_value=True):
            d._on_brightness_event()
        self.assertEqual(d.takeovers, {})
        self.assertEqual(d.state.screen_fraction, 0.1, "brightness still tracked")

    def test_the_gauge_uses_its_full_height(self):
        """The keybinds floor the screen at 2%, so a raw fraction could never
        show an empty bar and the bottom of the scale would be dead."""
        d = make_daemon(screen_fraction=daemon.screen.SCREEN_FLOOR_FRACTION)
        self.assertAlmostEqual(d._fraction_of_screen(), 0.0)
        d.state.screen_fraction = 1.0
        self.assertAlmostEqual(d._fraction_of_screen(), 1.0)


class TestPainting(unittest.TestCase):
    def test_an_unchanged_frame_is_not_resent(self):
        """A greyscale frame costs 169ms of link time and the clock moves once a
        minute. Redrawing every tick would hold the link at ~17% duty for
        nothing and delay takeovers behind an in-flight frame."""
        d = make_daemon()
        d.panels = {"left": FakePanel("left"), "right": FakePanel("right")}
        d.paint()
        first = len(d.panels["left"].frames)
        d.paint()
        self.assertEqual(len(d.panels["left"].frames), first, "redrew an identical frame")

    def test_a_changed_frame_is_sent(self):
        d = make_daemon()
        d.panels = {"left": FakePanel("left"), "right": FakePanel("right")}
        d.paint()
        before = len(d.panels["left"].frames)
        d.state.power = power.Power(5.0, False, False)  # battery bar moves
        d.paint()
        self.assertGreater(len(d.panels["left"].frames), before)

    def test_takeovers_use_the_fast_path(self):
        """25ms versus 169ms, on the one thing here with a latency budget you
        can actually perceive."""
        d = make_daemon()
        d.panels = {"left": FakePanel("left")}
        d.take_over("left", 0.5)
        d.paint()
        self.assertTrue(d.panels["left"].fast_flags[-1])

    def test_ambient_frames_do_not(self):
        """They carry per-pixel intensity that the 1-bit path would discard."""
        d = make_daemon()
        d.panels = {"left": FakePanel("left")}
        d.paint()
        self.assertFalse(d.panels["left"].fast_flags[-1])

    def test_screen_off_sleeps_the_panels(self):
        d = make_daemon(screen_off=True)
        panel = FakePanel("left")
        d.panels = {"left": panel}
        d.paint()
        self.assertTrue(panel.slept)
        self.assertEqual(panel.frames, [], "drew to a panel it just put to sleep")

    def test_sleeping_happens_once_not_every_tick(self):
        d = make_daemon(screen_off=True)
        panel = FakePanel("left")
        d.panels = {"left": panel}
        d.paint()
        panel.slept = False
        d.paint()
        self.assertFalse(panel.slept, "re-slept an already sleeping panel")

    def test_the_screen_coming_back_wakes_and_repaints(self):
        d = make_daemon(screen_off=True)
        panel = FakePanel("left")
        d.panels = {"left": panel}
        d.paint()
        d.state.screen_off = False
        with unittest.mock.patch.object(daemon.time, "sleep"):  # skip the fade
            d.paint()
        self.assertTrue(panel.woke)
        self.assertTrue(panel.frames, "woke but never repainted")

    def test_both_panels_wake_before_either_settles(self):
        """The settle is the LEDs fading, and the two panels fade in parallel.
        Waiting once each would block the loop for two seconds at the moment
        you touch the machine."""
        d = make_daemon(screen_off=True)
        d.panels = {"left": FakePanel("left"), "right": FakePanel("right")}
        d.paint()
        d.state.screen_off = False
        sleeps = []
        with unittest.mock.patch.object(daemon.time, "sleep", sleeps.append):
            d.paint()
        self.assertTrue(all(p.woke for p in d.panels.values()))
        self.assertEqual(len(sleeps), 1, f"settled once per panel: {sleeps}")

    def test_a_panel_that_dies_mid_paint_is_not_recorded_as_painted(self):
        """Otherwise the reconnected panel — which comes back blank — would be
        compared against a frame it never received and left dark."""
        d = make_daemon()
        d.panels = {"left": FakePanel("left", dies=True)}
        d.paint()
        self.assertNotIn("left", d.painted)

    def test_a_dead_panel_schedules_a_retry(self):
        d = make_daemon()
        d.panels = {"left": FakePanel("left", dies=True)}
        d._panel_retry_at = 0.0
        d.paint()
        self.assertGreater(d._panel_retry_at, time.monotonic())


class TestAmbientContent(unittest.TestCase):
    def _usage(self, five, seven, fetched_at=None):
        return usage.Usage(
            five_hour=usage.Window(five, None),
            seven_day=usage.Window(seven, None),
            fetched_at=time.monotonic() if fetched_at is None else fetched_at,
        )

    def test_stale_rate_limits_are_dropped_not_shown(self):
        """A bar that stopped updating hours ago looks current, which is worse
        than an empty zone on a surface whose whole point is a trusted glance."""
        d = make_daemon(usage=self._usage(80.0, 40.0, fetched_at=time.monotonic() - usage.STALE_AFTER - 60))
        fresh = make_daemon(usage=self._usage(80.0, 40.0))
        self.assertNotEqual(
            d.ambient_frames()["right"], fresh.ambient_frames()["right"]
        )
        self.assertEqual(d.ambient_frames()["right"], make_daemon().ambient_frames()["right"])

    def test_the_session_payload_backs_up_an_unreachable_endpoint(self):
        """The status line carries the same numbers, but only while a session is
        open — which is why it is the fallback rather than the source."""
        session = claude_session.Session(
            "s", context_pct=10.0, working=False, updated_at=0.0,
            five_hour_pct=55.0, seven_day_pct=20.0,
        )
        with_session = make_daemon(usage=None, session=session).ambient_frames()["right"]
        without = make_daemon(usage=None, session=None).ambient_frames()["right"]
        self.assertNotEqual(with_session, without)

    def test_the_endpoint_wins_over_the_session_payload(self):
        session = claude_session.Session(
            "s", context_pct=10.0, working=False, updated_at=0.0,
            five_hour_pct=5.0, seven_day_pct=5.0,
        )
        d = make_daemon(usage=self._usage(90.0, 90.0), session=session)
        endpoint_only = make_daemon(usage=self._usage(90.0, 90.0), session=None)
        left_zone = slice(*render.FIVE_HOUR_Y)
        self.assertEqual(
            d.ambient_frames()["right"][left_zone],
            endpoint_only.ambient_frames()["right"][left_zone],
        )

    def test_no_session_leaves_the_context_zone_dark(self):
        """Rather than the other zones reflowing to fill it — a layout that
        moves turns a glance into a lookup."""
        frame = make_daemon(session=None).ambient_frames()["right"]
        y0, y1 = render.CONTEXT_Y
        for row in frame[y0 : y1 + 1]:
            self.assertEqual(set(row), {render.OFF})

    def test_working_lights_the_activity_rule(self):
        idle = claude_session.Session("s", 10.0, False, 0.0)
        busy = claude_session.Session("s", 10.0, True, 0.0)
        y = render.RIGHT_RULE_2_Y
        self.assertLess(
            make_daemon(session=idle).ambient_frames()["right"][y][0],
            make_daemon(session=busy).ambient_frames()["right"][y][0],
        )


class TestBrightnessTargets(unittest.TestCase):
    def test_both_panels_track_the_screen_when_not_charging(self):
        d = make_daemon(screen_fraction=1.0, power=power.Power(50.0, False, False))
        self.assertEqual(d.target_brightness("left"), render.AMBIENT_CEILING)
        self.assertEqual(d.target_brightness("right"), render.AMBIENT_CEILING)

    def test_only_the_left_panel_pulses(self):
        """Charging is a machine state, and the machine panel is the left one."""
        d = make_daemon(screen_fraction=0.5, power=power.Power(50.0, True, True))
        base = d.target_brightness("right")
        d._pulse_phase = 0.5  # peak
        self.assertGreater(d.target_brightness("left"), base)
        self.assertEqual(d.target_brightness("right"), base)

    def test_an_unreadable_screen_falls_back_to_the_floor(self):
        d = make_daemon(screen_fraction=None)
        self.assertEqual(d.target_brightness("right"), render.AMBIENT_FLOOR)


if __name__ == "__main__":
    unittest.main()
