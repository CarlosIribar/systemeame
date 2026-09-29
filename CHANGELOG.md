# Changelog

## 0.1.0

- Initial Salesforce CLI plugin with a Rust parsing engine.
- Explicit system mode for native Apex DML and one-argument `Database.query` calls.
- Staged-file and project-wide scope, dry-run, check mode, and JSON output.
- Preserve explicit modes and reject partially staged files by default.
- Precompiled native packages for Linux, macOS, and Windows on x64 and ARM64.

This early release has not been compilation-tested in a Salesforce org. See the
README for supported transformations and known limitations.
