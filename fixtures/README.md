# Fixture conventions

Fixtures are small source inputs whose paths describe why they exist:

- `rules/<rule>/invalid` contains the smallest inputs that must report.
- `rules/<rule>/valid` contains safe patterns and false-positive regressions.
- `parser/malformed` contains deterministic parser failures.
- `reporters` contains checked-in text and JSON reporter snapshots.
- `projects` contains multi-file layouts once a test needs project context.

Use forward-slash logical paths in expected diagnostics. The test harness
normalizes Windows and Unix separators before comparison.

## Adding a fixture

1. Add the smallest `.vue` file that demonstrates one behavior.
2. Put it under the matching rule, parser, reporter, or project directory.
3. Add or update the exact diagnostic snapshot: rule ID, severity, message,
   help, logical path, byte offset/length, line, and column.
4. Add both a positive case and the common safe pattern that must not report.
5. Run `just snapshots`, inspect the diff, then run `just roll-rust`.

Snapshot changes are reviewed evidence. Never replace expected output merely
because the implementation changed.
