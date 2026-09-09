#!/usr/bin/env python3
"""Offline, paired parser comparison; never loads a model provider."""
from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess

from text2kg_general import metric, score_suite, sha256_file, sha256_bytes, validate_manifest

LABELS = ('strict', 'relation_filtered')


def aggregate(rows):
    return {label: metric(*(sum(row[label][key] for row in rows)
                            for key in ('tp', 'fp', 'fn'))) for label in LABELS}


def regressions(before, after):
    return [label for label in LABELS if after[label]['f1'] < before[label]['f1']]


def compare(dataset, manifest_path, sample):
    manifest = json.loads(manifest_path.read_text())
    commit = subprocess.run(['git', '-C', str(dataset), 'rev-parse', 'HEAD'],
                            check=True, capture_output=True, text=True).stdout.strip()
    if commit != manifest['dataset']['commit']:
        raise ValueError('dataset commit mismatch')
    validate_manifest(dataset, manifest)
    selection_path = sample / 'selection.json'
    selection = json.loads(selection_path.read_text())['ids']
    ids = set(selection)
    if not ids or len(ids) != len(selection):
        raise ValueError('empty or duplicate selected IDs')
    stored = json.loads((sample / 'score/cases.json').read_text())
    if {r['id'] for r in stored} != ids or len(stored) != len(ids):
        raise ValueError('published cases do not exactly cover selection')
    seen = set()
    response_hashes = {}
    models, providers = set(), set()
    for path in sorted((sample / 'responses').rglob('*.jsonl')):
        response_hashes[str(path.relative_to(sample))] = sha256_file(path)
        for line in path.read_text().splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            if row['id'] in seen:
                raise ValueError('duplicate response ID')
            seen.add(row['id'])
            models.add(str(row.get('model', 'UNKNOWN')))
            providers.add(str(row.get('provider', 'UNKNOWN')))
            if row.get('response_sha256') != sha256_bytes(row['raw_response'].encode()):
                raise ValueError('raw response hash mismatch')
    if seen != ids:
        raise ValueError('responses do not exactly cover selection')

    scored = {}
    for parser in ('strict-v1', 'fenced-json-v2'):
        full = score_suite(dataset, manifest, sample / 'responses', 'L1', parser=parser)
        cases = [r for r in full['cases'] if r['id'] in ids]
        stages = [r for r in full['stages'] if r['id'] in ids]
        if len(cases) != len(ids):
            raise ValueError('scorer did not cover every selected ID')
        scored[parser] = {'cases': cases, 'stages': stages, 'pipeline': full['pipeline']}
    old, new = scored['strict-v1'], scored['fenced-json-v2']
    if sorted(old['cases'], key=lambda r: r['id']) != sorted(stored, key=lambda r: r['id']):
        raise ValueError('legacy replay differs from the published baseline')

    groups = []
    for corpus, ontology in sorted({(r['corpus'], r['ontology']) for r in old['cases']}):
        before = [r for r in old['cases'] if (r['corpus'], r['ontology']) == (corpus, ontology)]
        after = [r for r in new['cases'] if (r['corpus'], r['ontology']) == (corpus, ontology)]
        b, a = aggregate(before), aggregate(after)
        groups.append({'corpus': corpus, 'ontology': ontology, 'cases': len(before),
                       'before': b, 'after': a, 'regressed_classes': regressions(b, a)})
    corpora = []
    for corpus in sorted({r['corpus'] for r in old['cases']}):
        before = [r for r in old['cases'] if r['corpus'] == corpus]
        after = [r for r in new['cases'] if r['corpus'] == corpus]
        b, a = aggregate(before), aggregate(after)
        corpora.append({'corpus': corpus, 'cases': len(before), 'before': b, 'after': a,
                        'regressed_classes': regressions(b, a)})
    stage_counts = {}
    for parser, value in scored.items():
        count = Counter()
        for row in value['stages']:
            count.update({k: v for k, v in row.items() if isinstance(v, int)})
        stage_counts[parser] = dict(sorted(count.items()))
    old_cases = {r['id']: r for r in old['cases']}
    changed = [{'id': r['id'], 'corpus': r['corpus'], 'ontology': r['ontology'],
                'before': {k: old_cases[r['id']][k] for k in LABELS},
                'after': {k: r[k] for k in LABELS}}
               for r in new['cases'] if r != old_cases[r['id']]]
    b, a = aggregate(old['cases']), aggregate(new['cases'])
    # Aggregate gains cannot hide a regressed ontology or scoring class.
    gate = not regressions(b, a) and not any(r['regressed_classes'] for r in corpora + groups)
    return {'schema_version': 1, 'experiment': 'offline paired parser replay',
            'selected_cases': len(ids), 'population_cases': manifest['totals']['cases'],
            'ontologies': len(groups), 'new_model_calls': 0,
            'original_models': sorted(models), 'original_providers': sorted(providers),
            'before_parser': 'strict-v1', 'after_parser': 'fenced-json-v2',
            'before_pipeline': old['pipeline'], 'after_pipeline': new['pipeline'],
            'dataset_commit': commit, 'manifest_sha256': sha256_file(manifest_path),
            'selection_sha256': sha256_file(selection_path),
            'response_files_sha256': response_hashes,
            'parser_implementation_sha256': sha256_file(Path(__file__).with_name('text2kg_general.py')),
            'comparison_implementation_sha256': sha256_file(Path(__file__)),
            'baseline_matches_published_cases': True, 'before': b, 'after': a,
            'stage_counts': stage_counts, 'corpora': corpora, 'per_ontology': groups,
            'changed_cases': changed,
            'unresolved_malformed_ids': sorted(r['id'] for r in new['stages'] if r.get('malformed_output')),
            'regression_gate': 'PASS' if gate else 'FAIL'}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dataset-root', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--sample', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        result = compare(args.dataset_root, args.manifest, args.sample)
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(2, f'comparison refused: {error}\n')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')
    print(json.dumps({k: result[k] for k in ('selected_cases', 'before', 'after',
                                           'regression_gate')}, indent=2))
    return 0 if result['regression_gate'] == 'PASS' else 3


if __name__ == '__main__':
    raise SystemExit(main())
