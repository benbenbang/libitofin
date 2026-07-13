# The knowledge graph: what it is, and what you may trust it for

Read this before you use the graph to plan a ticket, scope a port, or claim something is done.

**One sentence:** the graph is a routing table over names and structure. It tells you *which file to
open*. It never tells you whether a port is correct, complete, or faithful, because it contains no
behavioural information at all.

> **The rule: the graph proposes, `QuantLib/` source disposes.**
> Every claim that reaches a ticket must be verified by reading the source and citing `file:line`.
> This is the same gate the ticket-review process already enforces. The graph makes finding the
> source cheap. It does not make reading it optional. A graph makes an unverified claim *sound
> confident*, which makes skipping the read more dangerous, not less.

## What exists

| artifact | tracked? | what it is |
| --- | --- | --- |
| `graphify-out/` | no (generated) | **Rust graph** - `crates/`, 5,787 nodes, 225 communities. The default query target. |
| `graphify-out-ql/` | no (generated) | **QuantLib graph** - `ql/` (2,412 files) + `test-suite/` (192 files, the oracle). 39,405 nodes. |
| `graphify/port_map.json` | **yes** | The Rust <-> QuantLib correspondence, plus the oracle test per module. Curated. |
| `graphify/COVERAGE.md` | yes | Port coverage by subsystem, generated from `port_map.json`. |
| `graphify/scripts/` | yes | Regenerates all of the above. AST-only: no LLM, no API key, no token cost. |

The two graphs are **language-pure and deliberately not merged**. See "Why not one merged graph" below.

## How to query

```bash
# Rust graph (default - no ceremony)
graphify query "how does Settings notify observers"
graphify explain "libitofin_src_settings_settings"
graphify path "Settings" "IborIndex"

# QuantLib graph + test-suite oracle
GRAPHIFY_OUT=graphify-out-ql graphify explain "ql_cashflows_iborcoupon_iborcoupon"
GRAPHIFY_OUT=graphify-out-ql graphify explain "test_suite_calendars"
```

**Cross-language questions are two steps on purpose:** look the module up in `port_map.json`, then
query the other graph. The join stays visible as an inference every time you use it.

Node IDs are namespaced `libitofin_*` and `ql_*` / `test_suite_*`.

## Trust matrix

| what | provenance | trust |
| --- | --- | --- |
| Nodes, `calls` / `contains` / `imports_from` / `inherits` edges | AST, `EXTRACTED` | **High.** Deterministic. This is real structure. |
| God nodes, communities | computed from the above | **High**, within one language. |
| `port_map.json` mappings marked `exact-path` | name/path match | **Medium.** Right the vast majority of the time; still a name match. |
| `port_map.json` mappings marked `stem` | basename match, paths differ | **Low-medium.** Verify before citing. |
| `oracle` field marked `dir-stem` | parent directory matched a test file | **Low.** Coarse: `calendars.cpp` is the oracle for *every* calendar, not just yours. |
| Anything about behaviour, numerics, correctness, completeness | **not in the graph** | **None. It is not there.** |

`graphify query` (BFS) is the weakest of the three query modes on this codebase: it seeds from fuzzy
name matches, and the god nodes are so connected that depth-2 from `Date` reaches a quarter of the
graph. Prefer `path` and `explain`.

## Safe uses

- Navigation. "Where does `IborCoupon` live", "what does `Settings` touch".
- Blast radius. "What breaks if I change `Observable`".
- Scoping a ticket to a file set.
- Seeding a backlog: `port_map.json` has **1,131 QuantLib modules with no Rust counterpart**. That is
  a *candidate* list, not a validated one.
- Finding the oracle test for a module (117 of 257 modules have one identified).

## Unsafe uses

- Claiming a module is ported, unported, or partially ported without opening the source.
- Symbol-level "what is missing" diffs. These over-report badly. See below.
- Anything about numerical fidelity. The graph has never seen a number.

## Known failure modes (all observed, not hypothetical)

**1. Relocated symbols read as "missing."** QuantLib declares `IborLeg` inside `iborcoupon.hpp`. The
Rust port gives it its own `cashflows/iborleg.rs` (PR #311). A per-module symbol diff reported
`IborLeg` as unported. It is fully ported. This is hand-corrected in `port_map.json` as a `manual`
entry - **add more the same way when you find them.**

**2. Private C++ members look unported and always will be.** `caps_`, `gearings_`, `index_`. A Rust
port has no reason to mirror them by name.

**3. Coverage is module-level, not feature-level.** `time` reads 100% because every QuantLib header
has a Rust counterpart, not because every QuantLib behaviour is present. A half-ported module scores
the same as a complete one.

**4. Name collisions defeat fuzzy lookup.** `explain "Settings"` resolves to the free function
`settings()` in `cashflows.rs`, not the `Settings` struct. Address god nodes by **ID**, not by name.

**5. The extractor can silently produce nothing.** On the first build, all 192 `test-suite/` files
extracted to **zero nodes** (a worker process died, graphify issue #1666) and the build wrote a
confident-looking graph anyway, with the oracle entirely absent. `build_ql.py` now hard-aborts if the
test-suite yields no nodes. If you write your own extraction, **assert on node counts.**

**6. `test-suite` nodes are shallow.** About 13 nodes per file; `calendars.cpp` has degree 5. You get
the *presence* of a test file and some symbols. You do not get what it asserts.

## Why not one merged graph

A merged 40k-node graph with inferred cross-language edges was built and rejected. Evidence:

- **It corrupted the analytics.** C++ `Date()` (1,315 edges) and Rust `Date` (685) ranked as separate
  top-5 god nodes, indistinguishable to fuzzy lookup. Communities went from 225 meaningful ones to
  1,335 blended.
- **It disguised inference as observation.** The 1,104 name-matched `ports` edges sat inside
  `graph.json` next to real AST edges and rendered identically to anything not checking the
  confidence field.
- **It hid the missing oracle** (failure mode 5). Nothing in its coverage table looked wrong.
- **It bought little.** Its one showcase cross-language traversal was, minus the inferred hop, just a
  C++-internal traversal.

Split, the inference lives in a 46 KB JSON you can read, correct, and version. Correcting a mapping
is a one-line edit instead of a 40k-node rebuild.

## Working with `port_map.json`

Each entry:

```json
{ "rust": "cashflows/iborcoupon", "quantlib": "cashflows/iborcoupon",
  "match": "exact-path", "confidence": 0.95, "verified": false,
  "oracle": "cashflows", "oracle_match": "dir-stem", "note": "" }
```

**When you verify a mapping against source, flip `verified: true` and add the `file:line` you read to
`note`.** That check then compounds for every future reader instead of being repeated. Right now
**1 of 257** entries is verified, so treat the map as a routing table someone still has to walk.

`port_map.json` is tracked precisely because it carries this human knowledge. The graphs themselves
are disposable and gitignored.

## How to update

Run these from the repo root. All of it is AST-only: **no LLM, no API key, no token cost.** The
extraction cache means only changed files are re-read.

### After you change Rust code (the common case)

```bash
python graphify/scripts/build_rust.py    # ~9s warm; re-extracts only changed files
```

That is the whole loop. Do it when you have added or moved types, modules, or call sites; skip it for
edits that do not change structure (a formula body, a test assertion).

**Do not run `graphify update .` from the repo root.** It would rescan the whole repo, pull the 271
markdown files under `.agents/` into the Rust graph, and stop it being language-pure. The scripts
here are the supported path.

### After you port a module (also update the map)

```bash
python graphify/scripts/build_rust.py
python graphify/scripts/port_map.py      # re-derives the Rust <-> QuantLib mapping
python graphify/scripts/coverage.py      # regenerates COVERAGE.md from it
```

### After you verify a mapping against source

This is the one that matters, and it is a **code edit, not a data edit**:

`port_map.py` re-derives every entry from names on each run, so **a `verified: true` you write
directly into `port_map.json` will be wiped by the next rebuild.** Corrections live in the `MANUAL`
list at the top of `graphify/scripts/port_map.py`:

```python
MANUAL = [
    {
        'rust': 'cashflows/iborleg', 'quantlib': 'cashflows/iborcoupon',
        'match': 'manual', 'confidence': 1.0, 'verified': True,
        'note': 'QuantLib declares IborLeg inside iborcoupon.hpp/.cpp; the Rust port lives in '
                'its own file (iborleg.rs, PR #311). Path matching cannot see this.',
    },
]
```

Add an entry, cite the `file:line` you actually read in `note`, then rerun `port_map.py` and
`coverage.py`. The check now compounds for every future reader.

### After QuantLib itself changes (rare - it is a pinned symlink)

```bash
GRAPHIFY_OUT=graphify-out-ql python graphify/scripts/build_ql.py
python graphify/scripts/port_map.py && python graphify/scripts/coverage.py
```

`GRAPHIFY_OUT` must be set **before** the process starts: graphify reads it once at import. Setting it
from inside Python does nothing.

### Rebuilding from scratch

Delete `graphify-out/` and/or `graphify-out-ql/` and rerun the build scripts. Note the shrink guard:
graphify refuses to overwrite an existing `graph.json` with a smaller one, so a graph that legitimately
shrinks must have its old file removed first. `build_rust.py` and `build_ql.py` handle their own case.

### Sanity checks after any rebuild

- `build_ql.py` aborts if `test-suite/` yields zero nodes. If you fork these scripts, **keep an
  assertion on node counts** - the extractor can fail silently (failure mode 5).
- Expected magnitudes: Rust ~5.8k nodes / ~225 communities; QuantLib ~39k nodes with ~2.5k of them
  from `test-suite/`. A large drop means extraction failed, not that the code shrank.
