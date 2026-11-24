"""Linux native #29 smoke: real desktop + private zero-panel daemon, no login changes.

Requires the release no-bundle desktop, debug daemon, bubblewrap, PyGObject,
AT-SPI and a graphical session. Uses accessible actions in this window only.
"""
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import time

import gi
gi.require_version("Atspi", "2.0")
from gi.repository import Atspi

ipc = runpy.run_path(str(Path(__file__).with_name("check-preview-apply.py")))


def find(pid, predicate):
    def walk(node):
        if predicate(node):
            return node
        for i in range(node.get_child_count()):
            child = node.get_child_at_index(i)
            if child is not None:
                found = walk(child)
                if found:
                    return found
        return None

    desktop = Atspi.get_desktop(0)
    end = time.monotonic() + 10
    while time.monotonic() < end:
        for i in range(desktop.get_child_count()):
            app = desktop.get_child_at_index(i)
            if app and app.get_process_id() == pid:
                found = walk(app)
                if found:
                    return found
        time.sleep(.1)
    raise AssertionError("Expected accessible control did not appear in the isolated desktop")


def run(root):
    binary = Path(__file__).resolve().parents[1] / "target/release/matrix-desktop"
    with ipc["daemon"](root) as endpoint:
        with subprocess.Popen([binary], env=dict(os.environ, MATRIX_ENDPOINT=str(endpoint),
                              NO_AT_BRIDGE="0", GTK_MODULES="atk-bridge", WEBKIT_FORCE_SANDBOX="1")) as app:
            def click(name):
                button = find(app.pid, lambda n: (n.get_name() or "").casefold() == name.casefold()
                              and n.get_role() in (Atspi.Role.PUSH_BUTTON, Atspi.Role.TOGGLE_BUTTON))
                assert button.do_action(0), name
            try:
                click("Workspace")
                click("Configuration")
                # Change the layout by removing the second simulated panel.
                click("right panel")
                click("Remove right")
                click("Save changes")
                end = time.monotonic() + 5
                while time.monotonic() < end:
                    saved = ipc["request"](endpoint, "read_config")["saved"]
                    if saved["revision"] == 1:
                        break
                    time.sleep(.05)
                assert [p["name"] for p in saved["config"]["panels"]] == ["left"]
                assert json.loads((root / "config.json").read_text()) == saved
                # Apply commits before the next render tick; wait for the
                # subscription (including the desktop) to see the new layout.
                with ipc["connection"](endpoint) as stream:
                    ipc["message"](stream, {"type": "subscribe"})
                    for _ in range(20):
                        names = [p["name"] for p in ipc["receive"](stream)["snapshot"]["panels"]]
                        if names == ["left"]:
                            break
                    assert names == ["left"]
                click("Workspace")
                click("Diagnostics")
                click("Generate redacted report")
                box = find(app.pid, lambda n: "Redacted report (JSON)" in (n.get_name() or "") and n.is_text())
                data = json.loads(Atspi.Text.get_text(box, 0, -1))
                assert data["connection"] == "connected" and data["deviceScan"] == 0
                assert data["panels"] == [{"role":"left","state":"unassigned"}]
                print("PASS: native desktop applied and persisted an isolated layout; generated zero-panel support report")
            finally:
                app.terminate()


if __name__ == "__main__":
    os.environ.update(MATRIX_USAGE_OPENAI="0", MATRIX_USAGE_CLAUDE="0")
    with tempfile.TemporaryDirectory(prefix="matrix-desktop-smoke-") as work:
        run(Path(work))
