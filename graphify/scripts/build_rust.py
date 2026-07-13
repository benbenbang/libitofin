"""Rust graph: crates/ only, AST edges only. Writes graphify-out/ (the default query target).

    python graphify/scripts/build_rust.py

AST-only: no LLM, no API key. Run from the repo root.
"""
import json
from collections import Counter
from pathlib import Path

from graphify.extract import collect_files, extract
from graphify.build import build_from_json
from graphify.cluster import cluster, score_all
from graphify.analyze import god_nodes, surprising_connections, suggest_questions
from graphify.report import generate
from graphify.export import to_json

REPO = Path(__file__).resolve().parents[2]
OUT = 'graphify-out'

PRETTY = {
    'time/calendars': 'National Holiday Calendars', 'time/daycounters': 'Day Counters',
    'time': 'Date and Calendar Core', 'math/interpolations': 'Interpolation Schemes',
    'math/distributions': 'Probability Distributions', 'math/integrals': 'Numerical Integration',
    'math/optimization': 'Optimization and Solvers', 'math/statistics': 'Statistics Accumulators',
    'math/randomnumbers': 'Random Number Generators', 'math/matrixutilities': 'Matrix Utilities',
    'math/ode': 'ODE Integration', 'math': 'Math Primitives',
    'termstructures/volatility': 'Volatility Term Structures',
    'termstructures/yields': 'Yield Term Structures', 'termstructures': 'Term Structure Base',
    'pricingengines/vanilla': 'Vanilla Pricing Engines', 'pricingengines/bond': 'Bond Pricing Engines',
    'pricingengines': 'Pricing Engine Core', 'instruments': 'Instruments and Payoffs',
    'cashflows': 'Cashflows and Coupons', 'processes': 'Stochastic Processes',
    'indexes': 'Rate Indexes', 'patterns': 'Observer Pattern Core', 'methods': 'Numerical Methods',
    'currencies': 'Currency Definitions', 'quotes': 'Quotes', 'utilities': 'Utilities',
    '': 'Core Library',
}


def main():
    files = [p for p in collect_files(REPO / 'crates') if p.suffix == '.rs']
    ext = extract(files, cache_root=REPO / 'crates')
    for n in ext['nodes']:
        n['repo'] = 'libitofin'
    ext['hyperedges'] = []
    print(f'rust: {len(files)} files -> {len(ext["nodes"])} nodes')
    if not ext['nodes']:
        raise SystemExit('ERROR: extraction produced no nodes')

    G = build_from_json(ext, root=str(REPO / 'crates'), directed=False)
    comms = cluster(G)
    coh = score_all(G, comms)

    labels = {}
    for cid, ns in comms.items():
        dirs, stems = Counter(), Counter()
        for nid in ns:
            sf = G.nodes[nid].get('source_file') or ''
            if not sf.startswith('libitofin/src/'):
                continue
            body = sf[len('libitofin/src/'):]
            dirs['/'.join(body.split('/')[:-1])] += 1
            stems[body.split('/')[-1].rsplit('.', 1)[0]] += 1
        if not dirs:
            labels[cid] = f'Community {cid}'
            continue
        d = dirs.most_common(1)[0][0]
        name = PRETTY.get(d) or (d.replace('/', ' ').title() if d else 'Core')
        labels[cid] = f'{name} - {stems.most_common(1)[0][0]}' if stems else name

    gods = god_nodes(G)
    ok = to_json(G, comms, f'{OUT}/graph.json', community_labels=labels)
    qs = suggest_questions(G, comms, labels)
    rep = generate(G, comms, coh, labels, gods, surprising_connections(G, comms),
                   {'total_files': len(files), 'total_words': 0, 'files': {'code': []},
                    'skipped_sensitive': []},
                   {'input': 0, 'output': 0}, str(REPO / 'crates'), suggested_questions=qs)
    Path(f'{OUT}/GRAPH_REPORT.md').write_text(rep, encoding='utf-8')
    Path(f'{OUT}/.graphify_labels.json').write_text(
        json.dumps({str(k): v for k, v in labels.items()}, ensure_ascii=False), encoding='utf-8')
    print(f'RUST graph: {G.number_of_nodes()} nodes, {G.number_of_edges()} edges, '
          f'{len(comms)} communities, written={ok}')
    print('  god nodes:', ', '.join(f"{g['label']}({g['degree']})" for g in gods[:6]))


if __name__ == '__main__':
    main()
