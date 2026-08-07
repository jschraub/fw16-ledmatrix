#!/usr/bin/env python3
"""Smoke test for the Framework Laptop 16 LED Matrix input modules.

Establishes the one assumption the whole panel design rests on: that the two
modules can be addressed independently, and that we know which is which.

Both modules report the SAME USB serial number, so /dev/serial/by-id/ collapses
to a single symlink and is useless for telling them apart. We enumerate via
/dev/serial/by-path/ instead, which encodes USB topology and is stable per
physical bay.

Protocol: USB CDC-ACM, 115200 8N1. Every command is 0x32 0xAC then a command
byte then its payload. No external dependencies — raw termios, no pyserial.

Usage:
    smoke.py probe            query firmware version on each panel (no display change)
    smoke.py on   <sel>       fill a panel (sel: a port path, or its index from probe)
    smoke.py off  <sel|all>   blank a panel, or all of them
    smoke.py sweep            light each panel in turn, 2s apart, then blank
    smoke.py ramp <sel>       step a filled panel through the brightness range,
                              2.5s per step, to calibrate floor and ceiling by eye
"""

import glob
import os
import sys
import termios
import time

MAGIC = b"\x32\xAC"

CMD_BRIGHTNESS = 0x00
CMD_SLEEP = 0x03
CMD_DRAW_BW = 0x06
CMD_VERSION = 0x20

# 9 wide x 34 tall = 306 pixels, packed into 39 bytes (312 bits, 6 unused).
BITMAP_BYTES = 39

# Deliberately dim. These sit under your palms; full brightness is unpleasant
# and tells you nothing extra during a smoke test.
SMOKE_BRIGHTNESS = 20

# Seconds to wait after open() before the device will accept a command.
OPEN_SETTLE = 0.2

# Which physical bay each USB port path corresponds to. Determined empirically
# on 2026-08-07 via `smoke.py sweep`: port 3.3 lit the right-hand panel, 4.2 the
# left. There is nothing in USB topology that reveals this — it can only be
# found by lighting one and looking. Note the inversion: the LOWER-numbered port
# is the RIGHT bay. Panels are interchangeable and share a serial number, so
# this maps bays, not modules; moving a module to the other slot keeps the
# naming correct.
SIDE_BY_USB_PORT = {"3.3": "right", "4.2": "left"}


def find_panels():
    """Return [(by_path, ttyname, usb_port)] for every LED matrix, sorted.

    /dev/serial/by-path/ lists each device twice (`-usb-` and `-usbv2-` forms);
    keep the plain one. Confirm vendor/product via sysfs rather than trusting
    the path, so an unrelated CDC-ACM device can never be mistaken for a panel.
    """
    panels = []
    for path in sorted(glob.glob("/dev/serial/by-path/*-usb-*")):
        tty = os.path.basename(os.path.realpath(path))
        usb_dev = f"/sys/class/tty/{tty}/device/.."
        try:
            with open(os.path.join(usb_dev, "idVendor")) as f:
                vid = f.read().strip()
            with open(os.path.join(usb_dev, "idProduct")) as f:
                pid = f.read().strip()
            with open(os.path.join(usb_dev, "devpath")) as f:
                port = f.read().strip()
        except OSError:
            continue
        if (vid, pid) == ("32ac", "0020"):
            panels.append((path, tty, port))
    return panels


def open_raw(path):
    """Open a CDC-ACM port in raw mode at 115200 8N1."""
    fd = os.open(path, os.O_RDWR | os.O_NOCTTY)
    iflag, oflag, cflag, lflag, _ispeed, _ospeed, cc = termios.tcgetattr(fd)
    iflag = 0
    oflag = 0
    lflag = 0
    cflag = termios.CS8 | termios.CREAD | termios.CLOCAL
    cc = list(cc)
    cc[termios.VMIN] = 0
    cc[termios.VTIME] = 5  # 0.5s read timeout
    termios.tcsetattr(
        fd,
        termios.TCSANOW,
        [iflag, oflag, cflag, lflag, termios.B115200, termios.B115200, cc],
    )
    termios.tcflush(fd, termios.TCIOFLUSH)
    # Opening a CDC-ACM port toggles DTR/line state, and the RP2040 silently
    # drops a command sent before its USB stack has resettled. Measured: a
    # command issued immediately after open gets no reply; 0.2s later the
    # device answers in under 10ms, repeatably. The daemon must therefore hold
    # both ports open for its lifetime rather than reopening per frame — a
    # 200ms reopen would eat the entire takeover latency budget on its own.
    time.sleep(OPEN_SETTLE)
    return fd


def send(fd, cmd, payload=b""):
    os.write(fd, MAGIC + bytes([cmd]) + payload)


def read_exact(fd, n, deadline=1.0):
    buf = b""
    end = time.monotonic() + deadline
    while len(buf) < n and time.monotonic() < end:
        chunk = os.read(fd, n - len(buf))
        if chunk:
            buf += chunk
    return buf


def cmd_probe(panels):
    print(f"{len(panels)} panel(s) found\n")
    ok = True
    for i, (path, tty, port) in enumerate(panels):
        side = SIDE_BY_USB_PORT.get(port, "UNKNOWN BAY")
        print(f"[{i}] {side.upper():5}  {tty}  usb port {port}")
        print(f"    {path}")
        try:
            fd = open_raw(path)
        except PermissionError:
            print("    !! permission denied — udev rule not installed?\n")
            ok = False
            continue
        try:
            # The firmware sleeps on an idle timer (default 60s, reset by any
            # command). A sleeping module does not answer: the first command
            # wakes it and is consumed doing so, so a bare Version query after
            # an idle period reliably returns 0 bytes. Wake explicitly first —
            # this is why `on`/`off` never failed while `probe` did, since
            # set_fill() already sends a wake before drawing.
            # Waking fades the LEDs in rather than switching instantly, and the
            # module does not service commands during that transition. An
            # explicit wake plus a generous deadline gets this right most of the
            # time but not every time (measured 2/3 on cold modules), so also
            # retry — Version is an idempotent query and a retry costs ~nothing.
            send(fd, CMD_SLEEP, bytes([0]))
            time.sleep(0.4)
            resp = b""
            for _ in range(3):
                send(fd, CMD_VERSION)
                resp = read_exact(fd, 3, 2.0)
                if len(resp) == 3:
                    break
                time.sleep(0.3)
            if len(resp) == 3:
                # Bytes 0-1 are USB bcdDevice (MSB, LSB) — BCD, not decimal.
                # 0x00 0x20 is version 0.20, which printed as decimal would
                # misleadingly read "0.32".
                major, minor, pre = resp[0], resp[1], resp[2]
                tag = " (pre-release)" if pre else ""
                print(f"    firmware {major:x}.{minor:02x}{tag}  <- responded, protocol OK\n")
            else:
                print(f"    !! no/short response ({len(resp)} bytes)\n")
                ok = False
        finally:
            os.close(fd)
    return ok


def set_fill(path, on):
    fd = open_raw(path)
    try:
        send(fd, CMD_SLEEP, bytes([0]))  # make sure it is awake
        send(fd, CMD_BRIGHTNESS, bytes([SMOKE_BRIGHTNESS if on else 0]))
        send(fd, CMD_DRAW_BW, bytes([0xFF if on else 0x00]) * BITMAP_BYTES)
        # CDC-ACM writes are buffered; give the RP2040 a moment before close.
        time.sleep(0.05)
    finally:
        os.close(fd)


def resolve(panels, sel):
    if sel.isdigit() and int(sel) < len(panels):
        return panels[int(sel)][0]
    for path, tty, port in panels:
        if sel in (path, tty, os.path.basename(path), SIDE_BY_USB_PORT.get(port)):
            return path
    sys.exit(f"no panel matching {sel!r}; run `smoke.py probe`")


def main():
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    panels = find_panels()
    if not panels:
        sys.exit("no LED matrix modules found (looked for 32ac:0020 on /dev/serial/by-path/)")

    action = args[0]
    if action == "probe":
        sys.exit(0 if cmd_probe(panels) else 1)
    elif action == "on":
        set_fill(resolve(panels, args[1]), True)
    elif action == "off":
        if len(args) > 1 and args[1] == "all":
            for path, _t, _p in panels:
                set_fill(path, False)
        else:
            set_fill(resolve(panels, args[1]), False)
    elif action == "hold":
        # Hold a panel at one brightness long enough to judge it properly.
        # Re-sends periodically because the firmware idle timer (~60s) would
        # otherwise sleep the panel mid-look.
        path = resolve(panels, args[1] if len(args) > 1 else "left")
        level = int(args[2]) if len(args) > 2 else 1
        secs = float(args[3]) if len(args) > 3 else 20.0
        fd = open_raw(path)
        try:
            send(fd, CMD_SLEEP, bytes([0]))
            time.sleep(0.4)
            send(fd, CMD_BRIGHTNESS, bytes([level]))
            send(fd, CMD_DRAW_BW, b"\xFF" * BITMAP_BYTES)
            print(f"  holding at {level}/255 for {secs:.0f}s", flush=True)
            end = time.monotonic() + secs
            while time.monotonic() < end:
                time.sleep(min(15.0, max(0.0, end - time.monotonic())))
                send(fd, CMD_BRIGHTNESS, bytes([level]))   # keepalive
            send(fd, CMD_BRIGHTNESS, bytes([0]))
            send(fd, CMD_DRAW_BW, bytes(BITMAP_BYTES))
        finally:
            os.close(fd)
    elif action == "ramp":
        # Empirical brightness calibration. Constants for "dim enough in the
        # dark" and "bright enough to read" are a property of the room and the
        # person, not something to guess in code — so step through the range
        # and let the human pick the endpoints.
        path = resolve(panels, args[1] if len(args) > 1 else "left")
        fd = open_raw(path)
        try:
            send(fd, CMD_SLEEP, bytes([0]))
            time.sleep(0.4)
            send(fd, CMD_DRAW_BW, b"\xFF" * BITMAP_BYTES)
            for level in (1, 2, 4, 8, 16, 32, 64, 128, 255):
                print(f"  brightness {level:3}/255", flush=True)
                send(fd, CMD_BRIGHTNESS, bytes([level]))
                time.sleep(2.5)
            send(fd, CMD_BRIGHTNESS, bytes([0]))
            send(fd, CMD_DRAW_BW, bytes(BITMAP_BYTES))
        finally:
            os.close(fd)
    elif action == "sweep":
        for i, (path, tty, port) in enumerate(panels):
            print(f"lighting [{i}] {tty} (usb port {port}) — which side is lit?")
            set_fill(path, True)
            time.sleep(2)
            set_fill(path, False)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
