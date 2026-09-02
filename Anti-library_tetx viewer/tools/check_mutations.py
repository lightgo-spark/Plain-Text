"""Is the source clean, or is a mutation still sitting in it?

`mutate.py` restores every file it touches in a `finally`, which covers an
exception and does not cover the process being killed. When that happened the
tree was left with a defect put back on purpose — the hyphenation length guard,
in the run that prompted this — and everything built on top of it for hours
afterwards, tests included.

    python tools/check_mutations.py

Exits non-zero, and names the file, if any mutation's replacement is in the
source where its original is not.
"""
import io
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))

import mutate  # noqa: E402


def main():
    left = []
    for name, path, find, replace, _test in mutate.MUTATIONS:
        try:
            with io.open(os.path.join(ROOT, path), encoding="utf-8") as f:
                source = f.read()
        except OSError as e:
            left.append((name, path, "cannot be read: %s" % e))
            continue
        # The telling state is the replacement present and the original gone.
        # A mutation whose replacement merely resembles ordinary code is not
        # enough on its own.
        if replace in source and find not in source:
            left.append((name, path, "the mutated form is in the source"))

    if not left:
        print("Clean: %d mutations, none of them left in the tree." % len(mutate.MUTATIONS))
        return 0

    print("A mutation was left behind. The tree is not what it looks like.\n")
    for name, path, why in left:
        print("  %s\n      %s — %s" % (name, path, why))
    print("\nRestore those before trusting a green run.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
