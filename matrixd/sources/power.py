"""Battery and AC state, from sysfs.

Cheap enough to read on demand — these are a handful of small files under
/sys/class/power_supply. The daemon still wants udev events for the *edges* (AC
plugged or unplugged earns a takeover), but the values themselves come from
here.

Paths are discovered rather than hardcoded: this machine calls its battery BAT1
and its adapter ACAD, which is not universal even across Framework models.
"""

from __future__ import annotations

import glob
import os
from dataclasses import dataclass

POWER_SUPPLY = "/sys/class/power_supply"


@dataclass(frozen=True)
class Power:
    battery_pct: float | None
    charging: bool
    on_ac: bool


def is_charging(status: str | None) -> bool:
    """Whether current is actually going into the battery.

    sysfs reports "Charging", "Discharging", "Full", "Not charging", or
    "Unknown". Only "Charging" qualifies: "Full" while plugged in is *not*
    charging, and treating it as such would pulse the panel indefinitely for the
    entire time the laptop sits on mains at 100%. "Not charging" is the state a
    charge-limit threshold produces and is likewise not charging.
    """
    return status == "Charging"


def _read(path: str) -> str | None:
    try:
        with open(path) as f:
            return f.read().strip()
    except OSError:
        return None


def _battery_dir() -> str | None:
    """First device whose type is Battery.

    Matching on the `type` attribute rather than a BAT* glob avoids picking up
    the USB-C port power supplies, which on this machine outnumber the actual
    battery five to one.
    """
    for path in sorted(glob.glob(os.path.join(POWER_SUPPLY, "*"))):
        if _read(os.path.join(path, "type")) == "Battery":
            return path
    return None


def _mains_dir() -> str | None:
    for path in sorted(glob.glob(os.path.join(POWER_SUPPLY, "*"))):
        if _read(os.path.join(path, "type")) == "Mains":
            return path
    return None


def read() -> Power:
    """Current battery and AC state. Never raises; unknown values are None."""
    pct: float | None = None
    charging = False

    bat = _battery_dir()
    if bat:
        raw = _read(os.path.join(bat, "capacity"))
        if raw is not None:
            try:
                pct = max(0.0, min(100.0, float(raw)))
            except ValueError:
                pct = None
        charging = is_charging(_read(os.path.join(bat, "status")))

    on_ac = False
    mains = _mains_dir()
    if mains:
        on_ac = _read(os.path.join(mains, "online")) == "1"

    return Power(battery_pct=pct, charging=charging, on_ac=on_ac)
