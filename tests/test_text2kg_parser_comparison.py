"""A better aggregate must not conceal a worse ontology in a paired replay."""
import importlib.util
import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from text2kg_general import score_suite, sha256_bytes

SPEC=importlib.util.spec_from_file_location('parser_comparison',Path(__file__).resolve().parents[1]/'scripts/compare-text2kg-parsers.py')
comparison=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(comparison)


@pytest.fixture
def sample(tmp_path,monkeypatch):
    dataset=tmp_path/'dataset';dataset.mkdir()
    root=tmp_path/'sample';(root/'score').mkdir(parents=True)
    entries=[];ids=[]
    for ontology,case_specs in [('one',[('a','alpha',False),('b','beta',True)]),
                                ('two',[('c','alpha',True)])]:
        gold=[];responses=[]
        for cid,predicted,wrapped in case_specs:
            ids.append(cid)
            gold.append({'id':cid,'sent':'Alice alpha Bob. Alice beta Bob.',
                         'triples':[{'sub':'Alice','rel':'alpha','obj':'Bob'}]})
            raw=json.dumps({'triples':[{'subject':'Alice','relation':predicted,
                                       'object':'Bob','evidence_span':f'Alice {predicted} Bob'}]})
            if wrapped:raw='```json\n'+raw+'\n```\nExplanation.'
            responses.append({'id':cid,'raw_response':raw,'response_sha256':sha256_bytes(raw.encode())})
        (dataset/f'{ontology}-gold.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in gold))
        (dataset/f'{ontology}.json').write_text(json.dumps({'relations':[{'label':'alpha'},{'label':'beta'}]}))
        out=root/'responses'/'corpus'/f'{ontology}.jsonl';out.parent.mkdir(parents=True,exist_ok=True)
        out.write_text(''.join(json.dumps(r)+'\n' for r in responses))
        entries.append({'id':ontology,'corpus':'corpus','files':{
            'ontology':{'path':f'{ontology}.json'},'gold':{'path':f'{ontology}-gold.jsonl'}}})
    manifest={'dataset':{'commit':'pinned'},'totals':{'cases':3},'ontologies':entries}
    mp=tmp_path/'manifest.json';mp.write_text(json.dumps(manifest))
    (root/'selection.json').write_text(json.dumps({'ids':ids}))
    baseline=score_suite(dataset,manifest,root/'responses','L1')
    (root/'score/cases.json').write_text(json.dumps(baseline['cases']))
    # The real CLI independently pins and validates the complete dataset. This
    # fixture targets the comparison; manifest integrity has separate tests.
    monkeypatch.setattr(comparison,'validate_manifest',lambda *args:None)
    monkeypatch.setattr(comparison.subprocess,'run',lambda *args,**kw:SimpleNamespace(stdout='pinned\n'))
    return dataset,mp,root,tmp_path/'out.json'


def test_ontology_regression_fails_even_when_corpus_and_global_improve(sample):
    dataset,mp,root,out=sample
    result=comparison.compare(dataset,mp,root)
    assert result['after']['strict']['f1']>result['before']['strict']['f1']
    assert result['corpora'][0]['regressed_classes']==[]
    assert result['per_ontology'][0]['regressed_classes']==['strict']
    assert result['regression_gate']=='FAIL'
    assert comparison.main(['--dataset-root',str(dataset),'--manifest',str(mp),
                            '--sample',str(root),'--output',str(out)])==3
    assert json.loads(out.read_text())['regression_gate']=='FAIL'


def test_response_tampering_is_refused(sample):
    dataset,mp,root,_=sample
    p=root/'responses/corpus/one.jsonl'
    rows=[json.loads(l) for l in p.read_text().splitlines()]
    rows[0]['raw_response']='{"triples":[]}'
    p.write_text(''.join(json.dumps(r)+'\n' for r in rows))
    with pytest.raises(ValueError,match='response hash mismatch'):
        comparison.compare(dataset,mp,root)


def test_changed_baseline_is_refused(sample):
    dataset,mp,root,_=sample
    p=root/'score/cases.json';rows=json.loads(p.read_text());rows[0]['strict']['tp']=999
    p.write_text(json.dumps(rows))
    with pytest.raises(ValueError,match='differs from the published baseline'):
        comparison.compare(dataset,mp,root)


def test_missing_selected_response_is_refused(sample):
    dataset,mp,root,_=sample
    (root/'responses/corpus/two.jsonl').unlink()
    with pytest.raises(ValueError,match='responses do not exactly cover selection'):
        comparison.compare(dataset,mp,root)
