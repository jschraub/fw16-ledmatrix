"""Measure manual-mode brightness changes to the daemon's right-panel frames."""
import os
from pathlib import Path
import runpy
import tempfile
import time

ipc = runpy.run_path(str(Path(__file__).with_name("check-preview-apply.py")))


def run(root):
    light = root / "lights/panel"
    light.mkdir(parents=True)
    (light / "max_brightness").write_text("100")
    actual = light / "actual_brightness"
    actual.write_text("40")
    os.environ["XDG_RUNTIME_DIR"] = str(root)
    with ipc["daemon"](root, ["--ro-bind", light.parent, "/sys/class/backlight"]) as endpoint:
        saved = ipc["request"](endpoint, "read_config")["saved"]
        cfg = saved["config"]
        panel = next(p for p in cfg["panels"] if p["name"] == "right")
        panel.update(template="clock_gauge", slots=[cfg["panels"][0]["slots"][0], None],
                     feedback={"brightness": 3})
        assert not cfg.get("follow_display")
        assert ipc["request"](endpoint, "apply_config", config=cfg, expected_revision=saved["revision"])["status"] == "applied"
        with ipc["connection"](endpoint) as stream:
            ipc["message"](stream, {"type": "subscribe"})
            def frame():
                snap = ipc["receive"](stream)["snapshot"]
                return snap, next(p for p in snap["panels"] if p["name"] == "right")
            for _ in range(8):
                snap, right = frame()
                if snap.get("display_sources", {}).get("backlights", {}).get("panel") == 40 and max(right["frame"]["pixels"]) == 180:
                    break
            else:
                raise AssertionError("No initial brightness baseline")
            for percent in [75, 25, 50]:
                start = time.monotonic()
                actual.write_text(str(percent))
                rows = (34 * percent + 50) // 100
                while time.monotonic() - start < 5:
                    _, right = frame()
                    assert right["brightness"] == 32
                    if right["frame"]["pixels"] == [0]*((34-rows)*9) + [180]*(rows*9):
                        break
                else:
                    raise AssertionError("No gauge")
                elapsed = time.monotonic() - start
                print(f"Backlight {percent}% to subscriber: {elapsed:.3f}s", flush=True)
                assert elapsed < .4, "brightness feedback waited for a slow tick"


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="matrix-light-") as directory:
        run(Path(directory))
