"""Publish once, or reconcile an exact, non-yanked crates.io release."""

import argparse
import hashlib
import http.client
import io
import json
import os
import re
from pathlib import Path, PurePosixPath
import subprocess
import tarfile
import time
import urllib.error
import urllib.request

from check_release_version import check_version


AGENT = "libitofin-release (https://github.com/benbenbang/libitofin)"
PACKAGES = ("itofin-optimize", "libitofin")
DELAYS = (2, 4, 8, 16, 30)


class NotVisible(RuntimeError):
    """The API accepted a version that is not yet installable."""


def read_url(url, *, absent=False):
    for attempt in range(3):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": AGENT})
            with urllib.request.urlopen(request, timeout=10) as response:
                data = response.read(50_000_001)
                if len(data) > 50_000_000:
                    raise ValueError("registry response exceeds size limit")
                return data
        except urllib.error.HTTPError as error:
            if error.code == 404 and absent:
                return None
            if error.code != 429 and not 500 <= error.code < 600:
                raise
            failure = error
        except (urllib.error.URLError, http.client.HTTPException, OSError) as error:
            failure = error
        if attempt < 2:
            time.sleep((1, 3)[attempt])
    raise RuntimeError(f"registry unavailable: {url}") from failure


def archive_files(data, package, version, revision, source_path):
    prefix = f"{package}-{version}/"
    files = {}
    size = 0
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts or not member.name.startswith(prefix) or str(path) != member.name.rstrip("/"):
                raise ValueError("unsafe or unexpected crate archive path")
            if member.isdir():
                continue
            if not member.isfile() or member.name in files:
                raise ValueError("non-regular or duplicate crate archive member")
            size += member.size
            if size > 100_000_000 or len(files) >= 50_000:
                raise ValueError("crate archive exceeds size limit")
            files[member.name] = archive.extractfile(member).read()
    vcs = json.loads(files[prefix + ".cargo_vcs_info.json"])
    if vcs["git"]["sha1"] != revision or vcs.get("path_in_vcs") != source_path:
        raise ValueError("published crate source revision/path differs from release")
    if vcs["git"].get("dirty", False) is not False:
        raise ValueError("crate archive was built from dirty source")
    return files


def index_path(package):
    if len(package) == 1:
        return f"1/{package}"
    if len(package) == 2:
        return f"2/{package}"
    if len(package) == 3:
        return f"3/{package[0]}/{package}"
    return f"{package[:2]}/{package[2:4]}/{package}"


def existing(package, version, revision, source_path, expected, fetch=read_url):
    data = fetch(f"https://crates.io/api/v1/crates/{package}/{version}", absent=True)
    if data is None:
        return False
    metadata = json.loads(data)["version"]
    if metadata["crate"] != package or metadata["num"] != version or metadata["yanked"] is not False:
        raise ValueError("registry version identity differs or is yanked")
    checksum = metadata["checksum"]
    if not isinstance(checksum, str) or not re.fullmatch(r"[0-9a-f]{64}", checksum):
        raise ValueError("invalid registry checksum")
    index = fetch(f"https://index.crates.io/{index_path(package)}", absent=True)
    rows = [] if index is None else [json.loads(line) for line in index.splitlines()]
    matches = [row for row in rows if row["vers"] == version]
    if not matches:
        raise NotVisible("accepted version not yet present in registry index")
    if len(matches) != 1 or matches[0]["name"] != package or matches[0]["yanked"] is not False or matches[0]["cksum"] != checksum:
        raise ValueError("registry API and index disagree")
    data = fetch(f"https://static.crates.io/crates/{package}/{package}-{version}.crate", absent=True)
    if data is None:
        raise NotVisible("accepted version not yet downloadable")
    if hashlib.sha256(data).hexdigest() != checksum:
        raise ValueError("downloaded registry crate checksum differs")
    if archive_files(data, package, version, revision, source_path) != expected:
        raise ValueError("published crate files differ from verified release package")
    return True


def reconcile(probe, sleep=time.sleep):
    for attempt in range(len(DELAYS) + 1):
        try:
            if probe():
                return True
        except NotVisible:
            pass
        if attempt < len(DELAYS):
            sleep(DELAYS[attempt])
    return False


def visible_or_absent(probe, sleep=time.sleep):
    try:
        return probe()
    except NotVisible:
        if reconcile(probe, sleep):
            return True
        raise RuntimeError("accepted version is still not installable; refusing another upload")


def publish_once(probe, publish, sleep=time.sleep):
    if visible_or_absent(probe, sleep):
        return "already-published"
    code = publish()
    if reconcile(probe, sleep):
        return "published" if code == 0 else "reconciled-after-publish-error"
    raise RuntimeError(f"publication not confirmed in registry (cargo exit {code}); rerun this release, not a new version")


def prepare(root, package, tag, revision):
    if package not in PACKAGES:
        raise ValueError("unexpected publication package")
    version = check_version(root, tag)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if head != revision:
        raise ValueError("source checkout differs from resolved release revision")
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=root
    ))
    matches = [p for p in metadata["packages"] if p["name"] == package]
    if len(matches) != 1 or matches[0]["version"] != version:
        raise ValueError("package identity differs from committed release")
    source_path = str(Path(matches[0]["manifest_path"]).parent.relative_to(root.resolve()))
    subprocess.run(["cargo", "package", "-p", package, "--locked"], cwd=root, check=True)
    path = Path(metadata["target_directory"]) / "package" / f"{package}-{version}.crate"
    files = archive_files(path.read_bytes(), package, version, revision, source_path)
    return version, source_path, files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", choices=PACKAGES)
    parser.add_argument("tag")
    parser.add_argument("revision")
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--publish", action="store_true")
    args = parser.parse_args()
    try:
        version, source_path, files = prepare(args.root, args.package, args.tag, args.revision)
        probe = lambda: existing(args.package, version, args.revision, source_path, files)
        if args.publish:
            state = publish_once(probe, lambda: subprocess.run(
                ["cargo", "publish", "-p", args.package, "--locked", "--registry", "crates-io"], cwd=args.root
            ).returncode)
            required = False
        else:
            required = not visible_or_absent(probe)
            state = "ready-to-upload" if required else "already-published"
        print(f"{args.package}@{version}: {state}")
        if output := os.environ.get("GITHUB_OUTPUT"):
            with open(output, "a") as stream:
                stream.write(f"upload_required={str(required).lower()}\n")
        if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(summary, "a") as stream:
                stream.write(f"- {args.package}@{version}: {state}, source `{args.revision}`\n")
    except (ValueError, RuntimeError, KeyError, OSError, tarfile.TarError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Release publication failed: {error}\n")


if __name__ == "__main__":
    main()
