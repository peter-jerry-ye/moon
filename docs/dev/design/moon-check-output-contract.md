# `moon check` output contract

## Status and scope

This document defines the accepted human-output baseline and the target
contract for bounded `moon check --json`. It covers project and single-file
checks. Watch, dry-run, formatting composition, and a new JSON Lines protocol
are outside the first structured-output feature.

The existing `--output-json` option is a compatibility format. It is not the
new `--json` contract and will not be renamed.

## Output ownership

One bounded invocation eventually produces one logical Check Report, even
when workspace target preferences split execution across multiple backends.
The report owns the durable facts needed to understand the invocation:

- the aggregate outcome and exit status;
- normalized Moon and compiler diagnostics;
- the backend associated with an execution diagnostic;
- complete diagnostic counts and the selected diagnostic projection;
- truncation counts;
- durable Moon-authored messages;
- task and run facts needed by the final summary;
- a recoverable command failure when no successful report can be produced.

Dynamic progress is transient presentation, not a durable result. Tracing is
developer telemetry. Neither belongs in the Check Report.

Child-process passthrough is not generalized into this model. The first
structured feature is bounded `moon check`, which does not publish an
arbitrary child program's result.

## Accepted human baseline

The current human contract is intentionally preserved while the report seam
is extracted:

- stdout contains the final `Finished` or `Failed with` command result;
- compiler diagnostics are rendered on stderr;
- User Log messages and diagnostic-limit notices are written to stderr;
- `--quiet` suppresses the success summary and User Log, but not a semantic
  failure or its diagnostics;
- `--verbose` changes only the User Log level and does not change stdout;
- a multi-backend check currently writes one result summary per backend run;
- the process exit status expresses semantic success or failure;
- closing stdout must not replace an already determined build status with a
  broken-pipe failure.

These are characterization rules, not the desired final internal structure.
In particular, per-backend summaries will later be replaced by one aggregate
summary when Check Report aggregation is introduced.

## Existing `--output-json` compatibility

`--output-json` remains line-oriented compiler diagnostic output:

- stdout contains one compiler diagnostic object per line;
- it does not contain a complete command envelope or human failure summary;
- diagnostic-limit notices and Moon-authored failure context remain stderr;
- exit status remains authoritative;
- existing bytes and command coverage remain compatibility constraints.

The future `--json` implementation must not silently change this mode.

## Target `--json` publication contract

After the CLI has recognized a valid bounded `moon check --json` invocation
and established its command session:

1. stdout contains exactly one compact JSON document followed by one newline;
2. stderr contains zero bytes;
3. every durable user-visible message is represented in that document;
4. publication happens once, after the complete outcome is known;
5. exit status agrees with the document's outcome.

The conceptual envelope contains:

```json
{
  "status": "success",
  "diagnostics": [],
  "messages": [],
  "summary": {},
  "failure": null,
  "truncated": null
}
```

The exact field shapes are command-specific and will be finalized with the
first vertical JSON slice. The important contract is completeness: compiler
diagnostics alone are insufficient because warnings, selection messages,
truncation, summaries, and recoverable failures would otherwise be lost when
stderr is empty.

JSON contains no ANSI styling. Dynamic progress is disabled rather than
serialized as a stream of events. `--quiet` and `--verbose` do not remove
records or alter the JSON schema; deterministic machine output must not depend
on terminal verbosity.

Panics, aborts, out-of-memory termination, and failures before a valid JSON
command session exists cannot reliably satisfy the one-document guarantee.
They are process failures outside this command-result contract, not a reason
for recoverable command failures to leak onto stderr.

## Diagnostics and aggregation

Backend runs use canonical backend order:

1. `wasm`
2. `wasm-gc`
3. `js`
4. `native`
5. `llvm`

Diagnostics are normalized and deduplicated within a backend. Identical
diagnostics from different backends remain distinct because they describe
different executions. Stable ordering is based on backend, normalized path,
source span, severity, code, and message rather than executor completion
order.

`--diagnostic-limit N` applies once after aggregation and deduplication. Errors
are selected before warnings and informational diagnostics. Complete counts,
outcome, and `--deny-warn` behavior are computed before limiting. Truncation is
metadata, not another warning.

## Recoverable failures

Failures that can be handled by the command session are report data in JSON
mode, including invalid project or package manifests, selection/configuration
errors, dependency resolution failures, compiler failures, filesystem or lock
errors, and process spawn failures.

A recoverable failure therefore produces a JSON failure object, a nonzero exit
status, and empty stderr. If earlier backend runs completed, the command still
publishes only the final document; it never leaves an unframed partial JSON
prefix on stdout.

The current behavior for explicitly selected inputs that are skipped is
preserved until a separate selection-policy change. Such messages remain
successful semantic warnings today. In `--json` they must be represented in
`messages` rather than written to stderr.

## Flag compatibility

The first `--json` slice supports ordinary paths, package selection, target
selection, warning configuration, `--deny-warn`, and
`--diagnostic-limit`.

It rejects combinations that already claim a different result protocol:

- `--watch`;
- `--dry-run`;
- `--fmt`;
- `--no-render` and explicit human-rendering options;
- `--output-json`.

The first feature does not add `--jsonl`. A future JSON Lines mode needs an
independent framing contract, especially for watch cycles.

## Incremental implementation seam

Execution first returns facts; presentation later chooses a destination and
format. The initial seam is deliberately concrete:

- compiler human rendering accepts a caller-supplied `std::io::Write`;
- current human callers pass the same stream they use today;
- rendering failures are returned to the caller instead of being printed by a
  fallback path;
- structured publication consumes diagnostic facts and does not invoke the
  human renderer.

P1 and P2 do not introduce a project-specific renderer or output trait. A
`DiagnosticRenderer`, universal `Output`, or generic event protocol would
encode abstractions before two real adapters have demonstrated a shared need.
The standard writer boundary is sufficient to remove the hard-coded stream
without deciding the later Check Report API.

## Characterization coverage

`check_output_contract` locks the current stream and status behavior for:

- clean default, quiet, and verbose checks;
- compiler failure and global diagnostic limiting;
- legacy `--output-json` diagnostics;
- multi-backend execution.

The existing closed-pipe tests lock publication-failure behavior. These tests
must remain unchanged through the writer migration; later intentional contract
changes update them in the same pull request as the behavior change.
