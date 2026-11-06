"""Measure the existing async-inline test with both fixture scopes on one worker."""

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

TEST = "test_cases::moon_test::test_async_test_inline"
ROOT = Path(sys.argv[1]).resolve()
PAIRS = int(sys.argv[2]) if len(sys.argv) > 2 else 16
ROOT.mkdir(parents=True, exist_ok=True)
samples = []
for pair in range(PAIRS + 1):
    # Pair zero warms both paths. Counterbalance which variant runs first.
    order = ("full", "narrow") if pair == 0 or pair % 2 == 1 else ("narrow", "full")
    for position, scope in enumerate(order, 1):
        directory = ROOT / f"{pair:02d}-{scope}"
        directory.mkdir()
        env = dict(os.environ, MOON_TEST_TRACE_DIR=str(directory), MOON_TEST_KEEP_DIR="",
                   MOON_ASYNC_INLINE_SCOPE=scope)
        start = time.perf_counter()
        process = subprocess.run(
            ["cargo", "test", "--locked", "-p", "moon", "--test", "mod", TEST,
             "--", "--exact", "--test-threads=1"],
            env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        )
        cargo_ms = (time.perf_counter() - start) * 1000
        output = process.stdout.decode("utf-8", errors="replace")
        (directory / "test.log").write_bytes(process.stdout)
        match = re.search(r"test result: ok\. 1 passed; 0 failed;.*finished in ([\d.]+)s", output)
        if process.returncode or not match or f"test {TEST} ... ok" not in output:
            raise RuntimeError(f"pair {pair} {scope}: {output}")

        (trace,) = directory.glob("*.jsonl")
        records = [json.loads(line) for line in trace.read_text().splitlines()]
        assert all(record["test"] == TEST for record in records)
        events = [record["event"] for record in records]
        expected = [("create", "started"), ("create", "ok"), ("copy", "started"),
                    ("copy", "ok"), ("active", "started"), ("active", "finished"),
                    ("cleanup", "started"), ("cleanup", "ok")]
        assert [(event["phase"], event["state"]) for event in events] == expected
        paths = ["."] if scope == "full" else ["async_test_inline", "dummy_async_impl"]
        assert events[2]["paths"] == paths and not events[5]["unwinding"]
        assert events[3]["copied"]["files"] == (148 if scope == "full" else 7)

        row = dict(pair=pair, position=position, scope=scope, test_ms=float(match[1])*1000,
                   cargo_ms=cargo_ms, **events[3]["copied"])
        row.update({event["phase"] + "_ms": event["elapsed_us"]/1000
                    for event in events if "elapsed_us" in event})
        print(json.dumps(row), flush=True)
        samples.append(row)
        (ROOT / "samples.json").write_text(json.dumps(samples, indent=2))
