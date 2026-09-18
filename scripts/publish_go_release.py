"""Attach verified native assets to one core release and publish its Go tag."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
from urllib.parse import quote

from check_release_version import check_version


def command(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def api(path: str, *args: str):
    return json.loads(command("gh", "api", path, *args))


def remote_commit(repository: str, tag: str):
    result = subprocess.run(
        ["gh", "api", f"repos/{repository}/git/ref/tags/{quote(tag, safe='')}"],
        capture_output=True, text=True,
    )
    if result.returncode:
        if "(HTTP 404)" in result.stderr:
            return None
        raise RuntimeError(result.stderr)
    obj = json.loads(result.stdout)["object"]
    for _ in range(8):
        if obj["type"] == "commit":
            return obj["sha"]
        if obj["type"] != "tag":
            break
        obj = api(f"repos/{repository}/git/tags/{obj['sha']}")["object"]
    raise ValueError(f"tag {tag!r} does not resolve to a commit")


def archive_identity(path: Path, version: str, revision: str, platform: str) -> str:
    name = f"itofin-native-{version}-{platform}"
    if path.name != name + ".tar.gz":
        raise ValueError("unexpected native archive name")
    with tarfile.open(path, "r:gz") as archive:
        members = [member for member in archive.getmembers() if member.name == name + "/VERSION"]
        if len(members) != 1 or not members[0].isfile():
            raise ValueError("native archive must contain one regular VERSION manifest")
        source = archive.extractfile(members[0])
        if source is None:
            raise ValueError("missing native VERSION manifest")
        entries = [line.split("=", 1) for line in source.read().decode().splitlines()]
        manifest = dict(entries)
        if len(manifest) != len(entries):
            raise ValueError("duplicate native manifest keys")
    expected = {"version": version, "revision": revision, "platform": platform}
    if any(manifest.get(key) != value for key, value in expected.items()):
        raise ValueError("native archive identity does not match the released commit")
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_checksum(path: Path, digest: str) -> None:
    fields = path.with_name(path.name + ".sha256").read_text().split()
    if fields != [digest, path.name]:
        raise ValueError(f"checksum mismatch for {path.name}")


def publish(repository: str, tag: str, directory: Path, root: Path) -> dict:
    version = check_version(root, tag)
    if tag != f"v{version}":
        raise ValueError("coordinated releases require a v-prefixed core tag")
    module = root / "sdk/go/go.mod"
    if not module.is_file() or ["module", f"github.com/{repository}/sdk/go"] not in [
        line.split() for line in module.read_text().splitlines()
    ]:
        raise ValueError("checkout must declare the sdk/go module before publishing its tag")
    revision = command("git", "-C", str(root), "rev-parse", "HEAD")
    if remote_commit(repository, tag) != revision:
        raise ValueError("checkout does not match the remote core release tag")
    go_tag = f"sdk/go/v{version}"
    existing = remote_commit(repository, go_tag)
    if existing is not None and existing != revision:
        raise ValueError("existing Go tag points to a different commit; refusing to move it")
    endpoint = f"repos/{repository}/releases/tags/{quote(tag, safe='')}"
    release = api(endpoint)
    if release["tag_name"] != tag or release["draft"]:
        raise ValueError("the matching core release must already be published")
    names = {asset["name"] for asset in release["assets"]}
    checksums = {}
    with tempfile.TemporaryDirectory() as temporary:
        staged = Path(temporary)
        for platform in ("linux-amd64", "darwin-arm64"):
            name = f"itofin-native-{version}-{platform}.tar.gz"
            archive = directory / name
            digest = archive_identity(archive, version, revision, platform)
            verify_checksum(archive, digest)
            checksum_name = name + ".sha256"
            if checksum_name in names and name not in names:
                raise ValueError(f"published checksum has no archive: {checksum_name}")
            if name in names:
                command("gh", "release", "download", tag, "--repo", repository,
                        "--pattern", name, "--dir", str(staged))
                archive = staged / name
                digest = archive_identity(archive, version, revision, platform)
                if checksum_name in names:
                    command("gh", "release", "download", tag, "--repo", repository,
                            "--pattern", checksum_name, "--dir", str(staged))
                    verify_checksum(archive, digest)
                else:
                    archive.with_name(checksum_name).write_text(f"{digest}  {name}\n")
            else:
                command("gh", "release", "upload", tag, str(archive), "--repo", repository)
            if checksum_name not in names:
                command("gh", "release", "upload", tag, str(archive.with_name(checksum_name)),
                        "--repo", repository)
            checksums[name] = digest
    published = {asset["name"] for asset in api(endpoint)["assets"]}
    if not all(name in published and name + ".sha256" in published for name in checksums):
        raise ValueError("both native platforms must be attached before publishing the Go tag")
    if existing is None:
        api(f"repos/{repository}/git/refs", "--method", "POST", "-f", f"ref=refs/tags/{go_tag}",
            "-f", f"sha={revision}")
    if remote_commit(repository, go_tag) != revision:
        raise ValueError("published Go tag does not match the released commit")
    release = api(endpoint)
    body = release.get("body") or ""
    if "## Go bindings" not in body:
        notes = (f"\n\n## Go bindings\n\n"
                 f"Install `github.com/{repository}/sdk/go@v{version}` with Go 1.27.1. "
                 "The native archives and SHA-256 checksums are attached below for Linux amd64 "
                 "and macOS arm64. cgo, a C compiler, and the matching native package are required. "
                 f"See the [installation guide](https://github.com/{repository}/blob/{tag}/docs/go-distribution.md).\n")
        with tempfile.TemporaryDirectory() as temporary:
            note_file = Path(temporary) / "notes.md"
            note_file.write_text(body + notes)
            command("gh", "release", "edit", tag, "--repo", repository,
                    "--notes-file", str(note_file))
    return {"version": version, "revision": revision, "go_tag": go_tag, "sha256": checksums}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("directory", type=Path)
    parser.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY"))
    args = parser.parse_args()
    if not args.repository:
        parser.error("--repository or GITHUB_REPOSITORY is required")
    result = publish(args.repository, args.tag, args.directory, Path(__file__).resolve().parents[1])
    print(json.dumps(result, indent=2))
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a") as output:
            output.write("### Go release identity\n\n```json\n" + json.dumps(result, indent=2) + "\n```\n")


if __name__ == "__main__":
    main()
