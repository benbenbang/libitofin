"""Verify QuantLib attribution in Python resources and built release archives."""

import argparse
import hashlib
from pathlib import Path
import tarfile
import zipfile


ROOT = Path(__file__).resolve().parents[1]
NAMES = ("THIRD_PARTY_NOTICES.md", "QUANTLIB_LICENSE.txt")
RESOURCE = "itofin/licenses/libitofin"


def source_notices(root=ROOT):
    expected = {}
    for name in NAMES:
        canonical = (root / "crates/libitofin" / name).read_bytes()
        copied = root / "crates/itofin-py/python" / RESOURCE / name
        if copied.read_bytes() != canonical:
            raise ValueError(
                f"Python attribution differs from canonical source: {copied}"
            )
        expected[name] = canonical
    return expected


def check_archive(path, expected):
    native = path.name.startswith("itofin-native-")
    wheel = path.suffix == ".whl"
    with zipfile.ZipFile(path) if wheel else tarfile.open(path, "r:gz") as archive:
        names = archive.namelist() if wheel else archive.getnames()

        def read(name):
            return archive.read(name) if wheel else archive.extractfile(name).read()

        if wheel:
            prefix = RESOURCE
        else:
            roots = {name.split("/", 1)[0] for name in names}
            if len(roots) != 1:
                raise ValueError(f"Expected one archive root: {path}")
            prefix = f"{roots.pop()}/licenses/libitofin" if native else None
        for name, canonical in expected.items():
            matches = (
                [key for key in names if key == f"{prefix}/{name}"]
                if prefix
                else [
                    key for key in names if key.endswith(f"/python/{RESOURCE}/{name}")
                ]
            )
            if len(matches) != 1 or read(matches[0]) != canonical:
                raise ValueError(f"Missing or changed attribution {name}: {path}")
            if native:
                archive_root = matches[0].split("/", 1)[0]
                manifest_path = f"{archive_root}/SHA256SUMS"
                manifest = (
                    read(manifest_path).decode().splitlines()
                    if manifest_path in names
                    else []
                )
                wanted = f"{hashlib.sha256(canonical).hexdigest()}  licenses/libitofin/{name}"
                if wanted not in manifest:
                    raise ValueError(f"Attribution missing from SHA256SUMS: {name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifacts", nargs="*", type=Path)
    args = parser.parse_args()
    expected = source_notices()
    for requested in args.artifacts:
        paths = (
            sorted([*requested.glob("*.whl"), *requested.glob("*.tar.gz")])
            if requested.is_dir()
            else [requested]
        )
        if not paths:
            raise ValueError(f"No release archives found: {requested}")
        for path in paths:
            check_archive(path, expected)
            print(f"Attribution verified: {path}")


if __name__ == "__main__":
    main()
