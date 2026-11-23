"""Linux qualification fixture for #38. Run after cargo build -p matrix-daemon.

Needs bubblewrap and Python 3. Starts only private daemons and IPC clients.
"""
import contextlib
import copy
import json
import multiprocessing
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import time


def message(stream, value):
    data = json.dumps(value).encode()
    stream.write(struct.pack(">I", len(data)) + data)
    stream.flush()


def receive(stream):
    size, = struct.unpack(">I", stream.read(4))
    assert 0 < size <= 65536
    return json.loads(stream.read(size))


@contextlib.contextmanager
def connection(endpoint):
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(20)
        sock.connect(str(endpoint))
        with sock.makefile("rwb") as stream:
            message(stream, {"type": "hello", "protocol_version": 2})
            assert receive(stream)["type"] == "hello"
            yield stream


def request(endpoint, kind, **fields):
    with connection(endpoint) as stream:
        message(stream, dict(type=kind, **fields))
        return receive(stream)["reply"]


@contextlib.contextmanager
def daemon(root, mounts=()):
    endpoint = root / "ipc/daemon.sock"
    binary = Path(__file__).resolve().parents[1] / "target/debug/matrix-daemon"
    env = dict(os.environ, MATRIX_ENDPOINT=str(endpoint), MATRIX_CONFIG=str(root / "config.json"),
               MATRIX_FEEDS=str(root / "feeds"))
    proc = subprocess.Popen(["bwrap", "--die-with-parent", "--ro-bind", "/", "/",
                             "--dev", "/dev", "--tmpfs", "/sys/class/tty",
                             "--bind", root, root, *mounts, "--", binary], env=env)
    try:
        deadline = time.monotonic() + 5
        while True:
            try:
                assert request(endpoint, "devices")["devices"] == []
                break
            except (FileNotFoundError, ConnectionRefusedError):
                if proc.poll() is not None or time.monotonic() >= deadline:
                    raise
                time.sleep(0.05)
        yield endpoint
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()


def owner_request(endpoint, config, seconds):
    assert request(endpoint, "begin_preview", owner="fixture", config=config,
                   seconds=seconds)["status"] == "ended"


def output(endpoint):
    # A new subscription returns the most recent snapshot, not queued old frames.
    with connection(endpoint) as stream:
        message(stream, {"type": "subscribe"})
        return receive(stream)["snapshot"]


def await_frame(endpoint, preview, level, intensity, binding, after=-1):
    with connection(endpoint) as stream:
        message(stream, {"type": "subscribe"})
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            frame = receive(stream)["snapshot"]
            if not frame["panels"] or frame["preview"] != preview or frame["sequence"] <= after:
                continue
            panel, = frame["panels"]
            assert panel["brightness"] == level and panel["binding"] == binding, panel["name"]
            assert max(panel["frame"]["pixels"]) == intensity
            return frame["sequence"]
    raise AssertionError("expected output not observed")


def scenario(root, ending):
    with daemon(root) as endpoint:
        baseline = request(endpoint, "read_config")["saved"]
        assert baseline["revision"] == 0
        draft = copy.deepcopy(baseline["config"])
        draft["panels"] = draft["panels"][:1]
        panel, = draft["panels"]
        panel["slots"][1] = None  # Clock only: no dependency on host battery/feeds.
        panel["binding"] = "fixture-old-bay"
        draft["clock_intensity"] = 90
        draft["brightness"]["manual"] = 16
        latest = copy.deepcopy(draft)
        latest["brightness"].update(manual=64, off=ending == "cancel")
        latest["clock_intensity"] = 210
        latest["panels"][0]["binding"] = "fixture-new-bay"
        seconds = 6 if ending == "expiry" else 15
        owner = multiprocessing.get_context("spawn").Process(
            target=owner_request, args=(endpoint, draft, seconds))
        started = time.monotonic()
        owner.start()
        try:
            await_frame(endpoint, True, 16, 90, "fixture-old-bay")
            observed = time.monotonic()
            assert request(endpoint, "read_config")["saved"] == baseline
            applied = request(endpoint, "apply_config", expected_revision=0, config=latest)
            assert applied["status"] == "applied"
            saved = {"revision": 1, "config": latest}
            assert request(endpoint, "read_config")["saved"] == saved
            # Require a new render tick after Apply, not the cached pre-Apply frame.
            await_frame(endpoint, True, 16, 90, "fixture-old-bay", output(endpoint)["sequence"])
            if ending == "cancel":
                assert request(endpoint, "cancel_preview", owner="fixture")["status"] == "cancelled"
            elif ending == "kill":
                owner.kill()
            owner.join(seconds + 2)
            assert owner.exitcode == (-9 if ending == "kill" else 0)
            expected = (None, 0) if ending == "cancel" else (64, 210)
            await_frame(endpoint, False, *expected, "fixture-new-bay")
            restored = time.monotonic()
            if ending == "expiry":
                assert restored - started >= seconds, "lease ended prematurely"
            else:
                assert restored - started < seconds, "release waited for expiry"
        finally:
            if owner.is_alive():
                owner.kill()
            owner.join()
    with daemon(root) as endpoint:
        assert request(endpoint, "read_config")["saved"] == saved
        await_frame(endpoint, False, *expected, "fixture-new-bay")
    print(f"{ending}: preview survived Apply; latest frame observed "
          f"{restored - observed:.2f}s after first preview frame; revision 1 survived restart", flush=True)


if __name__ == "__main__":
    for ending in ("expiry", "cancel", "kill"):
        with tempfile.TemporaryDirectory(prefix="matrix-preview-") as directory:
            scenario(Path(directory), ending)
