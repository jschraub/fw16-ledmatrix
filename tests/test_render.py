"""Tests for the pure render layer. No hardware required.

The encoders are tested against the conventions measured on real modules — if
any of these fail, the panels will render mirrored, transposed, or upside down
rather than merely wrong.
"""

import unittest

from matrixd import render as r


class TestGeometry(unittest.TestCase):
    def test_frame_shape(self):
        f = r.blank()
        self.assertEqual(len(f), 34)
        self.assertTrue(all(len(row) == 9 for row in f))
        self.assertTrue(all(v == 0 for row in f for v in row))

    def test_both_layouts_fill_exactly_34_rows(self):
        """Every row is accounted for, with no overlap and no gaps beyond the
        one deliberate spacer between the hour and minute digits."""
        left = (
            set(range(r.HOUR_Y, r.HOUR_Y + 5))
            | {5}  # deliberate gap
            | set(range(r.MIN_Y, r.MIN_Y + 5))
            | {r.LEFT_RULE_Y}
            | set(range(r.BATTERY_Y[0], r.BATTERY_Y[1] + 1))
        )
        self.assertEqual(left, set(range(34)))

        right = (
            set(range(r.FIVE_HOUR_Y[0], r.FIVE_HOUR_Y[1] + 1))
            | {r.RIGHT_RULE_1_Y}
            | set(range(r.SEVEN_DAY_Y[0], r.SEVEN_DAY_Y[1] + 1))
            | {r.RIGHT_RULE_2_Y}
            | set(range(r.CONTEXT_Y[0], r.CONTEXT_Y[1] + 1))
        )
        self.assertEqual(right, set(range(34)))

    def test_digits_fit_the_width(self):
        self.assertEqual(r._DIGIT_X[1] + r.DIGIT_W, 8)  # 1px right margin
        self.assertEqual(r._DIGIT_X[0], 1)  # 1px left margin


class TestBars(unittest.TestCase):
    def test_empty_and_full(self):
        f = r.blank()
        r.draw_bar(f, 0, 33, 0.0)
        self.assertTrue(all(v == 0 for row in f for v in row))

        f = r.blank()
        r.draw_bar(f, 0, 33, 1.0)
        for y in range(34):
            self.assertTrue(all(f[y][x] == r.DATA for x in range(1, 8)))

    def test_fills_upward_from_the_bottom(self):
        """y=33 is the near end. A quarter-full bar lights the near end, not
        the far one — get this backwards and every gauge reads inverted."""
        f = r.blank()
        r.draw_bar(f, 0, 33, 0.25)
        self.assertGreater(f[33][4], 0)
        self.assertEqual(f[0][4], 0)

    def test_partial_tip_gives_sub_level_resolution(self):
        """A bar between two whole rows renders the topmost row dimmer, which
        is the whole reason the ambient frame is greyscale."""
        f = r.blank()
        r.draw_bar(f, 0, 9, 0.55)  # 10 rows -> 5.5
        self.assertEqual(f[9][4], r.DATA)  # bottom row full
        self.assertEqual(f[5][4], r.DATA)  # 5th row full
        self.assertGreater(f[4][4], 0)  # 6th row partial
        self.assertLess(f[4][4], r.DATA)
        self.assertEqual(f[3][4], 0)  # 7th row dark

    def test_tiny_fraction_still_shows_something(self):
        f = r.blank()
        r.draw_bar(f, 0, 33, 0.001)
        self.assertGreater(f[33][4], 0)

    def test_inset_leaves_margins(self):
        f = r.blank()
        r.draw_bar(f, 0, 33, 1.0)
        self.assertEqual(f[33][0], 0)
        self.assertEqual(f[33][8], 0)

    def test_fraction_is_clamped(self):
        for bad in (-1.0, 5.0):
            f = r.blank()
            r.draw_bar(f, 0, 33, bad)  # must not raise or overflow the zone
            self.assertEqual(len(f), 34)


class TestDigits(unittest.TestCase):
    def test_zero_padded(self):
        a, b = r.blank(), r.blank()
        r.draw_two_digits(a, 0, 7)
        r.draw_two_digits(b, 0, 7)
        self.assertEqual(a, b)
        self.assertGreater(sum(a[0]), 0)  # leading zero is drawn, not blank

    def test_clamped_not_truncated(self):
        """123 must not render as "23" — a confidently wrong reading is worse
        than a pinned one."""
        got, want = r.blank(), r.blank()
        r.draw_two_digits(got, 0, 123)
        r.draw_two_digits(want, 0, 99)
        self.assertEqual(got, want)

    def test_all_glyphs_fit_their_box(self):
        for d, glyph in r._FONT.items():
            self.assertEqual(len(glyph), r.DIGIT_H, f"digit {d} height")
            for row in glyph:
                self.assertEqual(len(row), r.DIGIT_W, f"digit {d} width")

    def test_glyphs_are_distinguishable(self):
        seen = {}
        for d, glyph in r._FONT.items():
            key = "".join(glyph)
            self.assertNotIn(key, seen, f"digits {d} and {seen.get(key)} identical")
            seen[key] = d


class TestVisibility(unittest.TestCase):
    """Encodes the measured threshold model so the intent survives refactoring."""

    def test_threshold_matches_hardware_measurements(self):
        # digits (greyscale 200): legible at global 3, not at 2
        self.assertTrue(r.is_visible(3, r.DATA))
        self.assertFalse(r.is_visible(2, r.DATA))
        # rules (greyscale 60): legible at global 9, not at 8
        self.assertTrue(r.is_visible(9, r.RULE))
        self.assertFalse(r.is_visible(8, r.RULE))

    def test_rules_vanish_at_the_floor(self):
        """DELIBERATE. Rules are invisible at the bottom of the range.

        A rule that cleared the threshold at AMBIENT_FLOOR would need greyscale
        near DATA, at which point it competes with the data it separates. Losing
        decoration is the right trade — band positions are fixed and learned.

        If this test fails because someone raised RULE, that person should read
        the comment block in render.py before changing it back.
        """
        self.assertFalse(r.is_visible(r.AMBIENT_FLOOR, r.RULE))

    def test_data_and_emphasis_survive_the_floor(self):
        """The corollary: everything that carries meaning must stay legible."""
        self.assertTrue(r.is_visible(r.AMBIENT_FLOOR, r.DATA))
        self.assertTrue(r.is_visible(r.AMBIENT_FLOOR, r.EMPHASIS))

    def test_activity_indicator_works_at_the_floor(self):
        """Its lit state must read even when its dim state cannot."""
        busy = r.render_claude(r.ClaudeState(working=True))
        idle = r.render_claude(r.ClaudeState(working=False))
        self.assertTrue(r.is_visible(r.AMBIENT_FLOOR, busy[r.RIGHT_RULE_2_Y][0]))
        self.assertFalse(r.is_visible(r.AMBIENT_FLOOR, idle[r.RIGHT_RULE_2_Y][0]))

    def test_everything_is_visible_at_the_ceiling(self):
        for value in (r.RULE, r.DATA, r.EMPHASIS):
            self.assertTrue(r.is_visible(r.AMBIENT_CEILING, value))


class TestEncoders(unittest.TestCase):
    def test_drawbw_is_row_major_lsb_first(self):
        """Measured on hardware: byte0=0x07 lit the three leftmost LEDs of the
        top row. Pixel (0,0) must therefore be bit 0 of byte 0."""
        f = r.blank()
        f[0][0] = f[0][1] = f[0][2] = 255
        self.assertEqual(r.to_drawbw(f)[0], 0x07)

    def test_drawbw_second_row_starts_at_bit_9(self):
        f = r.blank()
        f[1][0] = 255  # pixel index 9 -> byte 1, bit 1
        out = r.to_drawbw(f)
        self.assertEqual(out[0], 0x00)
        self.assertEqual(out[1], 0x02)

    def test_drawbw_length_and_threshold(self):
        f = r.blank()
        self.assertEqual(len(r.to_drawbw(f)), 39)
        f[0][0] = 127
        self.assertEqual(r.to_drawbw(f)[0], 0x00)  # below threshold
        f[0][0] = 128
        self.assertEqual(r.to_drawbw(f)[0], 0x01)

    def test_columns_shape_and_order(self):
        f = r.blank()
        f[0][3] = 11
        f[33][3] = 22
        cols = r.to_columns(f)
        self.assertEqual(len(cols), 9)
        self.assertTrue(all(len(c) == 35 for c in cols))
        self.assertEqual(cols[3][0], 3)  # column index leads the payload
        self.assertEqual(cols[3][1], 11)  # y=0 first
        self.assertEqual(cols[3][34], 22)  # y=33 last


class TestLayouts(unittest.TestCase):
    def test_machine_renders_time_and_battery(self):
        f = r.render_machine(r.MachineState(hour=5, minute=47, battery_pct=74))
        self.assertGreater(sum(sum(row) for row in f[0:5]), 0)  # hour
        self.assertGreater(sum(sum(row) for row in f[6:11]), 0)  # minute
        self.assertEqual(f[5], [0] * 9)  # spacer stays clear
        self.assertTrue(all(v == r.RULE for v in f[r.LEFT_RULE_Y]))
        self.assertGreater(f[33][4], 0)  # battery fills from the near end

    def test_claude_context_absent_leaves_the_zone_dark(self):
        """No live session must leave the band dark — never reflow the others
        into it."""
        f = r.render_claude(r.ClaudeState(five_hour_pct=50, seven_day_pct=10))
        for y in range(r.CONTEXT_Y[0], r.CONTEXT_Y[1] + 1):
            self.assertEqual(f[y], [0] * 9, f"row {y} should be dark")

    def test_activity_rule_lights_when_working(self):
        idle = r.render_claude(r.ClaudeState(working=False))
        busy = r.render_claude(r.ClaudeState(working=True))
        self.assertEqual(idle[r.RIGHT_RULE_2_Y][0], r.RULE)
        self.assertEqual(busy[r.RIGHT_RULE_2_Y][0], r.EMPHASIS)
        # and it must not disturb the data bands
        self.assertEqual(idle[r.RIGHT_RULE_1_Y], busy[r.RIGHT_RULE_1_Y])

    def test_severity_emphasises_only_its_own_band(self):
        plain = r.render_claude(r.ClaudeState(five_hour_pct=90, seven_day_pct=90))
        hot = r.render_claude(
            r.ClaudeState(
                five_hour_pct=90, seven_day_pct=90, five_hour_severity="critical"
            )
        )
        # Row 12 is the bottom of the 5h band, lit at any non-zero fraction.
        self.assertEqual(plain[r.FIVE_HOUR_Y[1]][4], r.DATA)
        self.assertEqual(hot[r.FIVE_HOUR_Y[1]][4], r.EMPHASIS)
        for y in range(r.SEVEN_DAY_Y[0], r.SEVEN_DAY_Y[1] + 1):
            self.assertEqual(hot[y], plain[y], "7d band must be untouched")

    def test_unknown_severity_is_treated_as_noteworthy(self):
        """The usage endpoint is undocumented; an unrecognised severity should
        fail toward being seen."""
        self.assertEqual(r._intensity("some-future-value"), r.EMPHASIS)
        self.assertEqual(r._intensity("normal"), r.DATA)

    def test_takeover_gauge_uses_the_whole_panel(self):
        f = r.render_gauge(1.0)
        self.assertEqual(f[0][0], r.EMPHASIS)  # no inset — full width
        self.assertEqual(f[33][8], r.EMPHASIS)


if __name__ == "__main__":
    unittest.main()
