"""Screen backlight, and the mapping from it to panel brightness.

Panel brightness follows *screen* brightness, not keyboard backlight. The screen
is the knob you actually reach for when the room changes, whereas keyboard
backlight is inversely correlated with ambient light — people raise it in the
dark and switch it off in daylight, so tracking it would make the panels
brightest exactly when you want them dimmest.

Reading is sysfs, and so is noticing changes: the daemon tracks brightness from
udev `backlight` events.

That reverses an earlier decision to use a keybind hook, for a reason that turned
out to matter more than the one behind the original choice. A keybind hook only
fires for the *keyboard*, so brightness changed any other way — a settings
slider, a script, a docking profile — would leave the panels at a stale level
indefinitely. udev sees every change whatever caused it, which makes tracking
correct by construction.

The original objection to udev stands on its own terms, though: it cannot tell
your thumb from hypridle's idle timer, and a takeover firing as you walk away
from the machine is exactly wrong. That is solved separately and narrowly by
`changed_automatically()` — the idle script marks its own writes, so the daemon
suppresses the *takeover* while still tracking the *value*. Tracking and
reacting are different questions, and conflating them was the mistake.
"""

from __future__ import annotations

import glob
import os
import time

from .. import render

BACKLIGHT = "/sys/class/backlight"

# Touched by hyprland's idle-dim.sh immediately before and after each of its own
# brightnessctl calls. Touched on both sides on purpose: the uevent is emitted
# by the kernel during the write, but the daemon reads it asynchronously, so a
# marker written only before (or only after) leaves a race in which an automatic
# change looks deliberate.
AUTO_MARKER = "matrixd/brightness-auto"

# How recently the marker must have been touched for a brightness change to
# count as automatic. Generously wide relative to the milliseconds actually
# involved: the cost of being wrong in this direction is one missed takeover,
# against a full-panel gauge lighting up as you leave the room.
AUTO_WINDOW = 2.0


def _read_int(path: str) -> int | None:
    try:
        with open(path) as f:
            return int(f.read().strip())
    except (OSError, ValueError):
        return None


def _device() -> str | None:
    devices = sorted(glob.glob(os.path.join(BACKLIGHT, "*")))
    return devices[0] if devices else None


def read_fraction() -> float | None:
    """Screen brightness as 0.0-1.0, or None if it cannot be read.

    Uses `brightness` rather than `actual_brightness`: the former is the
    requested value and changes the instant it is set, while the latter can lag
    or be clamped by the hardware.
    """
    dev = _device()
    if not dev:
        return None
    value = _read_int(os.path.join(dev, "brightness"))
    maximum = _read_int(os.path.join(dev, "max_brightness"))
    if value is None or not maximum:
        return None
    return max(0.0, min(1.0, value / maximum))


# The lowest screen brightness reachable in practice. `brightnessctl -n2` in the
# hypr keybinds enforces a floor of 2/100, so the screen never goes below this
# and the mapping must anchor here rather than at zero. Anchoring at zero puts
# the panel at 8 when the screen is at its minimum — nearly 3x the calibrated
# floor, i.e. wrong in exactly the situation the floor exists to protect.
SCREEN_FLOOR_FRACTION = 0.02


def panel_brightness(fraction: float | None) -> int:
    """Map screen brightness onto the panel's global brightness.

    Anchored on the two calibrated points rather than on the raw 0-1 range:

        screen  2/100  ->  panel   3/255   (AMBIENT_FLOOR)
        screen 100/100 ->  panel 255/255   (AMBIENT_CEILING)

    The floor is 3 because that is the lowest global brightness at which *real
    content* is legible. A figure calibrated against a solid fill would be far
    too low, since a full panel reads as a glow at currents where a few thin
    bands are invisible.

    Linear between the anchors. The screen keybinds use `brightnessctl -e4`, an
    exponential curve, so the sysfs value is already perceptually spaced —
    whether the panel mapping should match that curve too is an open question
    best settled by looking at it rather than reasoning about it.
    """
    if fraction is None:
        return render.AMBIENT_FLOOR
    lo, hi = render.AMBIENT_FLOOR, render.AMBIENT_CEILING
    span = 1.0 - SCREEN_FLOOR_FRACTION
    t = (max(SCREEN_FLOOR_FRACTION, min(1.0, fraction)) - SCREEN_FLOOR_FRACTION) / span
    return int(round(lo + (hi - lo) * t))


def _marker_path() -> str | None:
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    return os.path.join(runtime, AUTO_MARKER) if runtime else None


def changed_automatically(now: float | None = None) -> bool:
    """Whether the brightness change just seen was hypridle's, not yours.

    Absence of the marker means "deliberate", which is the right way round to
    fail: if the idle script is not installed, brightness changes are yours by
    definition, and a takeover is correct.
    """
    path = _marker_path()
    if not path:
        return False
    try:
        touched = os.stat(path).st_mtime
    except OSError:
        return False
    now = time.time() if now is None else now
    return 0 <= now - touched <= AUTO_WINDOW


# DPMS state, read from the connector rather than inferred from brightness.
# Backlight and DPMS are independent: hypridle blanks the screen without
# touching the backlight level, so a daemon watching only brightness would keep
# the panels lit against a black screen.
DRM = "/sys/class/drm"


def _connector() -> str | None:
    """The connected, enabled connector — usually the internal panel.

    Picked by state rather than by name: this machine exposes two eDP
    connectors, and the one that looks canonical (`card1-eDP-1`) is the
    disconnected one.
    """
    for path in sorted(glob.glob(os.path.join(DRM, "card*-*"))):
        try:
            with open(os.path.join(path, "status")) as f:
                if f.read().strip() != "connected":
                    continue
            with open(os.path.join(path, "enabled")) as f:
                if f.read().strip() != "enabled":
                    continue
        except OSError:
            continue
        return path
    return None


def is_screen_off() -> bool:
    """True when DPMS has blanked the display.

    Unknown reads as *on*: a daemon that cannot tell should leave the panels
    working rather than silently blank them, since a dark panel looks identical
    to a crashed daemon.
    """
    connector = _connector()
    if not connector:
        return False
    # The dpms attribute lives on the card-scoped node, not the class symlink.
    for candidate in (
        os.path.join(connector, "dpms"),
        os.path.join(os.path.realpath(connector), "dpms"),
    ):
        try:
            with open(candidate) as f:
                return f.read().strip() == "Off"
        except OSError:
            continue
    return False
