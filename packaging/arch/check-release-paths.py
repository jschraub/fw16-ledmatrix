"""Check the unpacked Arch candidate for Linux home paths before publication."""

import argparse
import gzip
import re
import subprocess
import tarfile
import tempfile
from pathlib import Path


# The CI build account is deliberately generic. The integration guide ships
# one literal example for configuring a local command.
HOME = re.compile(rb"/home/([^/\x00\s\"'<>:]+)")


def personal_path(data: bytes, member: str) -> bool:
    if member == "usr/share/doc/fw16-ledmatrix/INTEGRATIONS.md":
        data = data.replace(b"/home/you/.claude/statusline.sh", b"<example>")
    return any(match[1] != b"builder" for match in HOME.finditer(data))


def check(package: Path) -> bool:
    clean = True
    with tempfile.TemporaryFile() as unpacked:
        subprocess.run(["zstd", "-dc", "--", package], stdout=unpacked, check=True)
        unpacked.seek(0)
        with tarfile.open(fileobj=unpacked, mode="r:") as archive:
            for member in archive:
                name = member.name.removeprefix("./")
                # Inspect archive metadata as well as stripped executables.
                metadata = ("/" + name + "\0" + member.linkname).encode()
                data = archive.extractfile(member).read() if member.isfile() else b""
                if name == ".MTREE":
                    data = gzip.decompress(data)
                if personal_path(metadata, "") or personal_path(data, name):
                    # An archive member name can itself carry the private path.
                    print(f"Rejected: home path in archive member #{member.offset}.")
                    clean = False
    return clean


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    raise SystemExit(0 if check(parser.parse_args().package) else 1)
