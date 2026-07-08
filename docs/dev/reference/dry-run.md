# Behavior of `moon ... --dry-run`

`moon ... --dry-run` prints the commands that would perform the requested work.
For build-like commands, that means rebuilding requested artifacts. For `moon
fmt`, that means applying the planned formatting or manifest migration
operations. Treat the output as a human-readable command listing, not as a
stable machine interface: exact quoting, masking, and ordering details may still
change.

- **Command listing**. Dry-run prints planned actions instead of executing them.
  The output should stay close to shell commands that a developer can copy to
  diagnose or reproduce the operation.
- **Deterministic order**. Commands are printed in dependency order. For
  build-like commands, executing them sequentially produces the expected build
  artifacts; for `moon fmt`, executing them sequentially applies the planned
  formatting or migration operations.
- **Independent command tie-break**. When several build commands are ready at
  the same time, dry-run follows the dependency-first action-plan order. This is
  a stable planning order, not a scheduler trace.
- **Unix-style command lines**. Every command line is rewritten using Unix shell quoting, even on Windows hosts.
- **Backslash normalize**. Backslashes `\` in the commandline is normalized to forward slash `/`.
- **Home directory masking**. Any occurrence of the Moon home directory (`~/.moon` or a custom `$MOON_HOME`) is rewritten to the literal `$MOON_HOME`.
- **Project-relative paths**. Paths that live under the project root are emitted as a relative path from the project root, instead of absolute paths.
- **Target directory masking**. Paths under an out-of-project `--target-dir` are rewritten to `$MOON_TARGET_DIR`.
- **Toolchain binary aliases**. Known Moon toolchain executables (e.g. `~/.moon/bin/moonc`) are shortened to their bare names (`moonc`). Other executables keep their original paths.
- **Planning may still run**. Dry-run may perform the planning/configuration
  work required to discover the command plan. Today, module-level prebuild
  configuration is the known side-effecting planning exception because it can
  produce package link configuration before action planning. Lowered build
  actions, including package prebuild actions, are not executed by dry-run.
- **Verbose output is separate**. Extra diagnostics belong behind `--verbose` or
  debug flags. Dry-run itself should remain a rebuild command listing.
- **`moon run --dry-run` extras**. After the build commands, the dry-run output also prints the command that would execute the produced binary (typically `moonrun`, `node`, or the final executable).
- **`moon test --verbose` extras**. With `--verbose` set, `moon test` prints the command that is executed for each test case.

## Implementation Notes

Rupes Recta dry-run output is produced from typed command surfaces:

- Build-like commands use `build_lower::LoweredBuild`, emitted during
  action-plan lowering. `LoweredBuild` is captured only when a caller requests
  dry-run output, so normal build execution does not retain this additional
  command stream.
- `moon fmt` emits formatter operations into a semantic sink. The concrete
  builder creates `n2` nodes immediately and captures `fmt::FmtDryRun` only
  when a caller requests dry-run output.

Do not recover action dependencies, inputs, outputs, or command lines by
reparsing the `n2` executor graph when the information already exists in a typed
plan. New RR flows should expose a semantic command surface and should not add
new user-facing dry-run behavior by reparsing `n2`.
