"""Collect the full licence texts the binaries are obliged to carry.

`NOTICES.md` says which licences apply and to which crates. That is not what
the licences ask for. MIT wants its copyright notice *and its permission
notice* in every copy; Apache-2.0 wants a copy of the licence and the contents
of any NOTICE file; BSD wants the copyright notice, the conditions and the
disclaimer reproduced in the distribution; OFL and the Ubuntu Font Licence want
their text alongside the fonts they cover — and this program embeds those fonts.

NOTICES.md used to close by saying the full texts were "in each crate's source
package under ~/.cargo/registry/src/". That is true on the machine that built
it and nowhere else. Somebody who downloads the zip has no registry, so the
texts have to travel with the binaries.

    python tools/licenses.py           # write THIRD-PARTY-LICENSES.md
    python tools/licenses.py --check   # fail if it is out of date

Needs `cargo install cargo-license`.
"""

import hashlib
import io
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "THIRD-PARTY-LICENSES.md")

# A phrase that appears in a licence's text and in no other, used both to fill
# gaps below and by `tools/check_licence_cover.py` to verify the result. Kept
# distinctive: matching on a licence's *name* would find the SPDX identifier in
# the missing-file list and prove nothing.
MARKERS = {
    "MIT": "Permission is hereby granted, free of charge",
    "Apache-2.0": "Licensed under the Apache License, Version 2.0",
    "Apache-2.0 WITH LLVM-exception": "LLVM Exception",
    "BSD-2-Clause": "Redistribution and use in source and binary forms",
    "BSD-3-Clause": "Neither the name of",
    "ISC": "Permission to use, copy, modify, and/or distribute this software",
    "0BSD": "Permission to use, copy, modify, and/or distribute this software",
    "Zlib": "This software is provided 'as-is', without any express",
    "MPL-2.0": "Mozilla Public License",
    "CC0-1.0": "CC0 1.0 Universal",
    "Unlicense": "This is free and unencumbered software released into the public domain",
    "Unicode-3.0": "UNICODE LICENSE",
    "BSL-1.0": "Boost Software License",
    "OFL-1.1": "SIL Open Font License",
    "Ubuntu-font-1.0": "UBUNTU FONT LICENCE",
    "LGPL-2.1-or-later": "GNU LESSER GENERAL PUBLIC LICENSE",
    "GPL-2.0": "GNU GENERAL PUBLIC LICENSE",
}

# What a crate calls the file. Matched anywhere in the name, not only at the
# start: `epaint_default_fonts` ships `OFL.txt` and `UFL.txt` — the licences of
# the two typefaces this program embeds — and an earlier version of this script
# looked for names *beginning* with LICENSE and found neither.
MARKERS_IN_NAME = (
    "LICEN",      # LICENSE, LICENCE, and anything-license.txt
    "COPYING",
    "COPYRIGHT",
    "NOTICE",
    "UNLICENSE",
    "OFL",        # SIL Open Font Licence
    "UFL",        # Ubuntu Font Licence
)

# Fonts are the case where the licence sits beside the file it covers with a
# name of its own (`Hack-Regular.txt` next to `Hack-Regular.ttf`). Any .txt in
# a directory holding typefaces is taken as the terms for them.
FONT_SUFFIXES = (".ttf", ".otf", ".ttc", ".woff", ".woff2")

# How deep inside a crate to look. Deep enough for `fonts/`, shallow enough not
# to drag in the licence files of a crate's own test fixtures.
MAX_DEPTH = 2

BACKTICKS = chr(96) * 3
QUOTES = chr(39) * 3
FENCE = BACKTICKS + "\n%s\n" + BACKTICKS + "\n\n"

HEADER = """# Third-party licence texts

Anti-library is distributed under the MIT licence (see `LICENSE`). It is built
from the crates listed in `NOTICES.md`, and several of those licences require
their full text and copyright notices to travel with any copy of the program —
not merely a statement of which licence applies.

This file is that text, gathered by `tools/licenses.py` from the crate sources
in the local registry, so it cannot drift from what was actually built. It
ships in the zip and in the installer.

Identical texts are given once and the crates sharing them are listed together;
a licence differs from another chiefly in whose copyright line it carries, so
most entries below are distinct.

"""


def crates():
    proc = subprocess.run(
        ["cargo", "license", "--json"], cwd=ROOT, capture_output=True, text=True
    )
    if proc.returncode != 0:
        print("cargo license failed — is cargo-license installed?")
        print(proc.stderr.strip()[:2000])
        sys.exit(2)
    out = []
    for c in json.loads(proc.stdout):
        if c.get("name") == "anti-library":
            continue
        out.append((c["name"], c.get("version", ""), c.get("license") or "(unstated)"))
    return sorted(set(out))


def registry_roots():
    home = os.path.expanduser("~")
    base = os.path.join(home, ".cargo", "registry", "src")
    if not os.path.isdir(base):
        return []
    return [os.path.join(base, d) for d in os.listdir(base)]


def looks_like_licence(directory, entry, filenames):
    """Is this file the terms for something in the crate?"""
    upper = entry.upper()
    if any(m in upper for m in MARKERS_IN_NAME):
        return True
    # A .txt sitting beside typefaces is their licence.
    if upper.endswith(".TXT") and any(
        f.lower().endswith(FONT_SUFFIXES) for f in filenames
    ):
        return True
    return False


def texts_for(name, version, roots):
    """Every licence-ish file in the crate's unpacked source, as (file, text)."""
    found = []
    for root in roots:
        base = os.path.join(root, "%s-%s" % (name, version))
        if not os.path.isdir(base):
            continue
        for here, dirs, filenames in os.walk(base):
            depth = here[len(base):].count(os.sep)
            if depth >= MAX_DEPTH:
                dirs[:] = []
            # Never a crate's vendored dependencies or version control.
            dirs[:] = [d for d in dirs if d not in (".git", "target", "vendor")]
            for entry in sorted(filenames):
                if not looks_like_licence(here, entry, filenames):
                    continue
                path = os.path.join(here, entry)
                if not os.path.isfile(path) or os.path.getsize(path) > 200 * 1024:
                    continue
                try:
                    body = io.open(path, encoding="utf-8", errors="replace").read().strip()
                except OSError:
                    continue
                # Enough to be terms rather than a stub or a path listing.
                if len(body) < 40:
                    continue
                rel = os.path.relpath(path, base).replace("\\", "/")
                found.append((rel, body))
        if found:
            break
    return found


def spdx_parts(expr):
    """The identifiers in an SPDX expression, flattened.

    `OR` is a choice and any one alternative satisfies it; `AND` needs all of
    them. Flattening both is deliberately generous — carrying the text of a
    licence that was not chosen costs a page and misses nothing.
    """
    parts = re.split(r"\s+(?:OR|AND)\s+|[()]", expr)
    return [p.strip() for p in parts if p.strip()]


def find_licence_text(ident, roots):
    """A copy of this licence's text from anywhere in the local registry.

    Crate roots only, one level deep: that is where a licence file lives, and
    walking every crate in the registry to its leaves takes minutes.
    """
    marker = MARKERS.get(ident)
    if not marker:
        return None
    for root in roots:
        try:
            entries = sorted(os.listdir(root))
        except OSError:
            continue
        for crate in entries:
            d = os.path.join(root, crate)
            if not os.path.isdir(d):
                continue
            try:
                files = os.listdir(d)
            except OSError:
                continue
            for entry in sorted(files):
                if not any(m in entry.upper() for m in MARKERS_IN_NAME):
                    continue
                path = os.path.join(d, entry)
                if not os.path.isfile(path) or os.path.getsize(path) > 200 * 1024:
                    continue
                try:
                    body = io.open(path, encoding="utf-8", errors="replace").read().strip()
                except OSError:
                    continue
                if marker in body:
                    return "%s (%s)" % (crate, entry), body
    return None


def build():
    roots = registry_roots()
    all_crates = crates()

    # text -> {"crates": set, "file": name}
    by_text = {}
    missing = []
    for name, version, lic in all_crates:
        found = texts_for(name, version, roots)
        if not found:
            missing.append((name, version, lic))
            continue
        for filename, body in found:
            key = hashlib.sha256(body.encode("utf-8")).hexdigest()
            slot = by_text.setdefault(key, {"crates": set(), "file": filename, "body": body})
            slot["crates"].add("%s %s" % (name, version))

    lines = [HEADER]
    covered = set()
    for slot in by_text.values():
        covered |= {c.split(" ")[0] for c in slot["crates"]}
    lines.append(
        "%d crates in the build; %d have licence text in their source package and "
        "are reproduced below, under %d distinct texts.\n\n"
        % (len(all_crates), len(covered), len(by_text))
    )

    if missing:
        lines.append(
            "The following ship no licence file of their own. The terms that apply "
            "to them are the SPDX identifiers recorded here and in `NOTICES.md`, "
            "and the text of each of those licences appears elsewhere in this "
            "file.\n\n"
        )
        for name, version, lic in sorted(missing):
            lines.append("- `%s %s` — %s\n" % (name, version, lic))
        lines.append("\n")

    # Anything the dependencies did not supply a copy of. Without this the
    # sentence above ("the text ... appears elsewhere in this file") is a claim
    # nobody checked, and for CC0-1.0 it was simply untrue.
    have = "".join(slot["body"] for slot in by_text.values())
    wanted = []
    for _name, _version, lic in all_crates:
        for ident in spdx_parts(lic):
            marker = MARKERS.get(ident)
            if marker and marker not in have and ident not in wanted:
                wanted.append(ident)
    filled = []
    for ident in wanted:
        got = find_licence_text(ident, roots)
        if got:
            filled.append((ident, got[0], got[1]))
        else:
            print("WARNING: no text found anywhere for %s" % ident)

    if filled:
        lines.append("## Licences with no copy among the dependencies\n\n")
        lines.append(
            "These apply to crates that ship no licence file. No dependency in "
            "this build supplied a copy either, so the text is reproduced here "
            "from another package in the local registry.\n\n"
        )
        for ident, source, body in filled:
            lines.append("### %s\n\n*(text taken from `%s`)*\n\n" % (ident, source))
            lines.append(FENCE % body.replace(BACKTICKS, QUOTES))

    lines.append("---\n\n")
    ordered = sorted(
        by_text.values(), key=lambda s: (-len(s["crates"]), sorted(s["crates"])[0])
    )
    for slot in ordered:
        names = sorted(slot["crates"])
        lines.append("## %s\n\n" % ", ".join("`%s`" % n for n in names))
        lines.append("*(%s)*\n\n" % slot["file"])
        lines.append(FENCE % slot["body"].replace(BACKTICKS, QUOTES))

    return "".join(lines), len(all_crates), len(by_text), len(missing)


def main():
    text, n_crates, n_texts, n_missing = build()
    if "--check" in sys.argv:
        current = io.open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else ""
        if current != text:
            print("THIRD-PARTY-LICENSES.md is out of date — run: python tools/licenses.py")
            sys.exit(1)
        print(
            "THIRD-PARTY-LICENSES.md matches: %d crates, %d texts, %d without a file."
            % (n_crates, n_texts, n_missing)
        )
        return
    io.open(OUT, "w", encoding="utf-8", newline="").write(text)
    print(
        "THIRD-PARTY-LICENSES.md written: %d crates, %d distinct texts, %d without a file."
        % (n_crates, n_texts, n_missing)
    )


if __name__ == "__main__":
    main()
