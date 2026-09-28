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


REQUIRED_PLATFORMS = ("linux-amd64", "linux-arm64", "darwin-amd64", "darwin-arm64")
OPTIONAL_PLATFORMS = ("windows-amd64",)


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


def verify_checksum(path: Path, digest: str, checksum: Path | None = None) -> None:
    fields = (checksum or path.with_name(path.name + ".sha256")).read_text().split()
    if fields != [digest, path.name]:
        raise ValueError(f"checksum mismatch for {path.name}")


def verify_windows_contents(path: Path) -> None:
    root = path.name.removesuffix(".tar.gz")
    required = {
        "include/itofin.h", "lib/itofin_ffi.dll", "lib/libitofin_ffi.dll.a",
        "LICENSE", "VERSION", "licenses/libitofin/THIRD_PARTY_NOTICES.md",
        "licenses/libitofin/QUANTLIB_LICENSE.txt",
    }
    with tarfile.open(path, "r:gz") as archive:
        members = archive.getmembers()
        directories = {root, f"{root}/include", f"{root}/lib",
                       f"{root}/licenses", f"{root}/licenses/libitofin"}
        if any((member.isdir() and member.name.rstrip("/") not in directories)
               or (member.isfile() and not member.name.startswith(root + "/"))
               or not (member.isfile() or member.isdir()) for member in members):
            raise ValueError("Windows archive contains an unexpected entry")
        files = {member.name.removeprefix(root + "/"): member
                 for member in members if member.isfile()}
        if len(files) != sum(member.isfile() for member in members) or set(files) != required | {"SHA256SUMS"}:
            raise ValueError("Windows archive contents are incomplete or unexpected")
        checksums = archive.extractfile(files["SHA256SUMS"]).read().decode().splitlines()
        recorded = {}
        for line in checksums:
            digest, separator, name = line.partition("  ")
            if not separator or name in recorded:
                raise ValueError("invalid Windows archive SHA256SUMS")
            recorded[name] = digest
        if set(recorded) != required:
            raise ValueError("Windows archive SHA256SUMS is incomplete")
        for name in required:
            content = archive.extractfile(files[name]).read()
            if hashlib.sha256(content).hexdigest() != recorded[name]:
                raise ValueError(f"Windows archive checksum mismatch: {name}")


def publish_optional(repository: str, tag: str, directory: Path, version: str,
                     revision: str, endpoint: str, platform: str) -> dict:
    name = f"itofin-native-{version}-{platform}.tar.gz"
    checksum_name = name + ".sha256"
    local = directory / name
    try:
        names = {asset["name"] for asset in api(endpoint)["assets"]}
        with tempfile.TemporaryDirectory() as temporary:
            staged = Path(temporary)
            if name in names:
                command("gh", "release", "download", tag, "--repo", repository,
                        "--pattern", name, "--dir", str(staged))
                archive = staged / name
                digest = archive_identity(archive, version, revision, platform)
                verify_windows_contents(archive)
                if checksum_name in names:
                    command("gh", "release", "download", tag, "--repo", repository,
                            "--pattern", checksum_name, "--dir", str(staged))
                    verify_checksum(archive, digest)
                else:
                    archive.with_name(checksum_name).write_text(f"{digest}  {name}\n")
                    command("gh", "release", "upload", tag, str(staged / checksum_name),
                            "--repo", repository)
            elif local.is_file():
                digest = archive_identity(local, version, revision, platform)
                verify_windows_contents(local)
                verify_checksum(local, digest)
                if checksum_name in names:
                    command("gh", "release", "download", tag, "--repo", repository,
                            "--pattern", checksum_name, "--dir", str(staged))
                    verify_checksum(local, digest, staged / checksum_name)
                command("gh", "release", "upload", tag, str(local), "--repo", repository)
                if checksum_name not in names:
                    command("gh", "release", "upload", tag,
                            str(local.with_name(checksum_name)), "--repo", repository)
            else:
                return {"status": "absent" if checksum_name not in names else "incomplete"}
            published = {asset["name"] for asset in api(endpoint)["assets"]}
            if name not in published or checksum_name not in published:
                raise ValueError("optional archive/checksum pair is incomplete")
            for asset in (name, checksum_name):
                path = staged / asset
                path.unlink(missing_ok=True)
                command("gh", "release", "download", tag, "--repo", repository,
                        "--pattern", asset, "--dir", str(staged))
            digest = archive_identity(staged / name, version, revision, platform)
            verify_windows_contents(staged / name)
            verify_checksum(staged / name, digest)
            return {"status": "published", "sha256": digest}
    except Exception as error:
        return {"status": "skipped", "reason": f"{type(error).__name__}: {error}"}


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
    for platform in REQUIRED_PLATFORMS:
        if not (directory / f"itofin-native-{version}-{platform}.tar.gz").is_file():
            raise ValueError(f"missing native archive for {platform}")
    checksums = {}
    with tempfile.TemporaryDirectory() as temporary:
        staged = Path(temporary)
        for platform in REQUIRED_PLATFORMS:
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
        raise ValueError("all required native platforms must be attached before publishing the Go tag")
    if existing is None:
        api(f"repos/{repository}/git/refs", "--method", "POST", "-f", f"ref=refs/tags/{go_tag}",
            "-f", f"sha={revision}")
    if remote_commit(repository, go_tag) != revision:
        raise ValueError("published Go tag does not match the released commit")
    optional = {platform: publish_optional(repository, tag, directory, version, revision,
                                           endpoint, platform)
                for platform in OPTIONAL_PLATFORMS}
    release = api(endpoint)
    body = release.get("body") or ""
    updated_body = body
    if "## Go bindings" not in body:
        notes = (f"\n\n## Go bindings\n\n"
                 f"Install `github.com/{repository}/sdk/go@v{version}` with Go 1.27.1. "
                 "The native archives and SHA-256 checksums are attached below for Linux amd64, "
                 "Linux arm64, macOS amd64, and macOS arm64. cgo, a C compiler, and the matching native package are required. "
                 f"See the [installation guide](https://github.com/{repository}/blob/{tag}/docs/go-distribution.md).\n")
        updated_body += notes
    windows_note = ("Windows amd64 native Go package (GNU/MinGW) is available with "
                    "its SHA-256 checksum; use a matching MinGW cgo compiler.\n")
    if optional["windows-amd64"]["status"] == "published" and windows_note not in updated_body:
        updated_body += "\n" + windows_note
    if optional["windows-amd64"]["status"] != "published":
        updated_body = updated_body.replace("\n" + windows_note, "")
    if updated_body != body:
        with tempfile.TemporaryDirectory() as temporary:
            note_file = Path(temporary) / "notes.md"
            note_file.write_text(updated_body)
            command("gh", "release", "edit", tag, "--repo", repository,
                    "--notes-file", str(note_file))
    return {"version": version, "revision": revision, "go_tag": go_tag,
            "sha256": checksums, "optional": optional}


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
