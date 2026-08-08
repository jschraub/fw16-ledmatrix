"""Tests for the pure render layer. No hardware required.

The encoders are tested against the conventions measured on real modules — if
any of these fail, the panels will render mirrored, transposed, or upside down
rather than merely wrong.
"""

import unittest

from matrixd import render as r

FULL_WIDTH = (1, 7)  # a plain bar with a 1px margin, for exercising draw_bar


class TestGeometry(unittest.TestCase):
    def test_frame_shape(self):
        f = r.blank()
        self.assertEqual(len(f), 34)
        self.assertTrue(all(len(row) == 9 for row in f))
        self.assertTrue(all(v == 0 for row in f for v in row))

    def test_both_layouts_account_for_every_row(self):
        """Every row is spoken for. The blank ones are named rather than
        implied: padding around each digit block, plus the spacer between the
        hour and the minute."""
        left = (
            {0}  # padding above the hour
            | set(range(r.HOUR_Y, r.HOUR_Y + r.DIGIT_H))
            | {6}  # spacer between hour and minute
            | set(range(r.MIN_Y, r.MIN_Y + r.DIGIT_H))
            | {12}  # padding below the minute
            | set(range(r.BATTERY_Y[0], r.BATTERY_Y[1] + 1))
        )
        self.assertEqual(left, set(range(r.HEIGHT)))

        right = (
            {0}
            | set(range(r.CONTEXT_Y, r.CONTEXT_Y + r.DIGIT_H))
            | {6}
            | set(range(r.BARS_Y[0], r.BARS_Y[1] + 1))
        )
        self.assertEqual(right, set(range(r.HEIGHT)))

    def test_padding_rows_stay_dark_at_full_scale(self):
        """Padding is the only separator the layout has now that the rules are
        gone, so it has to survive the worst case — every bar at 100%, which is
        when they reach closest to the digits above them."""
        left = r.render_machine(r.MachineState(hour=23, minute=59, battery_pct=100))
        for y in (0, 6, 12):
            self.assertEqual(left[y], [r.OFF] * r.WIDTH, f"left row {y}")

        right = r.render_claude(
            r.ClaudeState(five_hour_pct=100, seven_day_pct=100, context_pct=42)
        )
        for y in (0, 6):
            self.assertEqual(right[y], [r.OFF] * r.WIDTH, f"right row {y}")

    def test_digits_fit_the_width(self):
        self.assertEqual(r._DIGIT_X[1] + r.DIGIT_W, 8)  # 1px right margin
        self.assertEqual(r._DIGIT_X[0], 1)  # 1px left margin

    def test_the_two_limit_bars_fit_side_by_side(self):
        """2 + 3 + 2, with a column spare either side, is the only arrangement
        of two 2-wide bars and a 3-wide gap that fits 9."""
        for span in (r.FIVE_HOUR_X, r.SEVEN_DAY_X):
            self.assertEqual(span[1] - span[0] + 1, 2, f"{span} should be 2 wide")
        self.assertEqual(r.SEVEN_DAY_X[0] - r.FIVE_HOUR_X[1] - 1, 3)  # gap
        self.assertEqual(r.FIVE_HOUR_X[0], 1)  # left margin
        self.assertEqual(r.WIDTH - 1 - r.SEVEN_DAY_X[1], 1)  # right margin

    def test_the_battery_bar_is_three_columns_centred(self):
        self.assertEqual(r.BATTERY_X[1] - r.BATTERY_X[0] + 1, 3)
        self.assertEqual(r.BATTERY_X[0], r.WIDTH - 1 - r.BATTERY_X[1])


class TestBars(unittest.TestCase):
    def test_empty_and_full(self):
        f = r.blank()
        r.draw_bar(f, 0, 33, 0.0, columns=FULL_WIDTH)
        self.assertTrue(all(v == 0 for row in f for v in row))

        f = r.blank()
        r.draw_bar(f, 0, 33, 1.0, columns=FULL_WIDTH)
        for y in range(34):
            self.assertTrue(all(f[y][x] == r.DATA for x in range(1, 8)))

    def test_fills_upward_from_the_bottom(self):
        """y=33 is the near end. A quarter-full bar lights the near end, not
        the far one — get this backwards and every gauge reads inverted."""
        f = r.blank()
        r.draw_bar(f, 0, 33, 0.25, columns=FULL_WIDTH)
        self.assertGreater(f[33][4], 0)
        self.assertEqual(f[0][4], 0)

    def test_partial_tip_gives_sub_level_resolution(self):
        """A bar between two whole rows renders the topmost row dimmer, which
        is the whole reason the ambient frame is greyscale."""
        f = r.blank()
        r.draw_bar(f, 0, 9, 0.55, columns=FULL_WIDTH)  # 10 rows -> 5.5
        self.assertEqual(f[9][4], r.DATA)  # bottom row full
        self.assertEqual(f[5][4], r.DATA)  # 5th row full
        self.assertGreater(f[4][4], 0)  # 6th row partial
        self.assertLess(f[4][4], r.DATA)
        self.assertEqual(f[3][4], 0)  # 7th row dark

    def test_the_tip_has_no_floor(self):
        """It used to be `max(1, int(value * tip))`, justified as "so a
        barely-started bar still shows something". It could not do that:
        greyscale 1 needs a global brightness of 520 to clear the visibility
        threshold and the hardware maximum is 255, so the floor guaranteed a
        pixel invisible on every setting the panel has. A tip too small to
        render is now honestly dark."""
        self.assertFalse(r.is_visible(r.AMBIENT_CEILING, 1))
        f = r.blank()
        r.draw_bar(f, 0, 33, 1e-9, columns=FULL_WIDTH)
        self.assertEqual(f[33][4], r.OFF)

    def test_columns_bound_the_bar(self):
        f = r.blank()
        r.draw_bar(f, 0, 33, 1.0, columns=(3, 5))
        self.assertEqual([f[33][x] for x in range(9)], [0, 0, 0] + [r.DATA] * 3 + [0] * 3)

    def test_fraction_is_clamped(self):
        for bad in (-1.0, 5.0):
            f = r.blank()
            r.draw_bar(f, 0, 33, bad, columns=FULL_WIDTH)  # must not raise or overflow
            self.assertEqual(len(f), 34)


class TestDigits(unittest.TestCase):
    def test_zero_padded(self):
        a, b = r.blank(), r.blank()
        r.draw_two_digits(a, 0, 7)
        r.draw_two_digits(b, 0, 7)
        self.assertEqual(a, b)
        self.assertGreater(sum(a[0]), 0)  # leading zero is drawn, not blank

    def test_out_of_range_renders_xx(self):
        """123 must not render as "23". It must not render as "99" either,
        which is what it used to do — a pinned 99 is a plausible value, so it
        reads as data. Negatives are covered too; they used to give "00"."""
        overflow = r.blank()
        r.draw_two_digits(overflow, 0, 100)
        for number in (100, 101, 123, 999, -1):
            f = r.blank()
            r.draw_two_digits(f, 0, number)
            self.assertEqual(f, overflow, f"{number} should render as XX")

        ninety_nine = r.blank()
        r.draw_two_digits(ninety_nine, 0, 99)
        self.assertNotEqual(ninety_nine, overflow)

    def test_xx_fills_both_digit_slots(self):
        """Stated against the glyph rather than against another call to the same
        function, which would let a one-X rendering agree with itself. A single
        X beside an empty slot reads as a rendering fault, not as a value."""
        f = r.blank()
        r.draw_two_digits(f, 0, 100)
        for slot in r._DIGIT_X:
            lit = sum(
                f[y][slot + dx] > 0
                for y in range(r.DIGIT_H)
                for dx in range(r.DIGIT_W)
            )
            self.assertEqual(lit, 9, f"slot at column {slot} is not a full X")

    def test_all_glyphs_fit_their_box(self):
        for name, glyph in [*r._FONT.items(), ("overflow", r._OVERFLOW)]:
            self.assertEqual(len(glyph), r.DIGIT_H, f"{name} height")
            for row in glyph:
                self.assertEqual(len(row), r.DIGIT_W, f"{name} width")

    def test_glyphs_are_distinguishable(self):
        """Including the overflow glyph: "off the scale" that resembled a digit
        would be worse than the clamp it replaced."""
        seen = {}
        for name, glyph in [*r._FONT.items(), ("overflow", r._OVERFLOW)]:
            key = "".join(glyph)
            self.assertNotIn(key, seen, f"{name} and {seen.get(key)} identical")
            seen[key] = name


class TestVisibility(unittest.TestCase):
    """Encodes the measured threshold model so the intent survives refactoring."""

    def test_threshold_matches_hardware_measurements(self):
        """Deliberately literal. These are properties of the panels, not of the
        palette, so they must not move when DATA does."""
        # digits at greyscale 200: legible at global 3, not at 2
        self.assertTrue(r.is_visible(3, 200))
        self.assertFalse(r.is_visible(2, 200))
        # a 1px rule at greyscale 60: legible at global 9, not at 8
        self.assertTrue(r.is_visible(9, 60))
        self.assertFalse(r.is_visible(8, 60))

    def test_data_sits_on_a_measured_product(self):
        """DATA is as low as it goes, to give EMPHASIS the widest swing above
        it. 174 is the arithmetic minimum (522 clears the 520 threshold) but
        520 is interpolated between the two measurements above; 540 is one of
        them, so DATA lands there instead of just past a guess."""
        self.assertEqual(r.AMBIENT_FLOOR * r.DATA, 540)

    def test_every_meaningful_intensity_survives_the_floor(self):
        """The panels carry no decoration any more — no rules, only padding —
        so nothing on either panel is allowed to vanish at low brightness. The
        one exception is a bar's partial tip, which is sub-row precision rather
        than a value."""
        for value in (r.DATA, r.EMPHASIS):
            self.assertTrue(r.is_visible(r.AMBIENT_FLOOR, value))
            self.assertTrue(r.is_visible(r.AMBIENT_CEILING, value))

    def test_the_working_swing_is_perceptible(self):
        """DATA -> EMPHASIS on the context number is how you see that a turn is
        in flight. What the eye judges is the ratio, not the difference, and
        the pulse work put the floor of that at about 1.15."""
        self.assertGreater(r.EMPHASIS / r.DATA, 1.4)

    def test_the_activity_signal_reads_at_the_floor(self):
        """Both of its states must be legible at the bottom of the range, since
        the number carries a value as well as the signal — an indicator that
        works by making the data disappear is not an indicator."""
        idle = r.render_claude(r.ClaudeState(context_pct=42, working=False))
        busy = r.render_claude(r.ClaudeState(context_pct=42, working=True))
        lit = [(y, x) for y in range(34) for x in range(9) if idle[y][x]]
        self.assertTrue(lit)
        for y, x in lit:
            self.assertTrue(r.is_visible(r.AMBIENT_FLOOR, idle[y][x]))
            self.assertTrue(r.is_visible(r.AMBIENT_FLOOR, busy[y][x]))
            self.assertGreater(busy[y][x], idle[y][x])


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
        self.assertGreater(sum(sum(row) for row in f[r.HOUR_Y : r.HOUR_Y + 5]), 0)
        self.assertGreater(sum(sum(row) for row in f[r.MIN_Y : r.MIN_Y + 5]), 0)
        self.assertEqual(f[6], [0] * 9)  # spacer between hour and minute
        self.assertGreater(f[33][4], 0)  # battery fills from the near end
        self.assertEqual(f[33][0], 0)  # and only in its own three columns
        self.assertEqual(f[33][8], 0)

    def test_claude_context_absent_leaves_the_zone_dark(self):
        """No live session must leave the digits dark — never reflow the bars
        into the space."""
        f = r.render_claude(r.ClaudeState(five_hour_pct=50, seven_day_pct=10))
        for y in range(r.CONTEXT_Y, r.CONTEXT_Y + r.DIGIT_H):
            self.assertEqual(f[y], [0] * 9, f"row {y} should be dark")

    def test_working_brightens_the_context_number(self):
        idle = r.render_claude(r.ClaudeState(context_pct=42, working=False))
        busy = r.render_claude(r.ClaudeState(context_pct=42, working=True))
        row = r.CONTEXT_Y
        self.assertEqual(max(idle[row]), r.DATA)
        self.assertEqual(max(busy[row]), r.EMPHASIS)
        # Same number either way: the brightness is the only thing that moves.
        self.assertEqual(
            [[v > 0 for v in idle[y]] for y in range(34)],
            [[v > 0 for v in busy[y]] for y in range(34)],
        )

    def test_context_is_truncated_not_rounded(self):
        """So XX means genuinely at or past 100, not close enough to round up."""
        for pct, same_as in ((99.9, 99), (0.9, 0)):
            got = r.render_claude(r.ClaudeState(context_pct=pct))
            want = r.render_claude(r.ClaudeState(context_pct=float(same_as)))
            self.assertEqual(got, want, f"{pct} should render as {same_as}")

    def test_context_at_full_renders_the_overflow_glyph(self):
        full = r.render_claude(r.ClaudeState(context_pct=100.0))
        expected = r.blank()
        r.draw_two_digits(expected, r.CONTEXT_Y, 100)
        self.assertEqual(full, expected)

    def test_severity_emphasises_only_its_own_bar(self):
        plain = r.render_claude(r.ClaudeState(five_hour_pct=90, seven_day_pct=90))
        hot = r.render_claude(
            r.ClaudeState(
                five_hour_pct=90, seven_day_pct=90, five_hour_severity="critical"
            )
        )
        bottom = r.BARS_Y[1]  # lit at any non-zero fraction
        self.assertEqual(plain[bottom][r.FIVE_HOUR_X[0]], r.DATA)
        self.assertEqual(hot[bottom][r.FIVE_HOUR_X[0]], r.EMPHASIS)
        for y in range(*r.BARS_Y):
            for x in range(r.SEVEN_DAY_X[0], r.SEVEN_DAY_X[1] + 1):
                self.assertEqual(hot[y][x], plain[y][x], "7d bar must be untouched")

    def test_the_two_bars_never_touch(self):
        """The gap between them is what makes them two bars rather than one
        wide one with a notch."""
        f = r.render_claude(r.ClaudeState(five_hour_pct=100, seven_day_pct=100))
        for y in range(r.BARS_Y[0], r.BARS_Y[1] + 1):
            for x in range(r.FIVE_HOUR_X[1] + 1, r.SEVEN_DAY_X[0]):
                self.assertEqual(f[y][x], 0, f"gap column {x} lit at row {y}")

    def test_unknown_severity_is_treated_as_noteworthy(self):
        """The usage endpoint is undocumented; an unrecognised severity should
        fail toward being seen."""
        self.assertEqual(r._intensity("some-future-value"), r.EMPHASIS)
        self.assertEqual(r._intensity("normal"), r.DATA)

    def test_takeover_gauge_uses_the_whole_panel(self):
        f = r.render_gauge(1.0)
        self.assertEqual(f[0][0], r.EMPHASIS)  # no margin — full width
        self.assertEqual(f[33][8], r.EMPHASIS)


if __name__ == "__main__":
    unittest.main()
