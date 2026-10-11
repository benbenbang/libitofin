# Release recovery

Publication is not atomic across registries. Cargo can upload successfully and
then fail while confirming index availability. A red job is not proof that the
version is missing. Never bump the version just to bypass an existing upload.

## Resume an existing release

Use **Actions > semantic-release > Run workflow** on **main**:

| Input | Rust-only recovery example |
| --- | --- |
| `prompt` | `true` |
| `release_tag` | `v0.41.0` |
| `publish_targets` | `rust` |
| `dry_run` | `false` |
| `python_artifact_run_id` | empty |

An explicit tag skips version generation. Recovery validates a published,
non-draft stable GitHub release, its remote tag commit and committed workspace
and lockfile versions. Every channel receives the resolved source revision.
Tooling comes from the workflow checkout, so old tags can use newer recovery
code. An existing-tag `dry_run=true` validates identity without publication;
a new-release dry run previews the version without requiring an uncreated tag.

Leave `release_tag` empty only when intentionally creating a **new** release.
Targets `all`, `rust`, `python` and `go` select independent publication channels;
Rust still publishes/verifies `itofin-optimize` before `libitofin`. Coordinated
releases remain serialized and are not cancelled by a later dispatch.

## Rust safeguards

- Package and compile with the released Rust toolchain and `--locked`.
- Skip an existing version only after verifying its non-yanked registry/API
  identity, sparse-index checksum, downloaded checksum, source revision/path
  and every packaged file against the locally verified release package.
- Upload at most once per invocation. Even a failed Cargo exit is reconciled
  against the public registry using bounded visibility retries.
- Disable Cargo HTTP/2 multiplexing and retain bounded network retries. Mint
  the short-lived OIDC credential only after preparation shows an upload is needed.
- Registry outages, yanked versions and conflicting contents fail closed.
  If confirmation remains unavailable, retry the **same** release later.

## Python recovery

Rerunning failed jobs reuses the original run's wheel artifacts. A new dispatch
can select `python` and supply the original successful wheel-build run ID in
`python_artifact_run_id`, for example `38102541936` for the v0.41.0 build.
The source-bound run, repository, workflow, all four successful required build
jobs and non-expired artifacts are validated before reuse.

The complete set is four required Linux/macOS wheels plus one sdist. Distribution
metadata must match `itofin` and the tag version. Already-uploaded files are
removed from the upload set only after all local and downloaded public SHA256
checks pass. Missing files are uploaded; differing or yanked files are not ignored.
Rebuilt wheels can differ byte-for-byte, so prefer the original artifacts for
partial recovery. Expired artifacts or conflicting rebuilds need manual review.
Enabling additional wheel platforms requires updating the complete-set contract.

Go retains its existing asset/checksum/tag reconciliation and published-consumer
checks. Changed workflow code is **not** applied to reruns of an old run: start a
new dispatch on main with the existing tag. The historical red run remains red;
the successful recovery is a separate audited run, not a rewritten history.
