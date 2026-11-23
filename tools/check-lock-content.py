"""Linux lock/unlock output fixture; needs bubblewrap and a debug daemon build."""
import os
from pathlib import Path
import runpy
import tempfile
import time

ipc = runpy.run_path(str(Path(__file__).with_name("check-preview-apply.py")))


def expect(endpoint, locked, intensities):
    with ipc["connection"](endpoint) as stream:
        ipc["message"](stream, {"type": "subscribe"})
        end = time.monotonic() + 8
        while time.monotonic() < end:
            s = ipc["receive"](stream)["snapshot"]
            if s.get("session_locked") == locked and [
                max(p["frame"]["pixels"]) for p in s["panels"]
            ] == intensities:
                return
    raise AssertionError((locked, intensities))


def run(root):
    # Simulate only the compositor query. Real daemon, renderer, config and IPC.
    stub = root / "hyprctl"
    stub.write_text('#!/bin/sh\n[ "$1" = locked ] && cat "$LOCK_FIXTURE"\n')
    stub.chmod(0o700)
    state = root / "locked"
    os.environ.update(PATH=f"{root}:{os.environ['PATH']}", XDG_CURRENT_DESKTOP="Hyprland", LOCK_FIXTURE=str(state))
    state.write_text("false")
    with ipc["daemon"](root) as endpoint:
        request = ipc["request"]
        config = request(endpoint, "read_config")["saved"]["config"]
        left, right = config["panels"]
        left["slots"][1] = None
        right.update(template="full_gauge", slots=[None], when_locked="clock")
        left["when_locked"] = "blank"
        def apply(revision):
            assert request(endpoint, "apply_config", expected_revision=revision, config=config)["status"] == "applied"
        apply(0)
        expect(endpoint, False, [180, 0])
        state.write_text("true")
        expect(endpoint, True, [0, 180])
        config["clock_intensity"] = 123
        apply(1)
        expect(endpoint, True, [0, 123])
        state.write_text("false")
        expect(endpoint, False, [123, 0])
    print("PASS: independent locked content and latest saved layout after unlock")


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="matrix-lock-") as directory:
        run(Path(directory))
