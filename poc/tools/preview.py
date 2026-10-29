#!/usr/bin/env python3
"""Push a rendered frame to the real panels.

The bridge between the pure render layer and hardware — deliberately the only
place the two meet, so render.py stays testable without a laptop attached.

Usage:
    preview.py [--brightness N] [--bw]

`--brightness` is the global per-panel level the daemon will drive from screen
brightness (1 is the calibrated dark-room floor, 255 the ceiling). `--bw` uses
the 25ms 1-bit path instead of the 169ms greyscale one, which is what takeovers
will use.
"""

import argparse
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import smoke  # noqa: E402  (same directory)
from matrixd import render as r  # noqa: E402

CMD_STAGE_COL, CMD_FLUSH_COLS = 0x07, 0x08


def push(path: str, frame: r.Frame, brightness: int, use_bw: bool) -> None:
    fd = smoke.open_raw(path)
    try:
        smoke.send(fd, smoke.CMD_SLEEP, bytes([0]))
        time.sleep(0.4)
        smoke.send(fd, smoke.CMD_BRIGHTNESS, bytes([brightness]))
        if use_bw:
            smoke.send(fd, smoke.CMD_DRAW_BW, r.to_drawbw(frame))
        else:
            for payload in r.to_columns(frame):
                smoke.send(fd, CMD_STAGE_COL, payload)
            smoke.send(fd, CMD_FLUSH_COLS)
        time.sleep(0.05)
    finally:
        os.close(fd)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--brightness", type=int, default=40)
    ap.add_argument("--bw", action="store_true")
    args = ap.parse_args()

    panels = smoke.find_panels()
    if not panels:
        sys.exit("no LED matrix modules found")

    now = time.localtime()
    frames = {
        "left": r.render_machine(
            r.MachineState(hour=now.tm_hour, minute=now.tm_min, battery_pct=74)
        ),
        "right": r.render_claude(
            r.ClaudeState(
                five_hour_pct=62,
                seven_day_pct=18,
                context_pct=41,
                working=True,
                five_hour_severity="warning",
            )
        ),
    }
    for side, frame in frames.items():
        push(smoke.resolve(panels, side), frame, args.brightness, args.bw)
    print(
        f"pushed at global brightness {args.brightness}/255"
        f" via {'DrawBW' if args.bw else 'greyscale'}"
    )


if __name__ == "__main__":
    main()
