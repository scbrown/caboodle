"""fixture-wheel is the caboodle python-wheel fixture stack member.

It keeps a list under $HOME/.fixture-wheel, so a hermetic HOME isolates it.
`add -` reads the item from stdin, which exercises a verify step's stdin.
"""

import os
import sys
from pathlib import Path


def main() -> int:
    args = sys.argv[1:]
    store = Path(os.environ["HOME"]) / ".fixture-wheel"
    if args[:1] in (["--help"], []):
        print(__doc__)
    elif args == ["--version"]:
        print("fixture-wheel 0.1.0")
    elif args == ["list"]:
        print(store.read_text() if store.exists() else "", end="")
    elif args[:1] == ["add"] and len(args) == 2:
        item = sys.stdin.read().strip() if args[1] == "-" else args[1]
        with store.open("a") as f:
            f.write(item + "\n")
    else:
        print(f"unknown arguments {args}", file=sys.stderr)
        return 2
    return 0
