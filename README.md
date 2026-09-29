# systemeame

`systemeame` is a Salesforce CLI plugin for applying an explicit Apex
system-execution policy while preserving code that already states a user or
system mode.

## Install

Install it through Salesforce CLI—the native binary for Linux, macOS, or Windows is selected automatically. No Rust toolchain, compiler, or separate
binary download is needed.

```sh
sf plugins install systemeame
```

Then run it in a Salesforce project:

```sh
sf apex system-mode fix --all --dry-run
```

## Behavior

- The engine is Rust; TypeScript is only the Salesforce CLI boundary.
- Default scope is staged `.cls` and `.trigger` files. The plugin never stages,
  commits, pushes, or runs hooks.
- `--all` is explicit project-wide scope across `packageDirectories`.
- Rewrites are AST/byte-offset based; comments and strings are not treated as
  operations.
- Files with parser errors remain unchanged with diagnostics.
- The final summary reports total processing time in seconds; JSON output includes
  `durationSeconds`. `--quiet` suppresses the human summary.
- Dynamic query text will never be rewritten to add `WITH SYSTEM_MODE`.

## Development

Requirements: Rust `1.98.0`, Node.js 20.17+, npm, Git, and Salesforce CLI for
installation smoke tests.

```sh
npm ci
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm run build
npm run lint
npm test
npm run test:integration
```

The project intentionally has no `postinstall` compilation or executable
download. Releases use precompiled optional native packages for Linux (GNU and
musl; x64 and ARM64), macOS (Intel and Apple Silicon), and Windows (x64 and ARM64).

## Local development

The engine rewrites native DML (`insert`, `update`, `upsert`, `delete`,
`undelete`, and `merge`) without an explicit mode, adds `WITH SYSTEM_MODE` to
static bracketed SOQL queries, and appends
`System.AccessLevel.SYSTEM_MODE` to one-argument `Database.query(...)` calls.
It preserves calls with an existing access-level argument and query arguments
containing `WITH USER_MODE` or `WITH SYSTEM_MODE`. Static SOQL preserves explicit
modes, adds the clause only to the outer query,
and places it before `GROUP BY`, `ORDER BY`, and `LIMIT`. Queries with a different
existing `WITH` clause remain unchanged with an `UNSUPPORTED_SOQL_WITH` diagnostic.
SOSL and other `Database.*` rewrites are not implemented.

```sh
npm ci
npm run build:native
sf plugins link .
cd /path/to/a/salesforce-project
sf apex system-mode fix --all --dry-run
```

`npm run build:native` automatically selects the host platform and stages the
native executable (`systemeame-native.exe` on Windows). Windows builds require
the Visual Studio C++ build tools for the MSVC Rust toolchain.

`npm run pack:local` also creates a host-native tarball pair for packaging
inspection. The supported local test route is `sf plugins link .`. Use
`--dry-run` first; the default scope only considers staged Apex files and never
changes the Git index.

## Security and compatibility

Adding system mode changes execution policy; it is not cosmetic formatting. The
tool will not alter sharing declarations, create credentials, or contact a
Salesforce org. This is an early release: changes have not been compilation-tested
in a Salesforce org. Review the diff and run your Apex tests before deployment.
SOSL rewriting, API-version checks, and symbol resolution for shadowed
`Database` names are not implemented. Some valid Apex may be rejected by the parser.
Project-wide scope does not
apply ignore-file rules; writes are not atomic.

## Publishing

Every push to `main` automatically reserves the next patch version, runs tests,
builds the eight Linux/macOS/Windows native packages, publishes them to npm, publishes the
plugin last, checks installation with Salesforce CLI, and creates a GitHub Release.
No manual version bump, tag, or environment approval is required.

The workflow creates a tagged release commit containing synchronized npm and Rust
versions. It leaves `main` untouched, so the release bot cannot create a publish
loop. The `v*` tags are immutable; a retry for the same source commit reuses its
reserved version. Failed releases may leave gaps in version numbers. Pushes share
one release concurrency group with a queue of up to 100 pending runs, so releases
execute one at a time without replacing earlier pending pushes.

A higher version committed in `package.json` establishes a new minimum (for
example, `0.2.0` for a minor release). Otherwise the next patch is automatic.
Manual workflow runs on `main` retry that source commit's release.

npm Trusted Publishing is configured per package for owner `CarlosIribar`,
repository `systemeame`, workflow `release.yml`, environment `npm-release`.
The environment permits only `main`. No npm token is stored in GitHub.
A newly introduced package requires a one-time owner-authenticated initial
publication before its Trusted Publisher can be configured. Existing versions
are skipped only when their registry integrity matches the build artifact.
