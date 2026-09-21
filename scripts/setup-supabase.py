#!/usr/bin/env python3
"""Put the schema and the licence keys into Supabase.

    SUPABASE_ACCESS_TOKEN=sbp_… python3 scripts/setup-supabase.py [--dry-run]

The token is a personal access token from
https://supabase.com/dashboard/account/tokens. It is read from the
environment and never written anywhere. Running this twice is safe: the
schema uses `create … if not exists` and `create or replace`, and keys are
inserted with `on conflict do nothing`.
"""

from __future__ import annotations

import csv
import json
import os
import pathlib
import sys
import urllib.error
import urllib.request

PROJECT = os.environ.get("SUPABASE_PROJECT_REF", "fuqrvmprgzqjmfqszxoe")
API = f"https://api.supabase.com/v1/projects/{PROJECT}/database/query"
ROOT = pathlib.Path(__file__).resolve().parent.parent
SCHEMA = ROOT / "supabase" / "schema.sql"
KEYS = pathlib.Path.home() / ".tauri" / "lane-keys"
BATCH = 100
DRY = "--dry-run" in sys.argv


def run(sql: str, label: str) -> None:
    """One statement batch, through the management API."""
    if DRY:
        print(f"  would send {label}: {len(sql):,} characters")
        return
    token = os.environ.get("SUPABASE_ACCESS_TOKEN", "")
    if not token:
        sys.exit("SUPABASE_ACCESS_TOKEN is not set")
    req = urllib.request.Request(
        API,
        data=json.dumps({"query": sql}).encode(),
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=120) as res:
            body = res.read().decode()
        print(f"  {label}: ok")
        if body.strip() not in ("[]", ""):
            print(f"    {body[:300]}")
    except urllib.error.HTTPError as e:
        detail = e.read().decode()[:600]
        sys.exit(f"  {label}: failed ({e.code})\n    {detail}")


def quote(s: str) -> str:
    return "'" + s.replace("'", "''") + "'"


def main() -> None:
    if not SCHEMA.exists():
        sys.exit(f"no schema at {SCHEMA}")
    print(f"project {PROJECT}")
    print("schema:")
    run(SCHEMA.read_text(), "tables, policies and functions")

    rows: list[tuple[str, str]] = []
    for f in sorted(KEYS.glob("*.csv")):
        with f.open() as fh:
            for row in csv.DictReader(fh):
                rows.append((row["plan"], row["licence_key"]))
    if not rows:
        print(f"no keys found in {KEYS}; mint some with scripts/licence.mjs")
        return

    print(f"keys: {len(rows):,} from {KEYS}")
    for i in range(0, len(rows), BATCH):
        chunk = rows[i : i + BATCH]
        values = ",".join(f"({quote(p)},{quote(k)})" for p, k in chunk)
        sql = (
            "insert into public.licence_keys (plan, licence_key) values "
            f"{values} on conflict (licence_key) do nothing;"
        )
        run(sql, f"keys {i + 1}–{i + len(chunk)}")

    print("checking:")
    run(
        "select plan, count(*) filter (where claimed_at is null) as unclaimed "
        "from public.licence_keys group by plan order by plan;",
        "pool",
    )


if __name__ == "__main__":
    main()
