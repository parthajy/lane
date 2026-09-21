#!/usr/bin/env python3
"""Tester codes for the label intake worker.

    python3 codes.py 25 > codes.txt          # 25 codes, one per line: CODE<TAB>name
    python3 codes.py 25 --kv <namespace-id>  # also prints the wrangler commands to load them

Codes look like LANE-7K2M-Q9XD. They identify a tester's uploads (so bad data
can be dropped later) and are the only credential the app ever holds.
"""
import secrets
import sys

ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"  # no 0/O/1/I


def code() -> str:
    part = lambda: "".join(secrets.choice(ALPHABET) for _ in range(4))
    return f"LANE-{part()}-{part()}"


def main() -> None:
    n = int(sys.argv[1]) if len(sys.argv) > 1 and sys.argv[1].isdigit() else 10
    kv = sys.argv[sys.argv.index("--kv") + 1] if "--kv" in sys.argv else None
    for i in range(1, n + 1):
        c = code()
        name = f"tester-{i:03d}"
        print(f"{c}\t{name}")
        if kv:
            print(f"npx wrangler kv key put --namespace-id {kv} '{c}' '{name}'", file=sys.stderr)


if __name__ == "__main__":
    main()
