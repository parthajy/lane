#!/usr/bin/env python3
"""Run evals/ask_cases.json through the running Lane (rat-mac --ask) and grade the answers.
usage: python3 evals/run_asks.py [~/Applications/Lane.app]"""
import json, os, subprocess, sys, time, glob, functools
print = functools.partial(print, flush=True)
app = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser('~/Applications/Lane.app')
binary = os.path.join(app, 'Contents/MacOS/rat-mac')
asks = os.path.expanduser('~/Library/Application Support/so.lane.app/asks')
cases = json.load(open(os.path.join(os.path.dirname(__file__), 'ask_cases.json')))
passed = 0
for c in cases:
    before = set(glob.glob(os.path.join(asks, '*.txt')))
    subprocess.run([binary, '--ask', c['q']], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    path = None
    for _ in range(420):
        time.sleep(1)
        new = set(glob.glob(os.path.join(asks, '*.txt'))) - before
        # Answers can arrive late from an earlier, slower question: only the
        # file that carries this question counts.
        mine = [p for p in new if open(p).read().startswith('Q: ' + c['q'] + '\n')]
        if mine:
            path = sorted(mine)[-1]; break
    if not path:
        print(f"TIMEOUT  {c['q']}"); continue
    text = open(path).read()
    answer = text.split('\nA: ', 1)[1].split('\nSOURCES:')[0].strip() if '\nA: ' in text else text
    fixed = 'SCOPE_FIXED: true' in text
    low = answer.lower()
    first = low.split('. ')[0]
    ok = True; why = []
    for w in c.get('must_not_lead_with', []):
        if w.lower() in first: ok = False; why.append(f"leads with '{w}'")
    for w in c.get('must_not_contain', []):
        if w.lower() in low: ok = False; why.append(f"contains '{w}'")
    if c.get('must_contain_any') and not any(w.lower() in low for w in c['must_contain_any']):
        ok = False; why.append(f"none of {c['must_contain_any']}")
    passed += ok
    print(f"{'PASS' if ok else 'FAIL'}{' (rewritten)' if fixed else ''}  {c['q']}\n      {answer[:300]}{' … ' + '; '.join(why) if why else ''}\n")
print(f"{passed}/{len(cases)} passed")
