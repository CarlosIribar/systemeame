# Changelog

## Unreleased

- Add explicit `System.AccessLevel.SYSTEM_MODE` to supported `Database` DML,
  dynamic-query, and query-locator overloads.

- Report total processing time in seconds in the command summary and JSON output.

- Build and package Windows x64 and ARM64 executables with Windows CI coverage.
- Select Windows native packages at runtime and support host-native local builds
  and packaging on Windows.

- Add explicit system mode to static SOQL, including multiline queries, binds,
  loops, and queries with ordering, limits, or subqueries.
- Preserve existing access modes and report incompatible WITH clauses for review.

## 0.1.1

- Initial Salesforce CLI plugin with a Rust parsing engine.
- Explicit system mode for native Apex DML and one-argument `Database.query` calls.
- Staged-file and project-wide scope, dry-run, check mode, and JSON output.
- Preserve explicit modes and reject partially staged files by default.
- Precompiled native packages for Linux and macOS on x64 and ARM64.
- Automatic patch releases from main; Windows support is deferred.

This early release has not been compilation-tested in a Salesforce org. See the
README for supported transformations and known limitations.
