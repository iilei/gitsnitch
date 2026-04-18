# Review Checklist (Rail/SIL-oriented)

## Correctness & determinism
- [ ] No `unsafe` introduced.
- [ ] No `unwrap/expect/panic/todo/unimplemented/dbg` in production paths.
- [ ] No unchecked indexing/slicing.
- [ ] Overflow behavior is explicit (`checked_*` etc.).
- [ ] Concurrency is bounded (no unbounded fan-out).

## Data & interfaces
- [ ] Inputs validated (ranges, schema assumptions, nullability).
- [ ] Outputs are deterministic given the same inputs (if required by component).
- [ ] Failure modes are explicit and safe (no silent partial results).

## Operability
- [ ] Logs/metrics are adequate and do not leak secrets.
- [ ] Timeouts/retries/backpressure are defined (for services).
- [ ] Migration/compatibility considerations noted (if applicable).

## Evidence
- [ ] Tests cover mainline and failure modes.
- [ ] CI gates passed (fmt/clippy/tests).
- [ ] Change references ticket/requirement ID (if required).
