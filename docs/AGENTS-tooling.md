# Tooling & Configuration Notes (Rust Safety Profile)

## Toolchain pinning
We pin the Rust toolchain via `rust-toolchain.toml` to make builds repeatable.

Recommended:
- fixed `channel` (exact version)
- `profile = "minimal"`
- include `clippy` + `rustfmt` components

## Clippy configuration
- Clippy is run with `-D warnings` in CI.
- Repository may include a `clippy.toml` to standardize thresholds and reduce noise.

## CI recommended commands
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`

Optional:
- `cargo audit`
- `cargo deny check`

## Recursion checks
Rust does not provide a built-in "forbid recursion" lint.
If recursion must be controlled, consider:
- documented coding rule + code review checklist
- optional custom CI script with heuristic checks
