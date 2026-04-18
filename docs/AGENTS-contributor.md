# Implementation Plan for Canary 0.1.0-50

## Phase 1: Freeze canary scope and output contract

- Define one canonical output envelope for stdout as **single-line JSON**.
- Treat template rendering as a follow-up feature behind a separate option; keep JSON as the handover source.
- Add acceptance criteria for deterministic output, stable field names, and explicit error shapes.

## Phase 2: Build config model and layering with strict precedence

- Implement typed config structs from your schema model.
- Support discovery order for rc files plus explicit path override.
- **Merge in this order:** schema/defaults → discovered rc → environment variables → CLI flags.
- Normalize optional arrays and derived fields during load to keep downstream logic total and predictable.

## Phase 3: Add schema-first preflight for both users and internal checks

- Validate the loaded config against the public JSON Schema before evaluation.
- Run semantic validation after schema validation (regex compilation, condition-specific constraints, derived subject, bounds).
- Expose a `validate-config` command that returns machine-readable errors and a human-friendly summary mode.

## Phase 4: Implement git context and CI history handling

- Resolve source branch, target branch, and default branch from flags/env/autodetect.
- Autodetect likely target branch (`main`/`master`) with explicit override support.
- Detect shallow clones in CI and perform bounded unshallow attempts using `history` config.
- Keep behavior deterministic: same inputs and repo state produce same lint result.

## Phase 5: Implement assertion evaluation engine

- Evaluate `skip_if` first, then `must_satisfy`.
- Support message, diff, meta, and threshold conditions with explicit error paths.
- Aggregate violations with configured exit behavior and severity semantics.
- Keep all evaluation paths panic-free and bounds-safe.

## Phase 6: Hardening, testing, and release gate

- Add unit tests for config merge precedence, schema failure, semantic failure, and skip logic.
- Add integration tests for branch detection and shallow-history CI paths.
- Gate release on `fmt`, `clippy --all-targets -- -D warnings`, and `test`.
- Tag canary only when JSON contract snapshots are stable.

---

## Recommended Third-Party Libraries

### Config Loading & Precedence

**Primary:** [`figment`](https://docs.rs/figment/)
- Closest Rust analog to Viper-style layered configuration
- Clean merging across defaults, files, and env
- Pair with `clap`-derived args for final CLI override layer

**Alternative:** [`config`](https://docs.rs/config/)
- Mature and flexible for multiple sources and formats
- Trade-off: less explicit ergonomics than figment for strict typed flows

### Serialization & Output

**`serde`** *(already in use)*
- Typed config and result model serialization

**`serde_json`**
- Canonical single-line JSON stdout output
- Snapshot-friendly serialization for testing

### Schema Validation

**`jsonschema`**
- Runtime validation against your published JSON Schema
- Good fit for `validate-config` and internal preflight parity

### Git Access & Repo Introspection

**`gix`**
- Pure Rust Git capabilities for deterministic internal operations
- Trade-off: steeper API learning curve

**Practical hybrid:** `std::process::Command` for unshallow/fetch + `gix` for read paths
- Simplest way to guarantee CI unshallow behavior while keeping internal reads structured

### Pattern Matching & Conditions

**`regex`**
- Compile-on-load and reuse at eval time
- Predictable performance and explicit error handling

### Error Handling

**`thiserror`** *(already in use)*
- Domain error enums and explicit propagation

### Testing Support

**`insta`** *(dev)*
- Snapshot testing for stable JSON output contracts

**`assert_cmd`** *(dev)*
- CLI integration testing of exit codes and stdout/stderr behavior

**`tempfile`** *(dev)*
- Isolated repo/config test fixtures without flaky state

---

## Initial Dependency Set

Add these to `Cargo.toml` to start Phase 1–3:

```toml
[dependencies]
serde_json = "1"
figment = { version = "0.13", features = ["toml"] }
jsonschema = "0.18"
regex = "1"

[dev-dependencies]
assert_cmd = "2"
insta = "1"
tempfile = "3"
```

---

## Next Steps

A concrete crate/module layout with files, structs, traits, and function signatures is ready to be provided when you're ready—matching this plan and your safety profile, without jumping to full implementation.