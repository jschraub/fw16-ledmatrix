"""Pure rendering: state in, frame out.

Nothing here does I/O, sleeps, or touches hardware. Every function is a pure
function of its arguments, which is what makes the layout unit-testable without
a laptop attached — and what makes this the part that ports to Rust
mechanically.

Coordinate system, all four conventions verified on hardware rather than
assumed (see README.md):

    y = 0    far end of the panel, toward the screen
    y = 33   near end, toward you — bars fill upward from here
    x = 0    your left, on BOTH panels. The two modules are seated alike, so
             there is no per-panel mirroring.
    DrawBW   row-major, bit index = y*9 + x, LSB-first within each byte.

Intensity is relative *within* a frame. Absolute brightness is a separate
global per-panel setting the daemon drives from screen brightness; the two
compose, with global brightness scaling these greyscale values rather than
overriding them.
"""

from __future__ import annotations

from dataclasses import dataclass

WIDTH = 9
HEIGHT = 34
BITMAP_BYTES = 39  # 9*34 = 306 bits, packed into 39 bytes (6 unused)

# Relative intensities. Only two carry meaning: DATA is everything the panels
# normally show, EMPHASIS is DATA raised to say something about itself — rate
# limit severity on the bars, a turn in flight on the context number.
#
# DATA is 180 rather than 200 to buy EMPHASIS more room above it. What the eye
# judges is the ratio between them, not the difference: 200 -> 255 is 1.28x,
# 180 -> 255 is 1.42x. It cannot go lower. Visibility is a product, so at
# AMBIENT_FLOOR (global 3) a greyscale of 180 lands on 540 — the dimmest product
# actually measured legible on this hardware. 174 is the arithmetic minimum
# against the 520 threshold, but 520 is interpolated between two measurements
# and 540 is one of them, so this sits on the evidence rather than just past it.
OFF = 0
DATA = 180
EMPHASIS = 255

# Perceptual visibility threshold, measured on hardware. An LED reads as lit
# when (global_brightness * greyscale) is roughly >= 520. Neither number matters
# on its own — it is the product. Verified across a 3x range of both:
#
#     digits, greyscale 200 : legible at global 3 (600), not at 2 (400)
#     rules,  greyscale  60 : legible at global 9 (540), not at 8 (480)
#
VISIBILITY_THRESHOLD = 520

# Global brightness range the daemon drives from screen brightness.
AMBIENT_FLOOR = 3
AMBIENT_CEILING = 255


def is_visible(global_brightness: int, greyscale: int) -> bool:
    """Whether a pixel at this greyscale reads as lit at this global brightness.

    See VISIBILITY_THRESHOLD. Useful for asserting intent in tests rather than
    rediscovering the constraint on hardware.
    """
    return global_brightness * greyscale >= VISIBILITY_THRESHOLD


# ── Every element is legible at every brightness ─────────────────────────────
#
# Both panels used to carry separator rules at greyscale 60, which vanished
# below global 9 (~5% screen brightness). That was deliberate — decoration is
# what you drop when the hardware forces a choice — but the layout no longer
# has any decoration to drop. Padding rows separate the zones now, and a dark
# row costs nothing to see.
#
# What is left is DATA and EMPHASIS, both of which clear the threshold at
# AMBIENT_FLOOR. So the panels show the same content at every screen
# brightness, only dimmer, and the only thing that can fall below the threshold
# is the partial tip of a bar — which is sub-row precision, not a value.
#
# See test_every_meaningful_intensity_survives_the_floor.

Frame = list[list[int]]


def blank() -> Frame:
    """A frame with every pixel dark."""
    return [[OFF] * WIDTH for _ in range(HEIGHT)]


# ── primitives ───────────────────────────────────────────────────────────────

# 3x5 digits. Wider would not fit: two digits plus a 1px gap is 7 of the 9
# available columns, leaving a 1px margin either side. Three digits would need
# 11, which is why no value on these panels is ever rendered as "100".
_FONT = {
    0: ("###", "#.#", "#.#", "#.#", "###"),
    1: (".#.", "##.", ".#.", ".#.", "###"),
    2: ("###", "..#", "###", "#..", "###"),
    3: ("###", "..#", "###", "..#", "###"),
    4: ("#.#", "#.#", "###", "..#", "..#"),
    5: ("###", "#..", "###", "..#", "###"),
    6: ("###", "#..", "###", "#.#", "###"),
    7: ("###", "..#", "..#", "..#", "..#"),
    8: ("###", "#.#", "###", "#.#", "###"),
    9: ("###", "#.#", "###", "..#", "###"),
}

# Shown in place of a number that will not fit in two digits. No digit in the
# font has this shape, so "off the scale" cannot be misread as a value.
_OVERFLOW = ("#.#", "#.#", ".#.", "#.#", "#.#")

DIGIT_W, DIGIT_H = 3, 5
# Two digits: columns 1-3 and 5-7, centred in 9. The dark columns either side
# are the horizontal padding — it falls out of the geometry rather than needing
# to be drawn.
_DIGIT_X = (1, 5)


def _stamp(frame: Frame, x: int, y: int, glyph: tuple[str, ...], value: int) -> None:
    """Paint one 3x5 glyph with its top-left corner at (x, y)."""
    for dy, row in enumerate(glyph):
        for dx, cell in enumerate(row):
            if cell == "#":
                frame[y + dy][x + dx] = value


def draw_digit(frame: Frame, x: int, y: int, digit: int, value: int = DATA) -> None:
    """Stamp one 3x5 digit with its top-left corner at (x, y)."""
    _stamp(frame, x, y, _FONT[digit], value)


def draw_two_digits(frame: Frame, y: int, number: int, value: int = DATA) -> None:
    """Stamp a zero-padded two-digit number, centred, top row at y.

    Anything outside 0-99 renders as "XX". Three digits do not fit — they would
    need 11 of the 9 available columns — so something has to give, and the
    earlier behaviour of clamping to 99 gave up the wrong thing: a pinned 99 is
    still a plausible reading, so it looks like data. "XX" cannot be mistaken
    for a value, which is the honest thing to show when there is no value that
    fits. It covers negatives too, which used to render a confident "00".

    The clock never reaches this branch (hour <= 23, minute <= 59). The context
    percentage does, at 100%.
    """
    if not 0 <= number <= 99:
        for x in _DIGIT_X:
            _stamp(frame, x, y, _OVERFLOW, value)
        return
    for slot, digit in enumerate(divmod(number, 10)):
        draw_digit(frame, _DIGIT_X[slot], y, digit, value)


def draw_bar(
    frame: Frame,
    y0: int,
    y1: int,
    fraction: float,
    value: int = DATA,
    *,
    columns: tuple[int, int],
) -> None:
    """Fill a vertical bar in rows y0..y1, columns[0]..columns[1], all
    inclusive, growing upward from y1.

    The topmost lit row is rendered at partial intensity proportional to the
    remainder, which roughly doubles effective resolution: a 13-row bar reads
    closer to 26 levels. That is why the ambient frame is greyscale — in 1-bit
    the same bar would be strictly integer-valued.

    The column range is explicit rather than a symmetric inset because the
    Claude panel places two bars side by side, which no inset can express.

    A tip too dim to clear the visibility threshold stays dim. There used to be
    a `max(1, ...)` here, "so a barely-started bar still shows something" —
    which it could not do: greyscale 1 would need a global brightness of 520 to
    be seen and the maximum is 255, so the floor guaranteed a pixel invisible on
    every setting the hardware has.
    """
    x0, x1 = columns
    rows = y1 - y0 + 1
    exact = max(0.0, min(1.0, fraction)) * rows
    full = int(exact)
    tip = exact - full

    for i in range(full):
        for x in range(x0, x1 + 1):
            frame[y1 - i][x] = value

    if tip > 0 and full < rows:
        partial = int(value * tip)
        for x in range(x0, x1 + 1):
            frame[y1 - full][x] = partial


# ── layout: left panel (machine) ─────────────────────────────────────────────
#
# A dark row of padding around the clock, then the battery bar takes everything
# below it. There is no divider: the padding row already separates them, and a
# rule would be a second separator doing the same job less well.

HOUR_Y = 1  # rows 1-5    (row 0 is padding)
MIN_Y = 7  # rows 7-11   (row 6 is the gap between hour and minute)
BATTERY_Y = (13, 33)  # 21 rows  (row 12 is padding)
# 3 columns rather than the full width. The bar is a magnitude, not a picture,
# and a narrower one leaves the panel reading as a clock with a gauge beside it
# instead of a clock sitting on a slab. It is still 21 rows, so the resolution
# is unchanged.
BATTERY_X = (3, 5)


@dataclass(frozen=True)
class MachineState:
    hour: int  # 0-23, 24-hour clock: 12-hour would need an AM/PM marker
    minute: int  # 0-59
    battery_pct: float  # 0-100
    charging: bool = False  # drives a global-brightness pulse, not frame content


def render_machine(state: MachineState) -> Frame:
    frame = blank()
    draw_two_digits(frame, HOUR_Y, state.hour)
    draw_two_digits(frame, MIN_Y, state.minute)
    draw_bar(
        frame,
        *BATTERY_Y,
        fraction=state.battery_pct / 100.0,
        columns=BATTERY_X,
    )
    return frame


# ── layout: right panel (Claude) ─────────────────────────────────────────────
#
# Context is a number, not a bar. It is the one value on either panel worth
# reading exactly rather than gauging — the difference between 78% and 91% is
# the difference between carrying on and wrapping up, and two bar rows do not
# say that. It also gets the top of the panel because it is the value that
# changes fastest and gets consulted most.
#
# The two rate limits sit side by side beneath it. 2 columns each, a 3-column
# gap, 1-column margins: the only arrangement of those that fits 9. Side by
# side rather than stacked so both get all 27 rows. That retires an earlier
# decision to give 7d fewer rows because it "rarely nears its ceiling" — which
# had it backwards. At 6 rows anything under 16.7% was a single dim partial
# row, so the band was coarsest exactly across the range it actually occupies.
# At 27 rows one row is 3.7%.

CONTEXT_Y = 1  # rows 1-5   (row 0 is padding, row 6 separates it from the bars)
BARS_Y = (7, 33)  # 27 rows, shared by both limits
FIVE_HOUR_X = (1, 2)  # left: the one that moves, and that you check most
SEVEN_DAY_X = (6, 7)


@dataclass(frozen=True)
class ClaudeState:
    five_hour_pct: float | None = None
    seven_day_pct: float | None = None
    context_pct: float | None = None  # None when no session is live
    working: bool = False
    five_hour_severity: str = "normal"
    seven_day_severity: str = "normal"


def _intensity(severity: str) -> int:
    """Anything the API does not call "normal" is emphasised.

    Deliberately open to unknown values: the usage endpoint is undocumented, so
    treating an unrecognised severity as noteworthy fails toward being seen
    rather than toward silence.
    """
    return DATA if severity == "normal" else EMPHASIS


def render_claude(state: ClaudeState) -> Frame:
    frame = blank()

    if state.five_hour_pct is not None:
        draw_bar(
            frame,
            *BARS_Y,
            fraction=state.five_hour_pct / 100.0,
            value=_intensity(state.five_hour_severity),
            columns=FIVE_HOUR_X,
        )

    if state.seven_day_pct is not None:
        draw_bar(
            frame,
            *BARS_Y,
            fraction=state.seven_day_pct / 100.0,
            value=_intensity(state.seven_day_severity),
            columns=SEVEN_DAY_X,
        )

    # Context is per-session and simply absent when nothing is running. The
    # zone stays dark rather than the others reflowing to fill it — a layout
    # that moves turns a glance into a lookup.
    #
    # Its *brightness* carries a second, independent variable: whether a turn is
    # in flight. That is a deliberate overload — the number's shape says how
    # full the window is, its intensity says whether Claude is thinking — and it
    # is what the activity rule used to do before the rules went away. The cost
    # is that the two are now coupled at the display even though they arrive
    # from different files (the status-line tap writes the percentage, the
    # session hook writes the state), so a payload Claude Code renames takes out
    # both. Accepted: that failure is visible as a dark space where a number
    # belongs, which is a symptom you can act on.
    #
    # Truncated, not rounded, so "XX" means genuinely at or past 100 rather than
    # close enough to round up.
    if state.context_pct is not None:
        draw_two_digits(
            frame,
            CONTEXT_Y,
            int(state.context_pct),
            EMPHASIS if state.working else DATA,
        )

    return frame


# ── takeovers ────────────────────────────────────────────────────────────────

GAUGE_X = (0, WIDTH - 1)  # the whole width: a takeover owns the panel


def render_gauge(fraction: float, value: int = EMPHASIS) -> Frame:
    """A single full-height bar filling the panel — volume, brightness, battery.

    Takeovers have the whole panel, so they get all 34 rows: finer than any
    ambient zone, and readable without resolving which band you are looking at.
    """
    frame = blank()
    draw_bar(frame, 0, HEIGHT - 1, fraction, value, columns=GAUGE_X)
    return frame


# ── encoders ─────────────────────────────────────────────────────────────────


def to_drawbw(frame: Frame, threshold: int = 128) -> bytes:
    """Pack to the 39-byte 1-bit format. Row-major, LSB-first — both verified.

    The fast path: 25ms versus 169ms for a full greyscale frame, which is why
    takeovers and post-reconnect repaints use it and the ambient frame does not.
    """
    out = bytearray(BITMAP_BYTES)
    for y in range(HEIGHT):
        row = frame[y]
        for x in range(WIDTH):
            if row[x] >= threshold:
                i = y * WIDTH + x
                out[i // 8] |= 1 << (i % 8)
    return bytes(out)


def to_columns(frame: Frame) -> list[bytes]:
    """Pack to nine StageCol payloads: column index followed by 34 values.

    The caller sends each of these as a StageCol command and then one
    FlushCols to commit.
    """
    return [
        bytes([x]) + bytes(frame[y][x] for y in range(HEIGHT)) for x in range(WIDTH)
    ]


def to_ascii(frame: Frame) -> str:
    """Render a frame as text for tests and debugging.

    Worth having: the layout arithmetic is easy to get subtly wrong, and a
    failing assertion about byte 17 tells you far less than looking at it.
    """
    ramp = " .:-=+*#%@"
    return "\n".join(
        "".join(ramp[min(len(ramp) - 1, v * len(ramp) // 256)] for v in row)
        for row in frame
    )
