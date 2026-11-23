"""Linux daemon fixture: logind events, interrupted preview, latest saved output.
Run after cargo build -p matrix-daemon; uses bubblewrap, no physical modules.
"""
import concurrent.futures
import os
from pathlib import Path
import runpy
import tempfile
import time

ipc = runpy.run_path(str(Path(__file__).with_name("check-preview-apply.py")))


def expect(endpoint, epoch, preview, level):
    with ipc["connection"](endpoint) as stream:
        ipc["message"](stream, {"type": "subscribe"})
        deadline = time.monotonic() + 6
        while time.monotonic() < deadline:
            snap = ipc["receive"](stream)["snapshot"]
            if (snap["suspend_epoch"], snap["preview"]) == (epoch, preview) and all(
                p["brightness"] == level for p in snap["panels"]
            ) and snap["panels"]:
                return
    raise AssertionError((epoch, preview, level))


def run(root):
    fifo = root / "signals"
    os.mkfifo(fifo)
    stub = root / "gdbus"
    stub.write_text('#!/bin/sh\necho "The name org.freedesktop.login1 is owned by :1.1"\n'
                    'while IFS= read -r line; do [ "$line" = RESTART ] && exit; echo "$line"; done < "$SIGNALS"\n')
    stub.chmod(0o700)
    busctl = root / "busctl"
    busctl.write_text('#!/bin/sh\ncat "$PREPARING"\n')
    busctl.chmod(0o700)
    state = root / "preparing"
    state.write_text("b false")
    os.environ.update(PATH=f"{root}:{os.environ['PATH']}", SIGNALS=str(fifo), PREPARING=str(state))
    with ipc["daemon"](root) as endpoint, os.fdopen(os.open(fifo, os.O_RDWR | os.O_NONBLOCK), "w") as events:
        def sleep_signal(value):
            events.write(f"/org/freedesktop/login1: org.freedesktop.login1.Manager.PrepareForSleep ({value},)\n")
            events.flush()
        def req(kind, **args):
            return ipc["request"](endpoint, kind, **args)
        config = req("read_config")["saved"]["config"]
        config["brightness"]["manual"] = 61
        with concurrent.futures.ThreadPoolExecutor() as pool:
            lease = pool.submit(req, "begin_preview", owner="fixture", config=config, seconds=15)
            expect(endpoint, 0, True, 61)
            sleep_signal("true")
            expect(endpoint, 1, False, 32)
            assert lease.result(timeout=3)["status"] == "ended"
        assert req("begin_preview", owner="fixture", config=config, seconds=1)["status"] == "busy"
        config["brightness"]["manual"] = 45
        assert req("apply_config", expected_revision=0, config=config)["status"] == "applied"
        sleep_signal("true")  # repeated notifications do not add epochs
        sleep_signal("false")
        expect(endpoint, 2, False, 45)
        sleep_signal("true")
        expect(endpoint, 3, False, 45)
        events.write("RESTART\n")  # lose wake edge; restarted monitor must re-read PreparingForSleep
        events.flush()
        expect(endpoint, 4, False, 45)
        print("PASS: suspend cancels preview, gates new lease, resume uses latest saved output; monitor restart resyncs")


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="matrix-suspend-") as root:
        run(Path(root))
