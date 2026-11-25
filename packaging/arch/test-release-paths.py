"""Exercise the publication gate on the package format used by makepkg."""

import gzip
import io
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

CHECK = Path(__file__).with_name("check-release-paths.py")


class ReleasePathsTest(unittest.TestCase):
    def scan(self, name, data):
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w") as tar:
            item = tarfile.TarInfo(name)
            item.size = len(data)
            tar.addfile(item, io.BytesIO(data))
        with tempfile.TemporaryDirectory() as work:
            package = Path(work) / "candidate.pkg.tar.zst"
            subprocess.run(["zstd", "-qo", package], input=archive.getvalue(), check=True)
            return subprocess.run([sys.executable, CHECK, package], capture_output=True)

    def test_private_paths_rejected_without_logging_values(self):
        for name, data in (
            ("usr/bin/example", b"\0/home/private-test-user/crate\0"),
            ("home/private-test-user/config", b""),
            (".MTREE", gzip.compress(b"./home/private-test-user/config")),
        ):
            with self.subTest(member=name):
                result = self.scan(name, data)
                self.assertEqual(result.returncode, 1)
                self.assertNotIn(b"private-test-user", result.stdout + result.stderr)

    def test_ci_builder_path_is_accepted(self):
        self.assertEqual(self.scan("usr/bin/example", b"\0/home/builder/source\0").returncode, 0)


if __name__ == "__main__":
    unittest.main()
