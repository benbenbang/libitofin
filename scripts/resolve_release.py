"""Resolve a new or explicitly resumed release to an immutable source revision."""

import json
import os
import re
import subprocess
import tomllib

from check_release_version import release_version


LEGS = ("linux-x86_64", "linux-aarch64", "macos-arm64", "macos-x86_64")


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def choose_tag(requested, created, generated):
    tag = requested or (generated if created == "true" else "")
    if not tag:
        return ""
    version = release_version(tag)
    if tag != f"v{version}":
        raise ValueError("release must use a canonical stable vVERSION tag")
    return tag


def validate_documents(tag, manifest, lockfile):
    version = release_version(tag)
    if tomllib.loads(manifest)["workspace"]["package"]["version"] != version:
        raise ValueError("tag differs from released workspace version")
    packages = [p for p in tomllib.loads(lockfile)["package"] if "source" not in p]
    if not packages or any(p["version"] != version for p in packages):
        raise ValueError("tag differs from released local lockfile versions")


def verify_artifacts(run_id, repository, revision, parent, changed, run, jobs, artifacts):
    if not re.fullmatch(r"[1-9][0-9]*", run_id):
        raise ValueError("artifact run ID must be a positive integer")
    if run["repository"]["full_name"] != repository or run["id"] != int(run_id):
        raise ValueError("artifact run belongs to another repository or ID")
    if run["path"] not in (".github/workflows/semantic-release.yml", ".github/workflows/python-wheels.yml") or run["event"] != "workflow_dispatch":
        raise ValueError("unexpected artifact-producing workflow/event")
    if run["status"] != "completed":
        raise ValueError("artifact run is not completed")
    if run["head_sha"] != revision and not (
        run["head_sha"] == parent and changed and set(changed) <= {"Cargo.toml", "Cargo.lock"}
    ):
        raise ValueError("artifact run is not bound to the released source")
    for leg in LEGS:
        suffix = f"build itofin wheel ({leg})"
        matches = [job for job in jobs if job["name"] == suffix or job["name"] == "build itofin wheels / " + suffix]
        if len(matches) != 1 or matches[0]["conclusion"] != "success":
            raise ValueError(f"required artifact-producing job not successful: {leg}")
        matches = [artifact for artifact in artifacts if artifact["name"] == f"wheels-{leg}"]
        if len(matches) != 1 or matches[0]["expired"] is not False:
            raise ValueError(f"required artifact missing, duplicated or expired: {leg}")


def resolve(tag, repository):
    refs = {ref: sha for sha, ref in (line.split("\t") for line in command(
        "git", "ls-remote", "--tags", "origin", f"refs/tags/{tag}", f"refs/tags/{tag}^{{}}"
    ).splitlines())}
    canonical = f"refs/tags/{tag}"
    if canonical not in refs:
        raise ValueError("release tag is absent from origin")
    revision = refs.get(canonical + "^{}", refs[canonical])
    subprocess.run(["git", "fetch", "--no-tags", "origin", f"{canonical}:{canonical}"], check=True)
    if command("git", "rev-parse", tag + "^{commit}") != revision:
        raise ValueError("local and remote release revisions disagree")
    release = json.loads(command("gh", "release", "view", tag, "--repo", repository,
                                 "--json", "tagName,isDraft,isPrerelease"))
    if release["tagName"] != tag or release["isDraft"] is not False or release["isPrerelease"] is not False:
        raise ValueError("release is not an existing published stable GitHub release")
    validate_documents(tag, command("git", "show", revision + ":Cargo.toml"),
                       command("git", "show", revision + ":Cargo.lock"))
    return revision


def paged(repository, run_id, endpoint, key):
    pages = json.loads(command("gh", "api", "--paginate", "--slurp",
                              f"repos/{repository}/actions/runs/{run_id}/{endpoint}?per_page=100"))
    return [item for page in pages for item in page[key]]


def main():
    requested = os.environ.get("REQUESTED_TAG", "")
    tag = choose_tag(requested, os.environ.get("NEW_RELEASE", ""),
                     os.environ.get("NEW_TAG", ""))
    outputs = {"publish": "false", "tag": tag, "revision": ""}
    if tag and (requested or os.environ.get("DRY_RUN") != "true"):
        repository = os.environ["GITHUB_REPOSITORY"]
        revision = resolve(tag, repository)
        run_id = os.environ.get("PYTHON_ARTIFACT_RUN_ID", "")
        if run_id:
            if not re.fullmatch(r"[1-9][0-9]*", run_id):
                raise ValueError("artifact run ID must be a positive integer")
            run = json.loads(command("gh", "api", f"repos/{repository}/actions/runs/{run_id}"))
            parent = command("git", "rev-parse", revision + "^")
            changed = command("git", "diff", "--name-only", parent, revision).splitlines()
            verify_artifacts(run_id, repository, revision, parent, changed, run,
                             paged(repository, run_id, "jobs", "jobs"),
                             paged(repository, run_id, "artifacts", "artifacts"))
        outputs.update(revision=revision, publish=str(os.environ.get("DRY_RUN") != "true").lower())
    with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
        for key, value in outputs.items():
            stream.write(f"{key}={value}\n")
    print(json.dumps(outputs))


if __name__ == "__main__":
    main()
