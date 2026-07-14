# QuantLib -> libitofin port coverage

Generated 2026-07-14 from `port_map.json`.

> **This is a hypothesis layer, not an audit.** Every mapping below comes from matching *names*
> and *paths* between the two trees - never from reading behaviour. It tells you which file to
> open. It does not tell you whether the port is correct, complete, or faithful. Before any
> claim reaches a ticket, verify it against `QuantLib/` source and cite `file:line` - then flip
> `verified: true` on that entry so the next reader inherits the check.

## Coverage by subsystem

| QuantLib subsystem | modules | mapped | coverage |
| --- | ---: | ---: | ---: |
| experimental | 225 | 1 | 0% |
| math | 167 | 50 | 30% |
| pricingengines | 156 | 5 | 3% |
| methods | 138 | 1 | 1% |
| models | 138 | 0 | 0% |
| termstructures | 122 | 18 | 15% |
| instruments | 89 | 9 | 10% |
| time | 78 | 78 | 100% |
| indexes | 65 | 5 | 8% |
| (root) | 45 | 15 | 33% |
| cashflows | 35 | 12 | 34% |
| processes | 21 | 1 | 5% |
| legacy | 15 | 0 | 0% |
| quotes | 11 | 5 | 45% |
| utilities | 10 | 4 | 40% |
| currencies | 7 | 0 | 0% |
| patterns | 5 | 3 | 60% |
| **TOTAL** | **1327** | **207** | **16%** |

## Map quality

- Rust modules: **269** - of which **61** have no QuantLib counterpart by name (internal helpers, or a port under a different name).
- QuantLib modules with no Rust counterpart: **1120** (the `unported` list in `port_map.json` - a candidate backlog, not a validated one).
- Entries verified against source by a human: **1** of 269.

## Oracle tests

A ticket is done when the matching `test-suite/*.cpp` cases pass. QuantLib's test-suite is
organised by **topic**, not by module (`calendars.cpp` covers every calendar), so the oracle
link is coarser than the module link:

| how the oracle was matched | entries | trust |
| --- | ---: | --- |
| `stem` / `stem-plural` (module name == test name) | 23 | good |
| `dir-stem` (parent directory == test name) | 103 | coarse - the test file covers a whole family |
| none found | 143 | no oracle identified; find it by hand |

## Known false positives this map cannot see

- **Relocated symbols.** QuantLib declares `IborLeg` inside `iborcoupon.hpp`; the Rust port
  gives it its own `cashflows/iborleg.rs` (PR #311). Path matching reads that as "missing".
  It is hand-corrected in `port_map.json` as a `manual` entry - add more the same way.
- **Private C++ members** (`caps_`, `gearings_`, `index_`) look unported and never will be.
- **Module-level, not feature-level.** A partially ported module scores the same as a complete
  one. `time` reads 100% because every header has a counterpart, not because every behaviour does.

## Querying the graphs

The two graphs are language-pure and kept separate on purpose - joining them with inferred edges
made the god-node and community analytics ambiguous (a C++ `Date()` and a Rust `Date` both ranked
top-5, indistinguishable to fuzzy lookup).

```bash
graphify query "how does Settings notify observers"      # Rust graph (default)
graphify explain "libitofin_src_settings_settings"

GRAPHIFY_OUT=graphify-out-ql graphify explain "ql_cashflows_iborcoupon_iborcoupon"   # QuantLib + test-suite
GRAPHIFY_OUT=graphify-out-ql graphify path "IborCoupon" "FloatingRateCoupon"
```

Cross-language questions are answered by looking the module up in `port_map.json`, then querying
the other graph - the join stays visible as an inference every time it is used.

## Regenerating

Both graphs are AST-only (no LLM, no API key, fully cached) and are gitignored. `port_map.json`
is **not** gitignored: it carries the `verified` flags and `manual` corrections, which are human
knowledge and must survive a rebuild.
