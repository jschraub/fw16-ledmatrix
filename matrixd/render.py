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

# Relative intensities. DATA sits below full scale so that EMPHASIS has
# somewhere to go — severity is expressed by rendering a zone brighter than its
# neighbours, and there is no headroom above 255.
OFF = 0
RULE = 60
DATA = 200
EMPHASIS = 255

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

DIGIT_W, DIGIT_H = 3, 5
_DIGIT_X = (1, 5)  # two digits: columns 1-3 and 5-7, centred in 9


def draw_digit(frame: Frame, x: int, y: int, digit: int, value: int = DATA) -> None:
    """Stamp one 3x5 digit with its top-left corner at (x, y)."""
    glyph = _FONT[digit]
    for dy, row in enumerate(glyph):
        for dx, cell in enumerate(row):
            if cell == "#":
                frame[y + dy][x + dx] = value


def draw_two_digits(frame: Frame, y: int, number: int, value: int = DATA) -> None:
    """Stamp a zero-padded two-digit number, centred, top row at y.

    Values above 99 are clamped rather than truncated: showing "23" for 123
    would be a confidently wrong reading, whereas a pinned "99" reads as
    saturation.
    """
    number = max(0, min(99, number))
    for slot, digit in enumerate(divmod(number, 10)):
        draw_digit(frame, _DIGIT_X[slot], y, digit, value)


def draw_rule(frame: Frame, y: int, value: int = RULE) -> None:
    """A horizontal separator spanning the full width.

    Dim by default, which is what makes a fully-lit rule usable as a state
    indicator elsewhere: a bright rule cannot be mistaken for data, because a
    rule and a bar are different shapes in different places.
    """
    for x in range(WIDTH):
        frame[y][x] = value


def draw_bar(
    frame: Frame,
    y0: int,
    y1: int,
    fraction: float,
    value: int = DATA,
    *,
    inset: int = 1,
) -> None:
    """Fill a vertical bar in rows y0..y1 inclusive, growing upward from y1.

    The topmost lit row is rendered at partial intensity proportional to the
    remainder, which roughly doubles effective resolution: a 13-row bar reads
    closer to 26 levels. That is why the ambient frame is greyscale — in 1-bit
    the same bar would be strictly integer-valued.

    `inset` leaves a margin either side so bars are visually distinct from
    rules, which always span the full width.
    """
    rows = y1 - y0 + 1
    exact = max(0.0, min(1.0, fraction)) * rows
    full = int(exact)
    tip = exact - full

    for i in range(full):
        for x in range(inset, WIDTH - inset):
            frame[y1 - i][x] = value

    if tip > 0 and full < rows:
        # Floor the partial tip at 1 so a barely-started bar still shows
        # something; rounding it to 0 would make low values indistinguishable
        # from empty.
        partial = max(1, int(value * tip))
        for x in range(inset, WIDTH - inset):
            frame[y1 - full][x] = partial


# ── layout: left panel (machine) ─────────────────────────────────────────────

HOUR_Y = 0  # rows 0-4
MIN_Y = 6  # rows 6-10   (row 5 is a gap)
LEFT_RULE_Y = 11
BATTERY_Y = (12, 33)  # 22 rows


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
    draw_rule(frame, LEFT_RULE_Y)
    draw_bar(frame, *BATTERY_Y, fraction=state.battery_pct / 100.0)
    return frame


# ── layout: right panel (Claude) ─────────────────────────────────────────────

FIVE_HOUR_Y = (0, 12)  # 13 rows
RIGHT_RULE_1_Y = 13
SEVEN_DAY_Y = (14, 19)  # 6 rows — rarely near its ceiling, so it gets fewer
RIGHT_RULE_2_Y = 20  # doubles as the activity indicator
CONTEXT_Y = (21, 33)  # 13 rows


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
            *FIVE_HOUR_Y,
            fraction=state.five_hour_pct / 100.0,
            value=_intensity(state.five_hour_severity),
        )
    draw_rule(frame, RIGHT_RULE_1_Y)

    if state.seven_day_pct is not None:
        draw_bar(
            frame,
            *SEVEN_DAY_Y,
            fraction=state.seven_day_pct / 100.0,
            value=_intensity(state.seven_day_severity),
        )

    # The second rule is the activity light: lit while Claude is working, dim
    # otherwise. Costs no data rows.
    draw_rule(frame, RIGHT_RULE_2_Y, EMPHASIS if state.working else RULE)

    # Context is per-session and simply absent when nothing is running. The
    # zone stays dark rather than the others reflowing to fill it — a layout
    # that moves turns a glance into a lookup.
    if state.context_pct is not None:
        draw_bar(frame, *CONTEXT_Y, fraction=state.context_pct / 100.0)

    return frame


# ── takeovers ────────────────────────────────────────────────────────────────


def render_gauge(fraction: float, value: int = EMPHASIS) -> Frame:
    """A single full-height bar filling the panel — volume, brightness, battery.

    Takeovers have the whole panel, so they get all 34 rows: finer than any
    ambient zone, and readable without resolving which band you are looking at.
    """
    frame = blank()
    draw_bar(frame, 0, HEIGHT - 1, fraction, value, inset=0)
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
