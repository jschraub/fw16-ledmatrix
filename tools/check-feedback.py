"""Linux #27 IPC/output fixture. Requires bubblewrap and a debug daemon build."""
from concurrent.futures import ThreadPoolExecutor
import os
from pathlib import Path
import runpy
import tempfile
import time

ipc = runpy.run_path(str(Path(__file__).with_name("check-preview-apply.py")))


def run(root):
    state = root / "volume"
    fifo = root / "events"
    os.mkfifo(fifo)
    stub = root / "pactl"
    stub.write_text("""#!/bin/sh
case "$1" in
subscribe) exec cat "$EVENT_FIXTURE";;
get-sink-volume) read v < "$VOLUME_FIXTURE" || exit 1; echo "Volume: mono: 1 / $v% / -3 dB";;
get-sink-mute) echo 'Mute: no';;
*) exit 1;;
esac
""")
    stub.chmod(0o700)
    os.environ.update(PATH=f"{root}:{os.environ['PATH']}", VOLUME_FIXTURE=str(state), EVENT_FIXTURE=str(fifo))
    state.write_text("17\n")
    with ipc["daemon"](root) as endpoint, os.fdopen(os.open(fifo, os.O_RDWR | os.O_NONBLOCK), "w") as events:
        def req(kind, **args):
            return ipc["request"](endpoint, kind, **args)
        cfg = req("read_config")["saved"]["config"]
        panel = cfg["panels"][0]
        panel["slots"][1] = None
        panel["feedback"] = {"volume": 4}
        def apply(revision):
            assert req("apply_config", expected_revision=revision, config=cfg)["status"] == "applied"
        def expect(ambient, preview=False, count=None):
            with ipc["connection"](endpoint) as stream:
                ipc["message"](stream, {"type": "subscribe"})
                end = time.monotonic() + 7
                while time.monotonic() < end:
                    snap = ipc["receive"](stream)["snapshot"]
                    pixels = snap["frame"]["pixels"]
                    if snap["preview"] != preview:
                        continue
                    if count is not None and pixels.count(180) == count:
                        return
                    if count is None and max(pixels) == ambient and not any(pixels[12*9:]):
                        return
            raise AssertionError("Expected subscriber output not observed")
        apply(0)
        expect(180)
        time.sleep(2)
        for value, rows in [(50, 17), (25, 9), (75, 26)]:
            start = time.monotonic()
            state.write_text(f"{value}\n")
            events.write("Event 'change' on sink #1\n")
            events.flush()
            expect(None, count=rows*5)
            elapsed = time.monotonic() - start
            print(f"Volume {value}% to subscriber gauge: {elapsed:.3f}s", flush=True)
            assert elapsed < .4, "volume feedback waited for an ambient render tick"
        cfg["clock_intensity"] = 111
        apply(1)
        expect(111)  # a constant reading must not extend the gauge
        state.unlink()
        time.sleep(2)
        state.write_text("85\n")
        time.sleep(2)
        expect(111)  # recovery is a baseline, not a volume edge
        state.write_text("100\n")
        expect(None, count=34*5)
        with ThreadPoolExecutor() as pool:
            lease = pool.submit(req, "begin_preview", owner="fixture", config=cfg, seconds=5)
            expect(111, preview=True)
            state.write_text("25\n")
            assert lease.result(timeout=7)["status"] == "ended"
        expect(111)
        print("PASS: actual audio change, stable input expiry, latest ambient, source recovery, preview priority")


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="matrix-feedback-") as directory:
        run(Path(directory))
