# Implementation Plan: PEWIK Gdynia Water Outage Alerts

**Status:** Implemented after user approval on 2026-10-21
**Research checked:** 2026-10-21

## Provider and coverage

- **Provider:** PEWIK GDYNIA Sp. z o.o.
- **Proposed provider ID:** `pewik_gdynia`.
- **Utility:** Drinking-water supply interruptions and failures. PEWIK states that it does not supply domestic hot water.
- **Initial scope:** Emergency and planned drinking-water interruption notices from PEWIK’s two official pages. The planned publication currently has no sample notice, so that parser path remains fixture-based until PEWIK publishes one.
- **Official emergency notice page:** <https://pewik.gdynia.pl/awarie/informacje-o-awariach/>.
- **Official planned outage page:** <https://pewik.gdynia.pl/awarie/planowane-wylaczenia/>.
- **Official service-area page:** <https://pewik.gdynia.pl/strefa-mieszkanca/nasze-uslugi/> (last updated 2026-06-30 when checked).

PEWIK lists water service in Gdynia, Rumia, Reda, and Wejherowo, plus named parts of Gmina Kosakowo (Dębogórze Wybudowanie, Kazimierz, southern Kosakowo, and Pogórze) and Gmina Wejherowo (Bolszewo, Gościcino, eastern Góra, Kniewo, Orle, southern Warszkowo, and Zamostne). The page identifies Gdańskie Wodociągi as serving Gdynia streets Łosiowa, Sarnia, and Jelenia; it also names military areas and Port Gdynia as other service territories. The provider predicate excludes the three named streets and districts explicitly marked as port or military. It must not treat all of either commune as PEWIK territory. Partially served villages remain an approximation unless Awaria has enough address detail to distinguish the served part.

## Source behavior and observed notice

- The supplied page embeds BlueAlert publication HTML at <https://app.bluealert.pl/pewikgdynia/articles/external/1/>. The page returned HTTP 200 and a short HTML document with notice text inside `.message_body`; no REST, GraphQL, JSON, or RSS feed was found during this research.
- The notice checked on 2026-10-21 reads, in substance: a water-supply failure in Gdynia on Owsiana; shortages may also affect Owsiana and Lniana; expected restoration is at 19:00; no house numbers, start hour, or cause are given. The text contains the explicit date `21.10.2026`.
- The content is HTML with nested paragraph markup and Polish entities such as `&nbsp;`. Parse visible text while preserving paragraph boundaries. Do not depend on the current nested-tag shape.
- This embed presents one current publication, not a browsable archive. Its retention/update behavior and the no-notice response were not observable from the active notice. A malformed page or network failure must remain a fetch error; do not silently convert it to an empty alert list. Confirm the emergency endpoint’s 404/empty semantics before treating a future 404 as “no active outage.”
- Planned notices use the separate official page, <https://pewik.gdynia.pl/awarie/planowane-wylaczenia/>, which embeds <https://app.bluealert.pl/pewikgdynia/articles/external/2/>. It returned 404 during initial research, then HTTP 200 with an empty document and no `.message_body` during implementation. No representative planned notice was available. Treat a blank HTTP 200 as no current planned notice; keep non-success statuses and non-empty malformed documents as errors. The planned parser fixture is an informed assumption based on the shared BlueAlert publication route, not a live parser validation.

### Field mapping and parsing

| Upstream content | Awaria handling |
| --- | --- |
| Explicit date such as `21.10.2026` | Parse as the notice’s outage date; publication time is not exposed separately. |
| `godz. 19.00` after expected-restoration wording | Parse as the expected local end time on that date. |
| No start hour in the observed notice | Preserve the date without inventing a start hour. |
| City and street references in prose | Normalize the locality and each affected street for address matching; keep the complete source notice in the message. |
| No listed house numbers | Match by locality and street; if later notices list numbers, use the central `Settings.filter_by_house_no` behavior. |
| No cause in the observed notice | Leave cause unknown; do not infer one from “awaria wodociągowa.” |

Interpret Polish dates and times in `Europe/Warsaw`. Handle dotted or colon time separators, whitespace, diacritics, and punctuation. Keep multiple affected streets in one notice together so a match on one street does not discard the other useful location details. An expected restoration time is an estimate and should be represented as the stated end time, not as proof that service was restored.

## Proposed implementation

### Rust backend

- Add `src-tauri/src/pewik_gdynia.rs` implementing `AlertProvider`, using the repository HTTP client and `scraper` for the BlueAlert HTML. Normalize the page into a water alert, parse the date/restoration time, preserve full notice text, match all affected streets, and create a stable hash.
- Add parser and normalization tests using a saved fixture based on the observed HTML, plus empty/blank content, malformed notice, missing date/time, multiple streets, diacritic/casing variation, and updated notice content.
- Fetch both BlueAlert publications independently. The emergency parser is based on the live notice. The planned path shares the HTML parser but its date-range fixture is not live-validated; revise it when a representative planned notice becomes available. Keep planned and emergency notices correctly classified in the message and date/location fields.
- Update `src-tauri/src/api_logic.rs` with `AlertSource::PewikGdynia`, the serialized ID/display name, water-provider classification, and an eligibility predicate for PEWIK’s published service area. Exclude the three named Gdynia streets served by Gdańskie Wodociągi where address data permits. Do not classify all of Gminas Kosakowo or Wejherowo as eligible.
- Update `src-tauri/src/lib.rs` to declare and register the provider in each applicable provider registry.
- Keep house-number filtering in `Settings.filter_by_house_no`; do not add provider-specific house-number enforcement. For notices without numbers, a configured address on an affected street can match regardless of its house number.

### Frontend, Home Assistant, and daemon

- Update `public/script.js` with the water source entry and PEWIK service-area matching. Keep `SOURCES` grouped by utility and sort water providers by ID. Match notice locality and street before marking it local; keep local/nonlocal location display rules and the full notice message.
- Add English and Polish full/short provider labels in `public/i18n.js`. Update `public/awaria-card.js` only if its existing generic water-provider metadata does not cover the new ID.
- Update `src-tauri/src/bin/awaria_daemon.rs` to classify this source as a water outage. It uses ordinary HTTP/HTML and should not be added to the daemon’s WebView-only list.
- Add `pewik_gdynia` to `awaria_outage_monitor/config.yaml` and the English and Polish add-on README provider lists.

### Android widgets and documentation

- Add the water-provider widget using the existing widget base and shared `@xml/widget_single_info`; register it in the Android manifest, widget configuration selection, and refresh worker. Add source/category mapping to the base widget and the source to the relevant aggregate widget lists.
- Add Polish and English Android resource labels. Keep widget results restricted to alerts locally matching the configured address (`is_local == Some(true)`).
- Update `README.md` and `README.pl.md` provider lists. There is no root `docs/` directory in this checkout; update any other provider list discovered during implementation.

### Address matching and locality

- Normalize Polish diacritics and compare provider locality/street names using the existing address utilities. Handle street suffixes and common punctuation without weakening matching to a city-only match for a street-specific notice.
- Use the official water-service area, not OPEC Gdynia’s heating area. Include Gdynia, Rumia, Reda, and Wejherowo, plus the exact named service localities in the two communes. Where the official source says only part of a locality is served, record the city/locality match as approximate and rely on the notice’s listed streets for alert matching; do not claim address-level network verification.
- Keep the complete PEWIK notice in the alert message for both local and nonlocal alerts. For nonlocal display, show only the locality in the location field.
- When house numbers are present in future notices, let the central setting control comparison. Without listed numbers, match a listed street as a whole.

## Focused validation after approval

- Rust tests: date and dotted-time parsing in `Europe/Warsaw`; absent start hour; multiple streets; no house-number list; `&nbsp;` and nested paragraphs; planned and emergency notice samples when available; no active/blank content; malformed response; stable hash changes; city/street locality matching; service-area boundaries and Gdynia exclusions; and house-number setting behavior when numbers are supplied.
- Frontend tests: source registration/category/labels, supported and unsupported localities, Gdynia exceptions, and matching of a multi-street notice.
- Android checks: source/widget configuration and category mappings, plus the existing Android JVM suite.
- Proposed checks: `cargo test --manifest-path src-tauri/Cargo.toml pewik_gdynia`, `npm run test:frontend`, `npm run test:android`, `npm run rust:check`, `cargo check --manifest-path src-tauri/Cargo.toml --bin awaria-daemon`, `rustfmt --edition 2024 --check src-tauri/src/pewik_gdynia.rs`, and `git diff --check`.
- The live emergency fetch with Awaria’s configured HTTP client passed. A physical Android widget run is separate from automated checks and was not performed.

## Review assumptions and open questions

1. The planned-outage page is included as a target per the user’s follow-up. A blank HTTP 200 is treated as no current publication; no active planned notice was available to confirm future wording or retention behavior.
2. The active example has a date and expected restoration time but no outage start hour. The plan preserves the missing start hour rather than inferring one.
3. BlueAlert’s historical-retention behavior and emergency-endpoint 404 semantics remain unknown. A blank HTTP 200 is treated as no current publication; non-success responses remain errors.
4. PEWIK’s public service-area list contains partial villages and exceptional Gdynia territories. Awaria’s locality/street fields may not encode exact service boundaries, so the plan uses explicit locality names and street-level notice matching and documents that limitation.

**Review gate:** Approved by the user on 2026-10-21; implementation is complete.

## Implementation and validation record

- Added `pewik_gdynia` as a water provider across Rust registration and classification, frontend source selection and region matching, Home Assistant, the daemon, Android widgets and aggregate widgets, translations, add-on configuration, and English/Polish documentation.
- Emergency notices are fetched from the BlueAlert HTML using Awaria’s configured Rust client. The planned publication is queried separately. When checked, its endpoint returned HTTP 200 with a blank document and no `.message_body`; the provider treats that blank response as no current planned notice. A non-empty malformed document remains an error.
- The live emergency smoke test parsed one current alert with no endpoint errors. There was no active planned notice to validate its wording or retention behavior; the planned date-range parser is covered by a representative fixture and remains provisional until a real notice is available.
- `cargo test --manifest-path src-tauri/Cargo.toml pewik`: 7 passed. The date-only upcoming-notification check passed separately.
- The ignored live provider smoke test passed and parsed one active alert.
- `npm run test:frontend`: 68 passed.
- Android JVM tests: 16 passed. Gradle and Robolectric caches were redirected to ignored workspace build directories because the default user cache paths were not writable.
- `npm run rust:check`, `cargo check --manifest-path src-tauri/Cargo.toml --bin awaria-daemon`, `rustfmt --edition 2024 --check src-tauri/src/pewik_gdynia.rs`, `git diff --check`, and `git diff --cached --check` passed.
- The full backend suite and a physical Android widget/device run were not performed.
