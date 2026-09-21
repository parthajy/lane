"""Pull a spread of cleaned captures out of a COPY of memory.db for bench.py.

usage: sqlite3 "$HOME/Library/Application Support/com.reattend.mac/memory.db" ".backup copy.db"
       python3 extract_samples.py copy.db   # writes samples.json next to this file, then delete copy.db
Samples contain your own screen text: never commit samples.json.
"""
import json, os, sqlite3, sys

db = sys.argv[1]
c = sqlite3.connect(db)
rows = c.execute("""select a.id, a.app_name, a.window_title, a.url, s.text
 from snapshots s join activities a on a.id = s.activity_id
 order by length(s.text) desc""").fetchall()
seen, out = set(), []
for aid, app, title, url, text in rows:
    src = url.split("/")[2] if (url or "").startswith("http") else app
    if src in seen:
        continue
    seen.add(src)
    out.append({"activity": aid, "app": app, "title": title, "url": url, "text": text[:2500]})
    if len(out) >= 8:
        break
json.dump(out, open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "samples.json"), "w"), indent=1)
print(len(out), "samples")
