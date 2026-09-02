"""Run the checks over and over and see whether they say the same thing.

A suite that passes once has been observed once. What ships is the claim that
it passes *every* time, and the things that break that claim — a test that
depends on the clock, on the order a filesystem lists a directory, on a
timeout, on a leftover file from the run before — pass on the run you happen
to watch.

    python tools/stability.py --qc 1500 --qa 100

QC runs the compiled test binaries; QA runs the quality gate. Both are run one
at a time on purpose: the durability tests and the gate both use fixed paths
under the temp directory, so two at once would be testing the harness.

Every failure is kept with its output. The timing summary is there for the
other kind of instability — the run that passes but takes ten times as long,
which is a test waiting on something it should not be waiting on.
"""
import argparse
import io
import json
import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def percentile(values, p):
    if not values:
        return 0.0
    ordered = sorted(values)
    k = (len(ordered) - 1) * p
    lo = int(k)
    hi = min(lo + 1, len(ordered) - 1)
    return ordered[lo] + (ordered[hi] - ordered[lo]) * (k - lo)


def summarise(name, times):
    if not times:
        return "  %-6s no runs" % name
    return (
        "  %-6s n=%-5d min %.3fs  median %.3fs  p95 %.3fs  max %.3fs  total %.0fs"
        % (
            name,
            len(times),
            min(times),
            percentile(times, 0.5),
            percentile(times, 0.95),
            max(times),
            sum(times),
        )
    )


def test_binaries():
    """The compiled test binaries, straight from cargo.

    Asked for rather than guessed: the file names carry a hash that changes
    with every build, and a glob that matches nothing looks exactly like a
    suite with no tests in it.
    """
    r = subprocess.run(
        ["cargo", "test", "--release", "--no-run", "--message-format=json"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if r.returncode != 0:
        raise SystemExit("the test binaries would not build:\n" + r.stderr[-4000:])
    found = []
    for line in r.stdout.splitlines():
        try:
            msg = json.loads(line)
        except ValueError:
            continue
        if msg.get("profile", {}).get("test") and msg.get("executable"):
            found.append(msg["executable"])
    if not found:
        raise SystemExit("cargo reported no test binaries")
    return found


def run_once(cmd, cwd=ROOT):
    at = time.perf_counter()
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    return time.perf_counter() - at, r.returncode, (r.stdout + r.stderr)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--qc", type=int, default=1500, help="test-suite repetitions")
    ap.add_argument("--qa", type=int, default=100, help="quality-gate repetitions")
    ap.add_argument("--report", default="", help="write the summary here as well")
    args = ap.parse_args()

    print("Stability run: QC x%d, QA x%d" % (args.qc, args.qa))
    print("-" * 66)

    bins = test_binaries()
    print("test binaries: %d" % len(bins))
    for b in bins:
        print("  %s" % os.path.basename(b))

    gate = os.path.join(ROOT, "target", "release", "antilib-qa.exe")
    if not os.path.exists(gate):
        r = subprocess.run(
            ["cargo", "build", "--release", "--bin", "antilib-qa"],
            cwd=ROOT, capture_output=True, text=True,
        )
        if r.returncode != 0:
            raise SystemExit("the gate would not build:\n" + r.stderr[-4000:])
    print("gate: %s" % os.path.basename(gate))
    print("-" * 66)

    failures = []
    qc_times = []
    started = time.time()

    for i in range(args.qc):
        round_at = time.perf_counter()
        for b in bins:
            took, code, out = run_once([b, "--test-threads=1"])
            if code != 0:
                failures.append(
                    {
                        "kind": "qc",
                        "run": i + 1,
                        "binary": os.path.basename(b),
                        "seconds": round(took, 3),
                        "output": out[-4000:],
                    }
                )
        qc_times.append(time.perf_counter() - round_at)
        if (i + 1) % 100 == 0 or i + 1 == args.qc:
            print(
                "  QC %5d/%d   %d failure(s) so far   %.0fs elapsed"
                % (i + 1, args.qc, len(failures), time.time() - started)
            )

    qa_times = []
    qa_checks = set()
    for i in range(args.qa):
        took, code, out = run_once([gate])
        qa_times.append(took)
        # The gate prints how many checks it made. A number that moves between
        # runs is itself an instability, whatever the exit code says.
        for line in out.splitlines():
            if line.strip().startswith("total"):
                qa_checks.add(line.split()[-2] if len(line.split()) > 1 else line.strip())
        if code != 0:
            failures.append(
                {
                    "kind": "qa",
                    "run": i + 1,
                    "binary": "antilib-qa",
                    "seconds": round(took, 3),
                    "output": out[-4000:],
                }
            )
        if (i + 1) % 10 == 0 or i + 1 == args.qa:
            print(
                "  QA %5d/%d   %d failure(s) so far   %.0fs elapsed"
                % (i + 1, args.qa, len(failures), time.time() - started)
            )

    print("-" * 66)
    lines = [
        "QC rounds: %d (each running every test binary)" % args.qc,
        "QA rounds: %d" % args.qa,
        summarise("QC", qc_times),
        summarise("QA", qa_times),
        "check counts seen: %s" % (", ".join(sorted(qa_checks)) or "none parsed"),
        "failures: %d" % len(failures),
    ]
    for l in lines:
        print(l)

    if failures:
        print("\nFirst few failures:")
        for f in failures[:5]:
            print("  %s run %d (%s, %.3fs)" % (f["kind"], f["run"], f["binary"], f["seconds"]))
            for line in f["output"].splitlines():
                if "panicked" in line or "FAILED" in line or line.startswith("test result"):
                    print("      %s" % line.strip())

    if args.report:
        with io.open(args.report, "w", encoding="utf-8") as fh:
            json.dump(
                {
                    "qc_rounds": args.qc,
                    "qa_rounds": args.qa,
                    "qc_seconds": qc_times,
                    "qa_seconds": qa_times,
                    "check_counts": sorted(qa_checks),
                    "failures": failures,
                },
                fh,
                indent=2,
            )
        print("\nwrote %s" % args.report)

    return 1 if failures or len(qa_checks) > 1 else 0


if __name__ == "__main__":
    sys.exit(main())
