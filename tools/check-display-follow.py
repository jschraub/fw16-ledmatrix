"""Run after cargo build -p matrix-daemon on Linux with bubblewrap installed."""
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import time

ipc = runpy.run_path(str(Path(__file__).with_name("check-preview-apply.py")))
request = ipc["request"]


def verify(endpoint, expected):
    with ipc["connection"](endpoint) as stream:
        ipc["message"](stream, {"type": "subscribe"})
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            snapshot = ipc["receive"](stream)["snapshot"]
            levels = [p["brightness"] for p in snapshot["panels"]]
            if levels == expected:
                return snapshot
        raise AssertionError(f"expected {expected}, last output {levels}")


def run(root):
    drm = root / "drm"
    status = drm / "card7-eDP-2/status"
    status.parent.mkdir(parents=True)
    status.write_text("connected\n")
    dpms = status.with_name("dpms")
    dpms.write_text("On")
    (drm / "card7").mkdir()
    (root / "fixture-gpu").mkdir()
    (drm / "card7/device").symlink_to(root / "fixture-gpu")
    lights = root / "backlight"
    for name, actual in [("selected", "55"), ("other", "100")]:
        light = lights / name
        light.mkdir(parents=True)
        (light / "max_brightness").write_text("100")
        (light / "actual_brightness").write_text(actual)
    endpoint = root / "ipc/test.sock"
    binary = Path(__file__).resolve().parents[1] / "target/debug/matrix-daemon"
    proc = subprocess.Popen(["bwrap", "--die-with-parent", "--ro-bind", "/", "/", "--dev", "/dev",
                             "--tmpfs", "/sys/class/tty", "--bind", root, root,
                             "--ro-bind", drm, "/sys/class/drm", "--ro-bind", lights, "/sys/class/backlight",
                             "--", binary], env=dict(os.environ, MATRIX_ENDPOINT=str(endpoint),
                             MATRIX_CONFIG=str(root / "config.json"), MATRIX_FEEDS=str(root / "feeds")))
    try:
        deadline = time.monotonic() + 5
        while not endpoint.exists():
            assert proc.poll() is None and time.monotonic() < deadline
            time.sleep(.05)
        assert request(endpoint, "devices")["devices"] == []
        saved = request(endpoint, "read_config")["saved"]
        config = saved["config"]
        assert not config.get("follow_display")
        config["brightness"].update(minimum=10, maximum=110, manual=32)
        config["panels"][1]["brightness_override"] = dict(config["brightness"], manual=40)
        config["follow_display"] = {"connector": "eDP-2@fixture-gpu", "backlight": "selected"}
        reply = request(endpoint, "apply_config", expected_revision=0, config=config)
        assert reply["status"] == "applied"
        verify(endpoint, [65, 40])
        config["follow_display"]["curve"] = "quadratic"
        config["sleep_with_display"] = "eDP-2@fixture-gpu"
        assert request(endpoint, "apply_config", expected_revision=1, config=config)["status"] == "applied"
        verify(endpoint, [40, 40])  # 10 + round(100 * .55²)
        dpms.write_text("Off")
        verify(endpoint, [None, None])
        config["panels"][1]["brightness_override"]["manual"] = 45
        assert request(endpoint, "apply_config", expected_revision=2, config=config)["status"] == "applied"
        dpms.write_text("On")
        verify(endpoint, [40, 45])
        config["panels"][1]["brightness_override"]["manual"] = 40
        assert request(endpoint, "apply_config", expected_revision=3, config=config)["status"] == "applied"
        actual = lights / "selected/actual_brightness"
        actual.write_text("0")
        verify(endpoint, [10, 40])
        actual.unlink()
        verify(endpoint, [32, 40])
        actual.write_text("100")
        verify(endpoint, [110, 40])
        status.write_text("disconnected")
        verify(endpoint, [32, 40])
        status.write_text("connected")
        verify(endpoint, [110, 40])
        config["brightness"]["off"] = True
        assert request(endpoint, "apply_config", expected_revision=4, config=config)["status"] == "applied"
        verify(endpoint, [None, 40])
        assert request(endpoint, "read_config")["saved"]["config"] == config
        print("PASS: selected pair, zero, lost reading, disconnect, recovery, override, sleep and durable manual fallback")
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="matrix-display-") as directory:
        run(Path(directory))
