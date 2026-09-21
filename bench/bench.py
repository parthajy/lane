"""Benchmark small local models on real cleaned captures via Ollama.

For each sample: one structured 'triage + extract' call. Measures latency,
tokens/s, JSON validity, and faithfulness (every extracted string must occur
literally in the source text — anything else counts as hallucination).

usage: python3 bench.py qwen3:4b [more models...]
samples.json (private, not committed) is produced by extract_samples.py from a copy of memory.db.
"""
import json, sys, time, urllib.request, re, os

HERE = os.path.dirname(os.path.abspath(__file__))
SAMPLES = json.load(open(os.path.join(HERE, "samples.json")))

SCHEMA = {
    "type": "object",
    "properties": {
        "keep": {"type": "boolean"},
        "kind": {"type": "string", "enum": ["email", "chat", "document", "article", "form", "search", "social", "table", "code", "other"]},
        "title": {"type": "string"},
        "summary": {"type": "string"},
        "people": {"type": "array", "items": {"type": "string"}},
        "organizations": {"type": "array", "items": {"type": "string"}},
        "dates": {"type": "array", "items": {"type": "string"}},
        "numbers": {"type": "array", "items": {"type": "string"}},
        "confidence": {"type": "number"},
    },
    "required": ["keep", "kind", "title", "summary", "people", "organizations", "dates", "numbers", "confidence"],
}

SYSTEM = (
    "You are a memory assistant running on the user's own computer. You are given text that was on "
    "their screen. Decide if it is worth remembering (keep=false for menus, ads, empty pages, generic "
    "feeds), classify it, give a short title, a 1-2 sentence factual summary, and extract people, "
    "organizations, dates and numbers. Copy names, dates and numbers EXACTLY as they appear in the text. "
    "Never invent anything. confidence is 0-1 for how sure you are about keep."
)


def ask(model, sample):
    prompt = f"App: {sample['app']}\nWindow: {sample['title']}\nURL: {sample['url'] or ''}\n\nTEXT:\n{sample['text']}"
    body = {
        "model": model,
        "messages": [{"role": "system", "content": SYSTEM}, {"role": "user", "content": prompt}],
        "format": SCHEMA,
        "stream": False,
        "think": False,
        "options": {"temperature": 0.1, "num_ctx": 4096},
    }
    req = urllib.request.Request("http://localhost:11434/api/chat", data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    t = time.time()
    with urllib.request.urlopen(req, timeout=600) as r:
        out = json.load(r)
    return out, time.time() - t


def norm(s):
    return re.sub(r"\s+", " ", s).strip().lower()


def faithful(items, source):
    src = norm(source)
    ok = [i for i in items if i and norm(i) in src]
    return ok, [i for i in items if i and norm(i) not in src]


def main():
    models = sys.argv[1:] or ["qwen3:4b"]
    for model in models:
        print(f"\n===== {model}")
        # warm up (model load) once, not counted
        try:
            ask(model, {"app": "x", "title": "x", "url": "", "text": "hello"})
        except Exception as e:
            print("load failed:", e)
            continue
        tot_lat = tot_tok = tot_ev = 0.0
        valid = 0
        ent_ok = ent_bad = 0
        rows = []
        for s in SAMPLES:
            try:
                out, lat = ask(model, s)
            except Exception as e:
                rows.append((s, None, str(e)))
                continue
            content = out["message"]["content"]
            tok = out.get("eval_count", 0)
            ev = out.get("eval_duration", 0) / 1e9
            tot_lat += lat; tot_tok += tok; tot_ev += ev
            try:
                j = json.loads(content)
                valid += 1
            except Exception:
                rows.append((s, None, "invalid JSON: " + content[:120]))
                continue
            items = j.get("people", []) + j.get("organizations", []) + j.get("dates", []) + j.get("numbers", [])
            # The window title and URL were part of the prompt, so strings from them are faithful too.
            ok, bad = faithful(items, f"{s['title']}\n{s['url'] or ''}\n{s['text']}")
            ent_ok += len(ok); ent_bad += len(bad)
            rows.append((s, j, {"lat": lat, "tok": tok, "tps": tok / ev if ev else 0, "bad": bad}))
        for s, j, m in rows:
            src = (s["url"] or s["app"])[:38]
            if j is None:
                print(f"  {src:38} ERROR {m}")
                continue
            print(f"  {src:38} keep={j['keep']!s:5} kind={j['kind']:9} conf={j['confidence']:.2f} {m['lat']:5.1f}s {m['tps']:4.1f} tok/s")
            print(f"     title: {j['title'][:90]}")
            print(f"     summary: {j['summary'][:160]}")
            print(f"     people={j['people']} orgs={j['organizations']} dates={j['dates']} numbers={j['numbers']}")
            if m["bad"]:
                print(f"     NOT IN SOURCE: {m['bad']}")
        n = len(SAMPLES)
        print(f"\n  valid JSON {valid}/{n} · mean latency {tot_lat / max(n,1):.1f}s · {tot_tok / max(tot_ev, 1e-9):.1f} tok/s generation"
              f" · extracted strings in source: {ent_ok}/{ent_ok + ent_bad}")


if __name__ == "__main__":
    main()
