import json
from pathlib import Path
from collections import Counter

pm = json.loads(Path('graphify/port_map.json').read_text(encoding='utf-8'))
s = pm['summary']
cov = s['coverage_by_subsystem']
rows = sorted(((v['modules'], v['mapped'], k) for k, v in cov.items()), reverse=True)
om = Counter(e['oracle_match'] for e in pm['entries'])

L = [
    '# QuantLib -> libitofin port coverage',
    '',
    f"Generated {pm['generated'][:10]} from `port_map.json`.",
    '',
    '> **This is a hypothesis layer, not an audit.** Every mapping below comes from matching *names*',
    '> and *paths* between the two trees - never from reading behaviour. It tells you which file to',
    '> open. It does not tell you whether the port is correct, complete, or faithful. Before any',
    '> claim reaches a ticket, verify it against `QuantLib/` source and cite `file:line` - then flip',
    '> `verified: true` on that entry so the next reader inherits the check.',
    '',
    '## Coverage by subsystem',
    '',
    '| QuantLib subsystem | modules | mapped | coverage |',
    '| --- | ---: | ---: | ---: |',
]
for n, hit, top in rows:
    if n < 5 and hit == 0:
        continue
    L.append(f'| {top} | {n} | {hit} | {100 * hit / n:.0f}% |')
L += [
    f"| **TOTAL** | **{s['quantlib_modules']}** | **{s['mapped']}** | "
    f"**{100 * s['mapped'] / s['quantlib_modules']:.0f}%** |",
    '',
    '## Map quality',
    '',
    f"- Rust modules: **{s['rust_modules']}** - of which **{sum(1 for e in pm['entries'] if e['match'] == 'none')}** "
    'have no QuantLib counterpart by name (internal helpers, or a port under a different name).',
    f"- QuantLib modules with no Rust counterpart: **{s['unported_quantlib_modules']}** (the `unported` "
    'list in `port_map.json` - a candidate backlog, not a validated one).',
    f"- Entries verified against source by a human: **{s['verified']}** of {len(pm['entries'])}.",
    '',
    '## Oracle tests',
    '',
    "A ticket is done when the matching `test-suite/*.cpp` cases pass. QuantLib's test-suite is",
    'organised by **topic**, not by module (`calendars.cpp` covers every calendar), so the oracle',
    'link is coarser than the module link:',
    '',
    '| how the oracle was matched | entries | trust |',
    '| --- | ---: | --- |',
    f"| `stem` / `stem-plural` (module name == test name) | {om.get('stem', 0) + om.get('stem-plural', 0)} | good |",
    f"| `dir-stem` (parent directory == test name) | {om.get('dir-stem', 0) + om.get('dir-stem-plural', 0)} | coarse - the test file covers a whole family |",
    f"| none found | {om.get(None, 0)} | no oracle identified; find it by hand |",
    '',
    '## Known false positives this map cannot see',
    '',
    '- **Relocated symbols.** QuantLib declares `IborLeg` inside `iborcoupon.hpp`; the Rust port',
    '  gives it its own `cashflows/iborleg.rs` (PR #311). Path matching reads that as "missing".',
    '  It is hand-corrected in `port_map.json` as a `manual` entry - add more the same way.',
    '- **Private C++ members** (`caps_`, `gearings_`, `index_`) look unported and never will be.',
    '- **Module-level, not feature-level.** A partially ported module scores the same as a complete',
    '  one. `time` reads 100% because every header has a counterpart, not because every behaviour does.',
    '',
    '## Querying the graphs',
    '',
    'The two graphs are language-pure and kept separate on purpose - joining them with inferred edges',
    'made the god-node and community analytics ambiguous (a C++ `Date()` and a Rust `Date` both ranked',
    'top-5, indistinguishable to fuzzy lookup).',
    '',
    '```bash',
    'graphify query "how does Settings notify observers"      # Rust graph (default)',
    'graphify explain "libitofin_src_settings_settings"',
    '',
    'GRAPHIFY_OUT=graphify-out-ql graphify explain "ql_cashflows_iborcoupon_iborcoupon"   # QuantLib + test-suite',
    'GRAPHIFY_OUT=graphify-out-ql graphify path "IborCoupon" "FloatingRateCoupon"',
    '```',
    '',
    'Cross-language questions are answered by looking the module up in `port_map.json`, then querying',
    'the other graph - the join stays visible as an inference every time it is used.',
    '',
    '## Regenerating',
    '',
    'Both graphs are AST-only (no LLM, no API key, fully cached) and are gitignored. `port_map.json`',
    'is **not** gitignored: it carries the `verified` flags and `manual` corrections, which are human',
    'knowledge and must survive a rebuild.',
]
Path('graphify/COVERAGE.md').write_text('\n'.join(L) + '\n', encoding='utf-8')
print('\n'.join(L[:22]))
