"""Measure Linux power-supply event to right-panel AC gauge (not optical latency)."""
import os
from pathlib import Path
import runpy
import tempfile
import time

ipc = runpy.run_path(str(Path(__file__).with_name("check-preview-apply.py")))


def run(root):
    supply = root / "supply"
    for name, files in [("AC", {"type": "Mains", "online": "1"}),
                        ("BAT", {"type": "Battery", "capacity": "50", "status": "Full"})]:
        path = supply / name
        path.mkdir(parents=True)
        for key, value in files.items():
            (path / key).write_text(value)
    fifo = root / "events"
    os.mkfifo(fifo)
    monitor = root / "udevadm"
    monitor.write_text('#!/bin/sh\nexec cat "$AC_EVENTS"\n')
    monitor.chmod(0o700)
    os.environ.update(PATH=f"{root}:{os.environ['PATH']}", AC_EVENTS=str(fifo))
    with ipc["daemon"](root, ["--ro-bind", supply, "/sys/class/power_supply"]) as endpoint, \
            os.fdopen(os.open(fifo, os.O_RDWR | os.O_NONBLOCK), "w") as events:
        saved = ipc["request"](endpoint, "read_config")["saved"]
        cfg = saved["config"]
        right = next(p for p in cfg["panels"] if p["name"] == "right")
        right.update(template="clock_gauge", slots=[cfg["panels"][0]["slots"][0], None],
                     feedback={"ac": 3})
        assert ipc["request"](endpoint, "apply_config", expected_revision=saved["revision"], config=cfg)["status"] == "applied"
        with ipc["connection"](endpoint) as stream:
            ipc["message"](stream, {"type": "subscribe"})
            def until(test):
                deadline = time.monotonic() + 8
                while time.monotonic() < deadline:
                    s = ipc["receive"](stream)["snapshot"]
                    p = next((p for p in s["panels"] if p["name"] == "right"), None)
                    if p and s["power"] and test(s["power"], p["frame"]["pixels"]):
                        return
                raise AssertionError("expected panel frame not observed")
            for online in (False, True):
                until(lambda power, px: power["ac"] is not online and max(px) == 180 and not any(px[12*9:]))
                start = time.monotonic()
                (supply / "AC/online").write_text(str(int(online)))
                events.write("KERNEL[1.0] change /devices/fixture/AC (power_supply)\n")
                events.flush()
                until(lambda power, px: power["ac"] is online and px == [0]*153 + ([0]*2+[180]*5+[0]*2)*17)
                elapsed = time.monotonic()-start
                print(f"AC {'connected' if online else 'disconnected'} to gauge: {elapsed:.3f}s", flush=True)
                assert elapsed < .4, "AC feedback waited for a periodic tick"


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="matrix-ac-") as directory:
        run(Path(directory))
