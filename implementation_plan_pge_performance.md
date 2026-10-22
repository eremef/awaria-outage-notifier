# PGE outage fetch performance and retry plan

**Status:** Implemented following user approval on 2026-10-22.

## Goal

Keep PGE's official outage data and matching behavior while reducing cold-fetch time and bounding stalls when the API is unhealthy.

## Research evidence (checked 2026-10-22)

- The official outage page is `https://pgedystrybucja.pl/wylaczenia/aktualne-przerwy-w-dostawie-energii`.
- Its client bundle calls `https://power-outage.gkpge.pl/api/power-outage` and serializes TERYT filters including `type=teryt`, `citySym`, and `streetSym`.
- A 30-day unfiltered request returned 1,275 outage rows, 4,790 address groups, and 3,836,436 response characters in about 7.8 seconds.
- A near-identical 30-day request filtered to `citySym=0808570` returned one outage row and three address groups in about 94 ms. This is a single-city live sample, not a guarantee for all cities.
- The response advertises `Cache-Control: max-age=300, public`. The current provider sends bounds with second-level precision, producing a different URL on each fetch.
- PGE has transiently returned plain-text 404 responses from the working API endpoint. Preserve retry coverage for those responses.
- The city autocomplete returns only rows where `SIMC.sym = SIMC.sympod`, so a saved city ID is a root SIMC code. A live PGE request for Łódź root `0957650` returned zero rows, while child district `0957666` returned 11.
- A live `type=teryt&streetSym=26544` request without `citySym` returned the Łódź-Bałuty outage in 887 response bytes; adding root `citySym=0957650` returned no rows.

## Implementation

### `src-tauri/src/pge.rs`

- Keep the official JSON endpoint.
- Prepare active, provider-applicable addresses before requesting data; return immediately if none apply.
- For street addresses with a selected TERYT ULIC ID, issue one `streetSym` request per unique five-digit street code without `citySym`. This avoids excluding rows stored under a child SIMC code.
- For addresses without a usable street ID, query seven-digit city SIMC codes only when the local TERYT database can enumerate a small root-plus-child scope. Use one broad request if the lookup is unavailable or the subtree exceeds eight codes.
- Use TERYT `sympod` mappings to match PGE child SIMC codes back to the selected root city, and use `streetSymUl` when present for exact street matching.
- Bound concurrent requests, deduplicate results by outage ID, and retain per-address alert assignment.
- Snap the rolling date-window bounds to stable 15-minute intervals while preserving the approximately 30-day look-ahead.
- Use a PGE-local retry policy: four attempts maximum; retry transport failures and HTTP 404, 408, 429, and 5xx responses; do not retry permanent 4xx responses or JSON decoding errors. Use a 12-second per-request timeout, 250/500/1000 ms backoff (respect a numeric `Retry-After` on 429), and a 30-second per-query deadline. Bound the overall multi-city fetch to 45 seconds and keep successful city results if another city request fails.

### Tests

Extend the PGE and TERYT module tests to cover:

- `streetSym` query parameters and five-digit zero-padding;
- root-plus-child SIMC lookup, small subtree filtering, and broad fallback for unavailable or large scopes;
- matching a PGE child locality such as Łódź-Bałuty to the selected Łódź root SIMC;
- one request per unique street and one broad fallback for addresses without safe scopes;
- merging/deduplicating repeated outage IDs across city responses while preserving address matching;
- stable date-window rounding;
- retries for transient 404, transport, 408, 429, and 5xx failures, with no retry for permanent 4xx or invalid JSON;
- partial success and bounded timeout behavior.

## Scope decisions

- Provider registration, frontend, Android, daemon, and localization contracts stay the same. City-only addresses in runtimes without a Tauri app handle use the broad request when the TERYT child mapping is unavailable; street-scoped addresses can still use `streetSym`.
- Keep the retry logic local to PGE; do not change the shared retry helper used by other providers.
- Retain the existing five-minute in-process cache. The provider change improves cold and forced refreshes and makes upstream cache reuse more likely.
- No README currently documents the PGE endpoint or retry behavior, so this internal implementation does not require a documentation change.

## Validation

- Focused PGE tests in `src-tauri/src/pge.rs`.
- Rust format check for the changed Rust module.
- `cargo check --lib --locked` and the full Rust test suite if the focused check passes.
- Do not claim device or live-provider validation from mock tests. The source endpoint was checked live during research; a post-change live request can be run separately if needed.

## Validation results

- `rustfmt --edition 2024 --check src/pge.rs src/teryt.rs` passed.
- `cargo check --lib --locked --target-dir target_pge_performance` passed.
- Focused PGE tests passed: 16 passed, 1 ignored live-source test. TERYT tests passed: 6.
- The full Rust suite passed: 264 passed, 8 ignored, 0 failed.
- Live source checks confirmed that the Łódź root query misses Bałuty records and the `streetSym`-only query retrieves them. The post-change Rust provider itself was validated with mocks rather than a live invocation.

## Assumptions and limits

- `AddressEntry.city_id` is a root SIMC code because city autocomplete filters to `sym = sympod`; PGE records may use a child `citySym` instead.
- Street codes can be reused across localities, so city/commune, street-name, and TERYT scope matching remains authoritative after the API filter.
- The 45-second overall deadline can return partial city data during a prolonged outage; provider errors should preserve the app's existing stale-cache recovery behavior.
