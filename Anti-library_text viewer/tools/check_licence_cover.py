"""Is the claim in THIRD-PARTY-LICENSES.md true?

Sixty of the crates ship no licence file of their own, and the file says of
them: "the text of each of those licences appears elsewhere in this file."
That is a claim about the document, and nothing checked it. A crate offered
under a licence whose text is nowhere in the collection is a crate shipped
without the notice it requires — and the sentence asserting otherwise makes it
worse, because it reads as though somebody looked.

    python tools/check_licence_cover.py

Exits non-zero and names the licence if any SPDX identifier in the list has no
text behind it.
"""

import io
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOC = os.path.join(ROOT, "THIRD-PARTY-LICENSES.md")

# The table lives with the generator, so a licence added there is one this
# check knows about too. Two copies of it would drift, and a drifted copy would
# pass this check while leaving a licence unshipped.
sys.path.insert(0, os.path.join(ROOT, "tools"))
from licenses import MARKERS  # noqa: E402

# An expression like "Apache-2.0 OR MIT" is satisfied by any one of its
# alternatives — that is what OR means, and the choice is recorded in
# NOTICES.md. An AND needs all of them.
from licenses import spdx_parts as alternatives  # noqa: E402


def main():
    if not os.path.exists(DOC):
        print("THIRD-PARTY-LICENSES.md is not there — run tools/licenses.py")
        return 1
    text = io.open(DOC, encoding="utf-8").read()

    # The bullet list of crates that carry no licence file.
    listed = re.findall(r"^- `([^`]+)` — (.+)$", text, re.M)
    if not listed:
        print("no missing-file list in the document; nothing to check")
        return 0

    unknown, uncovered = set(), {}
    for crate, expr in listed:
        # `OR` means any one of them suffices.
        ok = False
        seen_any_known = False
        for ident in alternatives(expr):
            marker = MARKERS.get(ident)
            if marker is None:
                unknown.add(ident)
                continue
            seen_any_known = True
            if marker in text:
                ok = True
                break
        if not ok and seen_any_known:
            uncovered.setdefault(expr, []).append(crate)

    print("Crates without a licence file: %d" % len(listed))
    print("Licence expressions among them: %d" % len({e for _, e in listed}))

    if unknown:
        print("\nSPDX identifiers this check does not know:")
        for u in sorted(unknown):
            print("  %s" % u)
        print("Add a marker for each to MARKERS in tools/licenses.py.")

    if uncovered:
        print("\nNo text in the document for:")
        for expr, crates in sorted(uncovered.items()):
            print("  %s — %d crate(s), e.g. %s" % (expr, len(crates), crates[0]))
        return 1

    if unknown:
        return 1

    print("\nEvery one of them has its licence text in the document.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
