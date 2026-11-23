"""Linux #26 read-only probe fixture. Requires bubblewrap and a debug daemon build."""
import json
import os
from pathlib import Path
import subprocess
import tempfile


def check(root):
    drm = root / "drm"
    connector = drm / "card7-eDP-2"
    connector.mkdir(parents=True)
    (connector / "status").write_text("connected")
    for program, source in [("loginctl", "lock"), ("busctl", "sleep"), ("hyprctl", "compositor")]:
        stub = root / program
        stub.write_text(f'#!/bin/sh\nexec cat "$FIXTURE/{source}"\n')
        stub.chmod(0o700)
    binary = Path(__file__).resolve().parents[1] / "target/debug/matrix-daemon"
    def probe(dpms, lock, sleep, compositor="false"):
        (connector / "dpms").write_text(dpms)
        (root / "lock").write_text(lock)
        (root / "sleep").write_text(sleep)
        (root / "compositor").write_text(compositor)
        raw = subprocess.check_output([
            "bwrap", "--die-with-parent", "--ro-bind", "/", "/", "--dev", "/dev",
            "--tmpfs", "/sys/class/tty", "--ro-bind", drm, "/sys/class/drm",
            "--setenv", "PATH", f"{root}:{os.environ['PATH']}", "--setenv", "FIXTURE", root,
            "--setenv", "XDG_CURRENT_DESKTOP", "Hyprland", "--", binary, "--probe",
        ], timeout=12)
        rows = {c["name"]: (c["state"], c["detail"]) for c in json.loads(raw)["capabilities"]}
        expected = {"true": ("available", "Hyprland reports locked."), "false": ("available", "Hyprland reports unlocked.")}
        assert rows["compositor_lock"] == expected.get(compositor, ("not_verified", "No readable Hyprland lock state."))
        return tuple(rows[k] for k in ("display_power:card7-eDP-2", "session_lock_hint", "preparing_for_sleep"))

    on, clear, awake = probe("On", "no", "b false")
    assert all(row[0] == "available" for row in (on, clear, awake))
    off, locked, sleeping = probe("Off", "yes", "b true")
    assert all(row[0] == "available" for row in (off, locked, sleeping))
    assert on != off and clear != locked and awake != sleeping
    assert probe("Off", "no", "b false") == (off, clear, awake)
    assert probe("On", "yes", "b false") == (on, locked, awake)
    assert probe("On", "no", "b false", "true") == (on, clear, awake)
    assert probe("On", "no", "b false", "") == (on, clear, awake)
    assert probe("On", "no", "b true") == (on, clear, sleeping)
    display_suspend, lock, sleep = probe("Suspend", "yes", "b false")
    assert display_suspend[0] == "available" and "display only" in display_suspend[1]
    assert (lock, sleep) == (locked, awake)
    assert all(row[0] == "not_verified" for row in probe("", "invalid", ""))
    assert probe("On", "no", "b false") == (on, clear, awake)
    print("PASS: independent DPMS, lock hint and sleep preparation; overlap, unknown and recovery")


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="matrix-lifecycle-") as directory:
        check(Path(directory))
