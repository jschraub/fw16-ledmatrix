"""Screen backlight, and the mapping from it to panel brightness.

Panel brightness follows *screen* brightness, not keyboard backlight. The screen
is the knob you actually reach for when the room changes, whereas keyboard
backlight is inversely correlated with ambient light — people raise it in the
dark and switch it off in daylight, so tracking it would make the panels
brightest exactly when you want them dimmest.

Reading is sysfs. Change *events* are not: the daemon learns about brightness
changes from a keybind hook rather than udev, because udev cannot distinguish
your thumb from hypridle's idle timer, and a udev-driven takeover would fire a
brightness popup at the moment you walked away from the machine.
"""

from __future__ import annotations

import glob
import os

from .. import render

BACKLIGHT = "/sys/class/backlight"


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
