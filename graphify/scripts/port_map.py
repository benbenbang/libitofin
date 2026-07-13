"""Derive the Rust <-> QuantLib correspondence as a standalone, correctable data layer.

This is a HYPOTHESIS layer: name/path matching, not semantic analysis. It is deliberately
NOT written as edges into either graph, so observed structure stays separable from inference.
"""
import json, re
from datetime import datetime, timezone
from pathlib import Path
from collections import defaultdict

RS = json.loads(Path('graphify-out/graph.json').read_text(encoding='utf-8'))
QL = json.loads(Path('graphify-out-ql/graph.json').read_text(encoding='utf-8'))

# Corrections for cases the name-matcher provably gets wrong. Each was verified by reading source.
MANUAL = [
    {
        'rust': 'cashflows/iborleg', 'quantlib': 'cashflows/iborcoupon',
        'match': 'manual', 'confidence': 1.0, 'verified': True,
        'note': 'QuantLib declares IborLeg inside iborcoupon.hpp/.cpp; the Rust port lives in its '
                'own file (iborleg.rs, PR #311). Path matching cannot see this.',
    },
]


def rs_key(sf):
    if not sf.startswith('libitofin/src/'):
        return None
    k = re.sub(r'\.rs$', '', sf[len('libitofin/src/'):])
    return None if k in ('lib', 'mod') or k.endswith('/mod') else k


def ql_key(sf):
    if not sf.startswith('ql/'):
        return None
    k = re.sub(r'\.(hpp|cpp|h)$', '', sf[3:])
    return None if k.endswith('/all') else k


def ts_key(sf):
    return re.sub(r'\.(cpp|hpp)$', '', sf[len('test-suite/'):]) if sf.startswith('test-suite/') else None


rs_mods, ql_mods, ts_mods = set(), set(), set()
for n in RS['nodes']:
    k = rs_key(n.get('source_file') or '')
    if k:
        rs_mods.add(k)
for n in QL['nodes']:
    sf = n.get('source_file') or ''
    k = ql_key(sf)
    if k:
        ql_mods.add(k)
    t = ts_key(sf)
    if t:
        ts_mods.add(t)

# ---- oracle index: test-suite stem -> test file
ts_by_stem = defaultdict(list)
for t in ts_mods:
    ts_by_stem[t.split('/')[-1]].append(t)


def find_oracle(rk, qk):
    """Which test-suite file is the oracle for this module?

    QuantLib's test-suite is organised by TOPIC, not by module: calendars.cpp is the oracle for
    every calendar, distributions.cpp for every distribution. So fall back from the module stem
    to the parent-directory stem, which is a coarser (and weaker) claim.
    """
    for cand in (qk.split('/')[-1], rk.split('/')[-1]):
        for stem, method in ((cand, 'stem'), (cand + 's', 'stem-plural')):
            hits = ts_by_stem.get(stem, [])
            if len(hits) == 1:
                return hits[0], method
    # topic-level: the module's parent directory (time/calendars/austria -> calendars.cpp)
    for key in (qk, rk):
        parts = key.split('/')
        if len(parts) < 2:
            continue
        parent = parts[-2]
        for stem, method in ((parent, 'dir-stem'), (parent + 's', 'dir-stem-plural')):
            hits = ts_by_stem.get(stem, [])
            if len(hits) == 1:
                return hits[0], method
    return None, None


ql_by_stem = defaultdict(list)
for k in ql_mods:
    ql_by_stem[k.split('/')[-1]].append(k)

entries, claimed = [], set()
for m in MANUAL:
    o, om = find_oracle(m['rust'], m['quantlib'])
    entries.append({**m, 'oracle': o, 'oracle_match': om})
    claimed.add(m['rust'])

for rk in sorted(rs_mods):
    if rk in claimed:
        continue
    if rk in ql_mods:
        qk, method, conf = rk, 'exact-path', 0.95
    else:
        cands = ql_by_stem.get(rk.split('/')[-1], [])
        if len(cands) != 1:
            entries.append({'rust': rk, 'quantlib': None, 'match': 'none', 'confidence': 0.0,
                            'verified': False, 'oracle': None, 'oracle_match': None,
                            'note': 'no QuantLib module matched by path or stem'})
            continue
        qk, method, conf = cands[0], 'stem', 0.75
    o, om = find_oracle(rk, qk)
    entries.append({'rust': rk, 'quantlib': qk, 'match': method, 'confidence': conf,
                    'verified': False, 'oracle': o, 'oracle_match': om, 'note': ''})

mapped_ql = {e['quantlib'] for e in entries if e['quantlib']}
unported = sorted(ql_mods - mapped_ql)

by_sub = defaultdict(lambda: [0, 0])
for k in ql_mods:
    top = '(root)' if '/' not in k else k.split('/')[0]
    by_sub[top][0] += 1
    if k in mapped_ql:
        by_sub[top][1] += 1

doc = {
    'generated': datetime.now(timezone.utc).isoformat(),
    'status': 'HYPOTHESIS - name/path matching, not semantic analysis. Verify against QuantLib '
              'source (file:line) before citing in a ticket. Flip "verified" once you have.',
    'method': {
        'exact-path': 'libitofin/src/<X>.rs <-> ql/<X>.hpp|.cpp  (confidence 0.95)',
        'stem': 'basename matched uniquely, path differed (confidence 0.75)',
        'manual': 'hand-corrected after reading source (confidence 1.0)',
        'none': 'no QuantLib counterpart found by name',
    },
    'graphs': {
        'rust': 'graphify-out/graph.json',
        'quantlib': 'graphify-out-ql/graph.json  (query with GRAPHIFY_OUT=graphify-out-ql)',
    },
    'summary': {
        'rust_modules': len(rs_mods),
        'quantlib_modules': len(ql_mods),
        'test_suite_files': len(ts_mods),
        'mapped': len(mapped_ql),
        'verified': sum(1 for e in entries if e['verified']),
        'unported_quantlib_modules': len(unported),
        'coverage_by_subsystem': {k: {'modules': v[0], 'mapped': v[1]} for k, v in sorted(by_sub.items())},
    },
    'entries': entries,
    'unported': unported,
}
Path('graphify/port_map.json').write_text(json.dumps(doc, indent=2, ensure_ascii=False), encoding='utf-8')

s = doc['summary']
print(f"rust modules      : {s['rust_modules']}")
print(f"quantlib modules  : {s['quantlib_modules']}")
print(f"test-suite files  : {s['test_suite_files']}")
print(f"mapped            : {s['mapped']}  (verified: {s['verified']})")
print(f"unported QL mods  : {s['unported_quantlib_modules']}")
print(f"rust with NO match: {sum(1 for e in entries if e['match'] == 'none')}")
print(f"with oracle test  : {sum(1 for e in entries if e['oracle'])}")
print()
for e in entries[:6]:
    print(f"  {e['rust']:<34} -> {str(e['quantlib']):<34} [{e['match']}] oracle={e['oracle']}")
