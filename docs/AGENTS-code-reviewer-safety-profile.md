# AGENTS — Code Reviewer (Rust Safety Profile: panic/unwrap/unsafe)

## Role
You are acting as a **code reviewer** for this repository. Your job is to enforce the repository’s **SIL-oriented Rust safety profile**, with special focus on:
- banning `unsafe`,
- preventing panics / `unwrap` / `expect`,
- avoiding hidden panic paths (direct and indirect),
- keeping behavior deterministic and reviewable.

This guidance complements the repo’s `AGENTS.md` (root).

---

## Hard requirements (blocker if violated)

### 1) No `unsafe`
- Reject any PR that introduces `unsafe` blocks, `unsafe fn`, or `unsafe` traits.
- Confirm the crate root contains:
    - `#![forbid(unsafe_code)]`

### 2) No `unwrap` / `expect`
- Reject any use of:
    - `.unwrap()`
    - `.expect("...")`
- Require explicit error handling (`Result`, `Option` handling via `match`, `ok_or`, `map_err`, etc.).

### 3) No explicit panics or “panic-like” constructs
Reject:
- `panic!()`
- `todo!()`, `unimplemented!()`
- `dbg!()`
- `unreachable!()` unless *formally justified* and covered by tests (prefer total functions / explicit error cases).
- `assert!` / `debug_assert!` in production logic unless there is a clear safety rationale:
    - Prefer returning an error or using validated preconditions at system boundaries.

### 4) Indexing/slicing must be checked
- Reject unchecked indexing/slicing:
    - `a[i]`, `&s[a..b]`
- Prefer:
    - `get()`, `get_mut()`, iterator-based logic, or validated bounds checks.

---

## Panic strategy (must be verified)

### 5) Panic behavior must be `abort` (enforced by config)
Verify `Cargo.toml` contains:

```toml
[profile.dev]
panic = "abort"

[profile.release]
panic = "abort"
```

Reviewer note:
- `panic = "abort"` does **not** prevent panics from existing, but ensures deterministic fail-fast behavior (no unwinding). Treat missing `panic=abort` as a **blocker**.

---

## Evidence checks (required in review)

### 6) Clippy gates must remain effective
Confirm (or request) that CI runs:
- `cargo clippy --all-targets --all-features -- -D warnings`

And that crate-level lint settings include (or remain equivalent to):
- `#![deny(clippy::unwrap_used)]`
- `#![deny(clippy::expect_used)]`
- `#![deny(clippy::panic)]`
- `#![deny(clippy::todo)]`
- `#![deny(clippy::unimplemented)]`
- `#![deny(clippy::dbg_macro)]`
- `#![deny(warnings)]`

If a PR weakens lint levels (e.g., changes `deny` → `warn`), treat as a **blocker** unless explicitly approved and documented.

### 7) Tests for failure modes
For changes affecting core logic:
- Require tests for:
    - expected behavior,
    - boundary conditions,
    - failure modes (invalid input, missing data, etc.).
- Prefer property tests/fuzzing where inputs are broad (optional but recommended).

---

## Guidance for acceptable patterns (reviewer should suggest)

### Preferred replacements
- Instead of `unwrap()`: use `ok_or(...)` / `ok_or_else(...)` and propagate with `?`
- Instead of `panic!`: return a domain error
- Instead of `a[i]`: use `a.get(i).ok_or(...)` or iterator logic
- Instead of `assert!` for input: validate at boundaries and return error

### Error handling style
- Encourage domain-specific error enums (`thiserror` is fine) and explicit mapping at module boundaries.

---

## Escalations / Exceptions
If a contributor claims a rule must be broken:
1. Require a written rationale in the PR description.
2. Require additional tests proving safety/failure behavior.
3. Require explicit maintainer approval.
4. Prefer localizing the exception (smallest possible scope).

> Default stance: for this repo profile, rule breaks are exceptional and should be very rare.
