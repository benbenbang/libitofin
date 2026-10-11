"""Validate complete release artifacts and retain only files missing from PyPI."""

import argparse
from email.parser import BytesParser
import hashlib
from http.client import HTTPException
import json
import os
from pathlib import Path
import re
import tarfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlparse
from urllib.request import Request, urlopen
import zipfile

from check_release_version import release_version


REQUIRED_PLATFORMS = {"linux-x86_64", "linux-aarch64", "macos-x86_64", "macos-arm64", "sdist"}
RETRIES = 4


def platform_identity(tags: str) -> str:
    identities = set()
    for tag in tags.split("."):
        linux = re.fullmatch(r"(?:manylinux_(?:\d+)_\d+|manylinux1|manylinux2010|manylinux2014)_(x86_64|aarch64)", tag)
        macos = re.fullmatch(r"macosx_\d+_\d+_(x86_64|arm64)", tag)
        if linux:
            identities.add(f"linux-{linux[1]}")
        elif macos:
            identities.add(f"macos-{macos[1]}")
        else:
            raise ValueError(f"unsupported wheel platform: {tag}")
    if len(identities) != 1:
        raise ValueError("wheel combines different required platforms")
    return identities.pop()


def filename_identity(name: str, version: str) -> str:
    if name == f"itofin-{version}.tar.gz":
        return "sdist"
    wheel = re.fullmatch(r"itofin-([^-]+)-(?:\d[^-]*-)?([\w.]+)-([\w.]+)-([\w.]+)\.whl", name)
    if not wheel or wheel[1] != version:
        raise ValueError(f"distribution filename does not match itofin {version}: {name}")
    return platform_identity(wheel[4])


def verify_metadata(data: bytes, version: str) -> None:
    metadata = BytesParser().parsebytes(data)
    if metadata.get_all("Name") != ["itofin"] or metadata.get_all("Version") != [version]:
        raise ValueError("distribution metadata does not match the release name and version")


def artifact_identity(path: Path, version: str) -> str:
    identity = filename_identity(path.name, version)
    if identity == "sdist":
        with tarfile.open(path, "r:gz") as archive:
            members = [member for member in archive.getmembers()
                       if member.name == f"itofin-{version}/PKG-INFO"]
            if len(members) != 1 or not members[0].isfile() or members[0].size > 2_000_000:
                raise ValueError("sdist must contain one regular root PKG-INFO")
            source = archive.extractfile(members[0])
            if source is None:
                raise ValueError("sdist metadata is missing")
            verify_metadata(source.read(), version)
    else:
        with zipfile.ZipFile(path) as archive:
            members = [member for member in archive.infolist() if member.filename.endswith(".dist-info/METADATA")]
            if len(members) != 1 or members[0].file_size > 2_000_000:
                raise ValueError("wheel must contain one METADATA file")
            if members[0].filename != f"itofin-{version}.dist-info/METADATA":
                raise ValueError("wheel metadata directory does not match the release")
            verify_metadata(archive.read(members[0]), version)
    return identity


def digest_file(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def read_url(url: str, *, allow_absent: bool = False, opener=None, sleeper=None, limit: int = 4_000_000):
    opener = opener or urlopen
    sleeper = sleeper or time.sleep
    for attempt in range(RETRIES):
        try:
            request = Request(url, headers={"Accept": "application/json", "User-Agent": "itofin-release-verifier"})
            with opener(request, timeout=30) as response:
                data = response.read(limit + 1)
                if len(data) > limit:
                    raise ValueError("registry response exceeds the permitted size")
                return data
        except HTTPError as error:
            if error.code == 404 and allow_absent:
                return None
            if error.code != 429 and not 500 <= error.code <= 599:
                raise RuntimeError(f"registry returned HTTP {error.code}") from error
            failure = error
        except (URLError, TimeoutError, ConnectionError, OSError, HTTPException) as error:
            failure = error
        if attempt + 1 < RETRIES:
            sleeper(2 ** attempt)
    raise RuntimeError("registry unavailable after bounded retries") from failure


def published_files(version: str, *, opener=None, sleeper=None) -> dict:
    data = read_url(f"https://pypi.org/pypi/itofin/{quote(version, safe='')}/json",
                    allow_absent=True, opener=opener, sleeper=sleeper)
    if data is None:
        return {}
    try:
        release = json.loads(data)
        if release["info"]["name"] != "itofin" or release["info"]["version"] != version:
            raise ValueError("PyPI release identity does not match the requested release")
        files = release["urls"]
        if not isinstance(files, list):
            raise ValueError("PyPI release urls must be a list")
        result = {}
        for entry in files:
            name = entry["filename"]
            if not isinstance(name, str) or name in result:
                raise ValueError("PyPI release contains an invalid or duplicate filename")
            filename_identity(name, version)
            digest = entry["digests"]["sha256"]
            if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
                raise ValueError("PyPI release contains an invalid SHA256 digest")
            if not isinstance(entry["yanked"], bool):
                raise ValueError("PyPI release contains an invalid yanked flag")
            result[name] = entry
        return result
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        raise ValueError("malformed PyPI release response") from error


def verify_remote_bytes(entry: dict, *, opener=None, sleeper=None) -> None:
    url = entry.get("url")
    parsed = urlparse(url) if isinstance(url, str) else None
    if parsed is None or parsed.scheme != "https" or parsed.netloc != "files.pythonhosted.org":
        raise ValueError("PyPI file URL is not on the trusted download host")
    data = read_url(url, opener=opener, sleeper=sleeper, limit=250_000_000)
    if hashlib.sha256(data).hexdigest() != entry["digests"]["sha256"]:
        raise ValueError(f"PyPI download digest mismatch: {entry['filename']}")


def prepare(tag: str, directory: Path, *, opener=None, sleeper=None, verify_downloads=False) -> bool:
    version = release_version(tag)
    if tag != f"v{version}":
        raise ValueError("PyPI publication requires the exact core vVERSION release tag")
    paths = sorted(directory.iterdir())
    if len(paths) != len(REQUIRED_PLATFORMS) or any(path.is_symlink() or not path.is_file() for path in paths):
        raise ValueError("release directory must contain exactly five regular distribution files")
    identities = [artifact_identity(path, version) for path in paths]
    if set(identities) != REQUIRED_PLATFORMS or len(set(identities)) != len(identities):
        raise ValueError("release artifacts must include exactly one wheel per required platform and one sdist")
    digests = {path.name: digest_file(path) for path in paths}
    remote = published_files(version, opener=opener, sleeper=sleeper)
    local_names = set(digests)
    if set(remote) - local_names:
        raise ValueError("PyPI already contains different distribution filenames for this release")
    removable = []
    for path in paths:
        entry = remote.get(path.name)
        if entry is None:
            continue
        if entry["yanked"]:
            raise ValueError(f"published distribution is yanked: {path.name}")
        if entry["digests"]["sha256"] != digests[path.name]:
            raise ValueError(f"published distribution differs from the local artifact: {path.name}")
        if verify_downloads:
            verify_remote_bytes(entry, opener=opener, sleeper=sleeper)
        removable.append(path)
    if any(path.is_symlink() or not path.is_file() or digest_file(path) != digests[path.name] for path in paths):
        raise ValueError("local artifacts changed during registry verification")
    for path in removable:
        path.unlink()
        print(f"Already published and byte-identical: {path.name}")
    return len(removable) != len(paths)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("directory", type=Path)
    parser.add_argument("--verify-downloads", action="store_true")
    args = parser.parse_args()
    try:
        required = prepare(args.tag, args.directory, verify_downloads=args.verify_downloads)
        output = f"upload_required={str(required).lower()}\n"
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as destination:
                destination.write(output)
        print(output, end="")
    except (ValueError, RuntimeError, OSError, tarfile.TarError, zipfile.BadZipFile) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
