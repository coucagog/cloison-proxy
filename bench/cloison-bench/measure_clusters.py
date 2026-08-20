#!/usr/bin/env python3
"""Mesure clusters de fusion : nb de sources × score, TP sur docs PII, FP sur non-PII.
Usage (dans bench/cloison-bench) : CLOISON_OFFLINE=1 CLOISON_AFRICAN_MODEL=afroxlmr \
    /path/.venv/bin/python3 measure_clusters.py"""
import json, os, sys
from collections import Counter
sys.path.insert(0, '../../services/cloison-detect')
from src.detect_service import DetectRequest, DetectService
from src.spans import Policy, SessionContext
from src.config import Config, AfricanConfig

AFR = os.environ.get("CLOISON_AFRICAN_MODEL", "serengeti")
svc = DetectService(Config(african=AfricanConfig(model_name=AFR)))
policy = Policy()

# Instrumentation : nombre de sources par cluster, mémorisé par id(span)
nsrc_of = {}
orig_resolve = svc._resolve_cluster
def wrapped(cluster, policy_):
    out = orig_resolve(cluster, policy_)
    if out is not None:
        nsrc_of[id(out)] = len({s.source.split(":")[0] for s in cluster})
    return out
svc._resolve_cluster = wrapped

gold = [json.loads(l) for l in open('results/dataset.jsonl')]
def norm(t, s, e): return t[s:e].lower().replace(' ', '')

tp = []
fp = []
for d in gold:
    text = d['text']
    resp = svc.detect(DetectRequest(text=text, locale='fr-SN', policy=policy, core_spans=(), session=SessionContext()))
    goldset = {(norm(text, e['start'], e['end']), e['type']) for e in d.get('entities', [])}
    for s in resp.spans:
        if s.type.value not in ('PERSON', 'LOC'): continue
        key = (norm(text, s.start, s.end), s.type.value)
        nsrc = nsrc_of.get(id(s), 1)
        if key in goldset:
            tp.append((nsrc, round(s.score, 2), s.type.value))
        elif not d['entities']:
            fp.append((nsrc, round(s.score, 2), s.type.value))

print('TP (docs PII):', len(tp))
c = Counter((n, sc) for n, sc, _ in tp)
print('  par (nsrc, score):', dict(sorted(c.items())))
print('  TP mono-source >= 0.9 :', sum(1 for n, sc, _ in tp if n == 1 and sc >= 0.9))
print('  TP mono-source <  0.9 :', sum(1 for n, sc, _ in tp if n == 1 and sc < 0.9))
print('FP (docs non-PII):', len(fp))
c2 = Counter((n, sc) for n, sc, _ in fp)
print('  par (nsrc, score):', dict(sorted(c2.items())))
print('  FP mono-source >= 0.9 :', sum(1 for n, sc, _ in fp if n == 1 and sc >= 0.9))
print('  FP mono-source <  0.9 :', sum(1 for n, sc, _ in fp if n == 1 and sc < 0.9))
