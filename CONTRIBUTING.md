# Contributing

## Build and test

```bash
cargo build --release
cargo test
```

CI runs the same steps against a pinned toolchain and fails on any warning:

```bash
rustup toolchain install 1.98.0 --profile minimal --component clippy
cargo build --release --locked
cargo test --locked
cargo clippy --all-targets -- -D warnings
```

Run `cargo clippy --all-targets -- -D warnings` locally before opening a PR.
`Cargo.lock` is committed and CI builds with `--locked`; add dependencies
with `cargo add` rather than hand-editing the lockfile.

## Where tests live

There is no `tests/` directory — `src/hash.rs` (the dHash implementation and
resize/brightness invariance) and `src/group.rs` (complete-linkage grouping,
the ordering of keep vs. extra) each carry their own `#[cfg(test)] mod tests`
block. Put a new test next to the code it exercises.

## Adding a new hashing or grouping rule

A change to how images are hashed or grouped starts with a failing test in
`src/hash.rs` or `src/group.rs` that encodes the exact case — a specific hash
pair that should or should not end up in the same group, for instance —
before the implementation changes. The chaining bug described in the README
(union-find collapsing unrelated images into one 912-file group) is the
reason grouping has dedicated tests for a "ladder" of hashes designed to
chain; a new grouping rule should be checked against that kind of adversarial
case, not just the easy one.

If a change touches the hot path (hashing or the O(n²) comparison), run the
benchmark before and after:

```bash
python bench/generate.py 5000
cargo build --release
./target/release/imgdupe bench/images --threshold 5
```

A correct run reports exactly 500 groups of 3 on the generated set — a
grouping bug shows up as the wrong group count, which makes the benchmark a
correctness check as well as a timing one.

## Commit style

Match the existing log (`git log --oneline`): `Area: what changed`, lower
case after the colon, imperative, no trailing period, no conventional-commit
prefixes. Examples from this repository:

```
Ship the benchmark behind the hashing figures, and run the tests in CI
README: publish the measured benchmark, and label which numbers came from real photos
Release workflow: build a binary for Linux, macOS and Windows on a tag
```

## Pull requests

If a change affects grouping behavior, the measured numbers in the README
benchmark table may need re-measuring — say in the PR whether you did. Small,
focused PRs; describe what dataset you tested against.
