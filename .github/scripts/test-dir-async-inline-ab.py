"""Measure narrow copies and shared sources on the same runner."""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(sys.argv[1]).resolve()
PAIRS = int(sys.argv[2]) if len(sys.argv) > 2 else 32
TEST = "test_cases::moon_test::test_async_test_inline"
PROMOTE = "test_cases::snapshot_testing::test_snapshot_test_target_js"
CASES = Path("crates/moon/tests/test_cases")
INPUTS = ["moon_test/async_test_inline", "moon_test/dummy_async_impl", "snapshot_testing.in"]
ROOT.mkdir(parents=True, exist_ok=True)


def inventory():
    entries = {}
    for name in INPUTS:
        folder = CASES / name
        for path in (folder, *sorted(folder.rglob("*"))):
            entries[path.relative_to(CASES).as_posix()] = [
                path.stat().st_mtime_ns,
                hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None,
            ]
    return entries


def invoke(directory, test, scope):
    directory.mkdir()
    env = dict(os.environ, MOON_TEST_TRACE_DIR=str(directory), MOON_TEST_KEEP_DIR="",
               MOON_ASYNC_INLINE_SCOPE=scope)
    start = time.perf_counter()
    proc = subprocess.run(
        ["cargo", "test", "--locked", "-p", "moon", "--test", "mod", test,
         "--", "--exact", "--test-threads=1", "--show-output"],
        env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    cargo_ms = (time.perf_counter() - start) * 1000
    (directory / "test.log").write_bytes(proc.stdout)
    output = proc.stdout.decode("utf-8", errors="replace")
    match = re.search(r"test result: ok\. 1 passed; 0 failed;.*finished in ([\d.]+)s", output)
    if proc.returncode or not match or f"test {test} ... ok" not in output:
        raise RuntimeError(f"{directory.name}: {output}")
    trace, = directory.glob("*.jsonl")
    records = [json.loads(line) for line in trace.read_text().splitlines()]
    assert all(record["test"] == test for record in records)
    events = [record["event"] for record in records]
    assert [(e["phase"], e["state"]) for e in events] == [
        ("create", "started"), ("create", "ok"), ("copy", "started"), ("copy", "ok"),
        ("active", "started"), ("active", "finished"), ("cleanup", "started"), ("cleanup", "ok"),
    ]
    assert not events[5]["unwinding"]
    row = dict(harness_ms=float(match[1]) * 1000, cargo_ms=cargo_ms,
               workspace=records[-1]["workspace"], **events[3]["copied"])
    row.update({e["phase"] + "_ms": e["elapsed_us"] / 1000 for e in events if "elapsed_us" in e})
    if test == TEST:
        assert events[2]["paths"] == (["async_test_inline", "dummy_async_impl"] if scope == "copy" else [])
        assert row["files"] == (7 if scope == "copy" else 0)
        row["body_ms"] = int(re.search(r"async-inline-body-us (\d+)", output)[1]) / 1000
    else:
        assert events[2]["paths"] == ["."] and row["files"] == 8
    return row


baseline = inventory()
(ROOT / "sources-before.json").write_text(json.dumps(baseline, indent=2))
samples = []
for pair in range(PAIRS + 1):
    order = ("copy", "shared") if pair == 0 or pair % 2 else ("shared", "copy")
    for position, scope in enumerate(order, 1):
        row = invoke(ROOT / f"{pair:02d}-{scope}", TEST, scope)
        row.update(pair=pair, position=position, scope=scope)
        samples.append(row)
        print(json.dumps(row), flush=True)
        (ROOT / "samples.json").write_text(json.dumps(samples, indent=2))
    assert inventory() == baseline, f"source tree changed during pair {pair}"

# Exercise simultaneous readers on the same Windows runner, outside timed pairs.
with ThreadPoolExecutor(max_workers=2) as pool:
    parallel = list(pool.map(lambda n: invoke(ROOT / f"parallel-{n}", TEST, "shared"), (1, 2)))
assert parallel[0]["workspace"] != parallel[1]["workspace"]
assert inventory() == baseline, "source tree changed during concurrent test"

# This existing test checks the rewritten source and all three promoted files in its private copy.
promotion = invoke(ROOT / "private-promotion", PROMOTE, "shared")
after = inventory()
(ROOT / "sources-after.json").write_text(json.dumps(after, indent=2))
assert after == baseline, "promotion touched checked-in fixtures"
(ROOT / "controls.json").write_text(json.dumps(dict(parallel=parallel, promotion=promotion), indent=2))
