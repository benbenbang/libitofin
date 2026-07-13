"""QuantLib graph: ql/ implementation + test-suite/ oracle, in one language-pure graph.

Run with GRAPHIFY_OUT=graphify-out-ql set BEFORE the process starts (paths.py reads it at import).
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

QL = Path('/Users/bn/Git/benbenbang/QuantLib')
OUT = 'graphify-out-ql'


def main():
    impl = [p for p in collect_files(QL / 'ql') if p.suffix in ('.hpp', '.cpp', '.h')]
    tests = [p for p in collect_files(QL / 'test-suite') if p.suffix in ('.cpp', '.hpp')]
    print(f'ql/: {len(impl)} files | test-suite/: {len(tests)} files')

    a = extract(impl, cache_root=QL)
    # test-suite crashed the worker pool when batched with ql/ (graphify #1666); run it sequentially
    b = extract(tests, cache_root=QL, parallel=False)
    print(f'  impl: {len(a["nodes"])} nodes | tests: {len(b["nodes"])} nodes')

    seen, nodes = set(), []
    for n in a['nodes'] + b['nodes']:
        if n['id'] in seen:
            continue
        seen.add(n['id'])
        sf = n.get('source_file') or ''
        n['repo'] = 'quantlib-test-suite' if sf.startswith('test-suite/') else 'quantlib'
        nodes.append(n)
    edges = [e for e in a['edges'] + b['edges']
             if e.get('source') in seen and e.get('target') in seen]
    ext = {'nodes': nodes, 'edges': a['edges'] + b['edges'], 'hyperedges': [],
           'input_tokens': 0, 'output_tokens': 0}

    ts_nodes = sum(1 for n in nodes if n['repo'] == 'quantlib-test-suite')
    print(f'merged: {len(nodes)} nodes ({ts_nodes} from test-suite), {len(ext["edges"])} raw edges')
    if ts_nodes == 0:
        raise SystemExit('ERROR: test-suite produced no nodes - aborting rather than shipping a graph '
                         'that silently lacks the oracle')

    G = build_from_json(ext, root=str(QL), directed=False)
    comms = cluster(G)
    coh = score_all(G, comms)

    src = {n: (G.nodes[n].get('source_file') or '') for n in G.nodes}
    labels = {}
    for cid, ns in comms.items():
        dirs, files = Counter(), Counter()
        for nid in ns:
            sf = src.get(nid, '')
            if not sf:
                continue
            tag = 'TEST' if sf.startswith('test-suite/') else 'QL'
            body = sf.split('/', 1)[1] if '/' in sf else sf
            dirs[(tag, '/'.join(body.split('/')[:-1]))] += 1
            files[body.split('/')[-1].rsplit('.', 1)[0]] += 1
        if not dirs:
            labels[cid] = f'Community {cid}'
            continue
        (tag, d), _ = dirs.most_common(1)[0]
        stem = files.most_common(1)[0][0] if files else ''
        name = d.replace('/', ' ').title() if d else 'Core'
        labels[cid] = f'{tag}: {name} - {stem}' if stem else f'{tag}: {name}'

    gods = god_nodes(G)
    surprises = surprising_connections(G, comms)
    qs = suggest_questions(G, comms, labels)
    ok = to_json(G, comms, f'{OUT}/graph.json', community_labels=labels)
    det = {'total_files': len(impl) + len(tests), 'total_words': 0,
           'files': {'code': []}, 'skipped_sensitive': []}
    rep = generate(G, comms, coh, labels, gods, surprises, det,
                   {'input': 0, 'output': 0}, str(QL), suggested_questions=qs)
    Path(f'{OUT}/GRAPH_REPORT.md').write_text(rep, encoding='utf-8')
    Path(f'{OUT}/.graphify_labels.json').write_text(
        json.dumps({str(k): v for k, v in labels.items()}, ensure_ascii=False), encoding='utf-8')
    print(f'QUANTLIB graph: {G.number_of_nodes()} nodes, {G.number_of_edges()} edges, '
          f'{len(comms)} communities, written={ok}')
    print('  god nodes:', ', '.join(f"{g['label']}({g['degree']})" for g in gods[:6]))


if __name__ == '__main__':
    main()
