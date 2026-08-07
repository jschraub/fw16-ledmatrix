#!/usr/bin/env python3
"""End-to-end test of the panel disconnect/reconnect path. Requires root.

This is the one part of the robustness story that cannot be tested any other
way. The modules are internal so they cannot be unplugged, pty allocation
produces no tty uevents, and a mocked udev event would only prove the mock
works. Deauthorising the USB device produces a genuine remove/add pair —
the same thing a suspend cycle does.

Deauthorisation is reversible and the device is re-authorised in a finally
block, including on Ctrl-C.

    sudo python3 tools/test-reconnect.py
"""

import os
import select
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from matrixd import render, transport  # noqa: E402
from matrixd.sources import udev  # noqa: E402

USB_DEVICES = "/sys/bus/usb/devices"


def usb_node_for(path: str) -> str | None:
    """Map /dev/serial/by-path/... back to its /sys/bus/usb/devices/N-N.N node."""
    tty = os.path.basename(os.path.realpath(path))
    usb_iface = os.path.realpath(f"/sys/class/tty/{tty}/device")
    return os.path.basename(os.path.dirname(usb_iface)) or None


def authorize(node: str, value: int) -> None:
    with open(os.path.join(USB_DEVICES, node, "authorized"), "w") as f:
        f.write(str(value))


def collect(watcher: udev.Watcher, seconds: float) -> list[udev.Event]:
    events: list[udev.Event] = []
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if select.select([watcher], [], [], max(0.0, end - time.monotonic()))[0]:
            events += watcher.read_events()
    return events


def main() -> int:
    if os.geteuid() != 0:
        sys.exit("needs root: sudo python3 tools/test-reconnect.py")

    found = transport.discover()
    if "left" not in found:
        sys.exit(f"left panel not found (discovered: {sorted(found)})")

    path = found["left"]
    node = usb_node_for(path)
    if not node:
        sys.exit(f"could not resolve a USB node for {path}")
    print(f"target: left panel, {path}\n        usb node {node}")

    panel = transport.Panel("left", path)
    if not panel.open():
        sys.exit("could not open the panel")
    panel.wake()
    panel.set_brightness(20)
    panel.draw(render.render_gauge(1.0), fast=True)
    print("  panel lit\n")

    watcher = udev.Watcher(subsystems={"tty"})
    collect(watcher, 0.3)  # clear anything pending
    failures = 0

    try:
        print("1. deauthorising (simulates the disconnect)")
        authorize(node, 0)
        events = collect(watcher, 3.0)
        removes = [e for e in events if e.action == "remove"]
        print(f"   tty events: {[e.action for e in events]}")
        if removes and any(udev.affects_panels(e) for e in removes):
            print("   udev reported the removal")
        else:
            print("   !! no remove event seen"); failures += 1

        print("2. writing to the dead panel (the backstop)")
        try:
            for _ in range(50):
                panel.set_brightness(21)
                panel.draw(render.blank())
            print("   !! writes kept succeeding — backstop did not fire"); failures += 1
        except transport.PanelGone as exc:
            print(f"   PanelGone raised: {exc}")
            print(f"   panel.connected == {panel.connected}")
            if panel.connected:
                print("   !! still marked connected"); failures += 1

    finally:
        print("3. re-authorising")
        authorize(node, 1)
        time.sleep(2.5)

    events = collect(watcher, 3.0)
    adds = [e for e in events if e.action == "add"]
    print(f"   tty events: {[e.action for e in events]}")
    if adds:
        print("   udev reported the device returning")
    else:
        print("   !! no add event seen"); failures += 1

    print("4. reconnecting")
    # by-path can take a moment to reappear after re-enumeration
    for _ in range(20):
        if "left" in transport.discover():
            break
        time.sleep(0.25)
    panel.path = transport.discover().get("left", panel.path)
    if panel.reconnect(brightness=20):
        print(f"   reconnected, connected={panel.connected}")
        panel.draw(render.render_gauge(0.5), fast=True)
        print("   repainted at half — panel should show a half bar")
        time.sleep(3)
        panel.sleep()
    else:
        print("   !! reconnect failed"); failures += 1

    panel.close()
    watcher.close()
    print(f"\n{'PASS' if failures == 0 else f'FAIL ({failures} problem(s))'}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
