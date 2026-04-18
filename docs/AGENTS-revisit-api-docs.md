# API Revisit Note (Superseded)

This file is kept as a lightweight historical marker.

The detailed review that originally lived here predated current canary behavior and became stale after API/implementation changes. Use the files below as the authoritative sources instead.

## Source of Truth

1. `docs/api_design/api_v1.schema.json` (primary contract)
2. `docs/api_design/api_v1.example.json` (canonical fixture)
3. `docs/api_design/api_design.plantuml` (conceptual model)
4. `README.md` (runtime behavior and CLI docs)

## Resolved Since Original Review

1. `exit_code_if_violations_found` removed from contract.
2. `violation_severity_as_exit_code` introduced.
3. Violation severity and severity band thresholds capped at `0..250`.
4. Internal/runtime error exit range reserved to `251..255`.

## Open Follow-up (Optional)

1. Add integration tests that assert end-to-end process exit behavior for mixed violation severities.
2. Consider compile-time/parse-time regex validation in config loading to fail earlier.

If this note no longer provides value, it can be deleted.
