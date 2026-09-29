# Contributing

Keep transformations conservative. A new rewrite needs a parser fixture, golden
input/output, idempotence coverage, negative cases, compatibility evidence, and
a changelog entry. Do not copy customer Apex into
fixtures.

Run the documented Rust and Node checks before submitting changes. Do not publish
packages, change release tags, or add credentials to this repository.
