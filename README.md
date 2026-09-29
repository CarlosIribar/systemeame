# systemeame

`systemeame` is a Salesforce CLI plugin for applying an explicit Apex
system-execution policy while preserving code that already states a user or
system mode.

## Install

Install it through Salesforce CLI—the native binary for Linux, macOS, or
Windows is selected automatically. No Rust toolchain, compiler, or separate
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
musl; x64 and ARM64), macOS (Intel and Apple Silicon), and Windows (x64 and
ARM64).

## Local development (Linux x64)

Version 0.1.0 rewrites native DML (`insert`, `update`, `upsert`, `delete`,
`undelete`, and `merge`) without an explicit mode, and appends
`System.AccessLevel.SYSTEM_MODE` to one-argument `Database.query(...)` calls.
It preserves calls with an existing access-level argument and query arguments
containing `WITH USER_MODE` or `WITH SYSTEM_MODE`. Other query and `Database.*`
rewrites are not implemented.

```sh
npm ci
npm run build:native
sf plugins link .
cd /path/to/a/salesforce-project
sf apex system-mode fix --all --dry-run
```

`npm run pack:local` also creates a host-native tarball pair for packaging
inspection. The supported local test route is `sf plugins link .`. Use
`--dry-run` first; the default scope only considers staged Apex files and never
changes the Git index.

## Security and compatibility

Adding system mode changes execution policy; it is not cosmetic formatting. The
tool will not alter sharing declarations, create credentials, or contact a
Salesforce org. This is an early release: changes have not been compilation-tested
in a Salesforce org. Review the diff and run your Apex tests before deployment.
Static SOQL/SOSL rewriting, API-version checks, and symbol resolution for shadowed
`Database` names are not implemented. Some valid Apex (including static queries
with `WITH USER_MODE`) may be rejected by the parser. Project-wide scope does not
apply ignore-file rules; writes are not atomic.

## Publishing

Pushing a version tag, such as `v0.1.0`, runs the release workflow. It builds
and validates all eight native packages, publishes them first through npm Trusted
Publishing, publishes the plugin last, smoke-tests `sf plugins install`, and
creates a GitHub Release. The `npm-release` environment requires owner approval.
Configure npm Trusted Publishing for the root package and all eight optional
native packages with owner `CarlosIribar`, repository `systemeame`, workflow
`release.yml`, and environment `npm-release`.

Manual workflow runs build and verify packages without publishing. For the first
release, push the version tag and leave the publish job awaiting environment
approval. Download its `release-packages` artifact and publish the eight native
packages before the root package using an authenticated npm session
(`npm publish <tarball> --access public --provenance=false`). Configure Trusted
Publishing, then approve the waiting job. It checks registry integrity and skips
identical packages already published; mismatching contents fail. Keep the original
workflow artifacts for retries. No npm token is stored in GitHub.
