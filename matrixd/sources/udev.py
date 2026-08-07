"""Kernel device events, over a netlink socket.

Gives the event loop a single pollable fd for two unrelated jobs:

- **`power_supply`** — AC plugged or unplugged, which earns a takeover.
- **`tty`** — a panel appearing or disappearing, which is the authoritative
  signal for `transport.reconnect()`. Without it a dead panel is only noticed at
  the next keepalive up to 30s later, and any takeover fired in that window
  silently does nothing.

Why a raw socket rather than the alternatives: `pyudev` is a third-party
dependency this project does not otherwise need, and parsing `udevadm monitor`
means another child process to supervise. A netlink socket is stdlib-only and is
*already* a file descriptor, which is exactly what an epoll loop wants.

**Events say "something changed", not "here is the new state."** Callers should
re-read the authoritative source rather than trusting event properties. That is
not laziness: `remove` events carry far fewer properties than `add` ones — the
device is gone, so udev has little left to report — and matching a removal by
`ID_VENDOR_ID` would work in testing and fail in production. Re-running
`transport.discover()` is both simpler and correct.

Binding is unprivileged. Group 2 (udev) is used rather than group 1 (kernel)
because group 2 fires *after* rules have been processed, which matters when the
thing you are waiting for is a udev rule applying an ACL.
"""

from __future__ import annotations

import errno
import os
import socket
import struct
from dataclasses import dataclass

NETLINK_KOBJECT_UEVENT = 15
UDEV_MONITOR_GROUP = 2  # post-rule-processing; group 1 is raw kernel events

_LIBUDEV_PREFIX = b"libudev\0"
_MAGIC = 0xFEEDCAFE
_HEADER_LEN = 40

# Kernel events are dropped on the floor if the receive buffer fills. A missed
# event is survivable — the next one triggers a full reconcile — but a large
# buffer makes it unlikely in the first place.
RECV_BUFFER = 1 << 20


@dataclass(frozen=True)
class Event:
    action: str  # "add", "remove", "change", "bind", "unbind"
    subsystem: str
    devpath: str
    properties: dict[str, str]

    @property
    def device_name(self) -> str | None:
        """e.g. "ttyACM0". Present on add/change, often absent on remove."""
        return self.properties.get("DEVNAME") or os.path.basename(self.devpath) or None


def parse_message(data: bytes) -> Event | None:
    """Decode one netlink datagram. Pure, so the wire format is testable.

    Returns None for anything not recognised — including kernel-group messages,
    which use a different framing entirely ("ACTION@DEVPATH\\0" then properties).
    """
    if len(data) < _HEADER_LEN or not data.startswith(_LIBUDEV_PREFIX):
        return None
    # Magic is big-endian; the remaining header fields are native-endian.
    (magic,) = struct.unpack(">I", data[8:12])
    if magic != _MAGIC:
        return None
    _, _, props_off, props_len = struct.unpack("=IIII", data[8:24])
    if props_off + props_len > len(data):
        return None

    properties: dict[str, str] = {}
    for raw in data[props_off : props_off + props_len].split(b"\0"):
        if not raw:
            continue
        key, _, value = raw.decode("utf-8", "replace").partition("=")
        if key:
            properties[key] = value

    action = properties.get("ACTION")
    if not action:
        return None
    return Event(
        action=action,
        subsystem=properties.get("SUBSYSTEM", ""),
        devpath=properties.get("DEVPATH", ""),
        properties=properties,
    )


class Watcher:
    """Netlink uevent listener, filtered to the subsystems you care about."""

    def __init__(self, subsystems: set[str] | None = None) -> None:
        self.subsystems = subsystems or {"tty", "power_supply"}
        self._sock = socket.socket(
            socket.AF_NETLINK, socket.SOCK_DGRAM, NETLINK_KOBJECT_UEVENT
        )
        self._sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, RECV_BUFFER)
        self._sock.bind((0, UDEV_MONITOR_GROUP))
        self._sock.setblocking(False)

    def fileno(self) -> int:
        """For select/epoll/asyncio.add_reader."""
        return self._sock.fileno()

    def read_events(self) -> list[Event]:
        """Drain everything pending. Never blocks, never raises.

        Returns only events in the configured subsystems. Filtering in Python
        rather than with a socket BPF filter: the volume is a handful of events
        per minute, so the kernel-side optimisation would buy nothing and cost
        a chunk of unreadable bytecode.
        """
        events: list[Event] = []
        while True:
            try:
                data = self._sock.recv(65536)
            except OSError as exc:
                if exc.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                    break
                if exc.errno == errno.ENOBUFS:
                    # Buffer overran and the kernel dropped events. Keep
                    # draining: the caller reconciles from authoritative state,
                    # so a gap costs nothing beyond one late reconcile.
                    continue
                break
            event = parse_message(data)
            if event is not None and event.subsystem in self.subsystems:
                events.append(event)
        return events

    def close(self) -> None:
        self._sock.close()

    def __enter__(self) -> Watcher:
        return self

    def __exit__(self, *_exc: object) -> None:
        self.close()


def affects_panels(event: Event) -> bool:
    """Whether this event should trigger a panel reconcile.

    Any tty add/remove qualifies. Deliberately does not try to identify the
    device: `remove` events carry almost no properties, so matching on
    ID_VENDOR_ID would pass in testing and fail when a panel actually vanished.
    The caller re-runs discover() and reconciles against reality.
    """
    return event.subsystem == "tty" and event.action in ("add", "remove", "bind", "unbind")


def affects_power(event: Event) -> bool:
    """Whether this event should trigger a re-read of battery and AC state."""
    return event.subsystem == "power_supply"
