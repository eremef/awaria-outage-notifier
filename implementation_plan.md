# Implementation Plan: Energa Ciepło Kaliskie Heat Alerts

- **Status:** Implemented; validation complete
- **Research checked:** 19 October 2026
- **Utility type:** District heat
- **Proposed provider ID:** `energa_cieplo_kaliskie`
- **Proposed display name:** Energa Ciepło Kaliskie

## Provider and coverage

The official provider is Energa Ciepło Kaliskie Sp. z o.o. Its public outage notices identify interruptions in Kalisz. Scope this source to Kalisz in Wielkopolskie, using the existing `is_kalisz` address predicate and TERYT city ID `936579` rather than adding a broader Energa heat territory.

Official sources:

- [Aktualności / outage notices](https://energa-cieplokaliskie.pl/aktualnosci)
- [Representative outage notice: Awaria sieci 01.10.2026](https://energa-cieplokaliskie.pl/aktualnosci/awaria-sieci-01-10-2026)
- [Official heat-network map](https://energa-cieplokaliskie.pl/informacje-dla-klientow/mapa-sieci)

## Research findings and parser assumptions

- Live DOM inspection confirmed the listing uses `.basic-card` entries with `.basic-card-content h5`, an excerpt paragraph, and an `a.read-more` link. Each detail page has an `article.publication`, a `.article-title h1`, and notice text under `.prnt-pub__content`. No API or feed link appeared in the page's alternate/feed links; use the official HTML list and linked detail pages.
- The representative article was published on 1 October 2026 at 07:11. Its body says the outage is on 1 October 2026 from 07:00 to 21:00. Publication time must never be used as the outage start. The notice had already expired by the research date; it is evidence for a historical fixture, not a live-active parser check.
- The notice describes a network failure (`awaria sieci`) and says heat supply will be stopped. It lists multiple streets and buildings: Legionów 42, 40, 40A, 29, and Polna 20 with the suffix `Pawilon`. Preserve the full useful notice text, normalize each street for matching, and retain house numbers and suffixes in the alert message.
- The article body stores times as `7<sup>00</sup>` and `21<sup>00</sup>`. Search indexing flattens these to `700` and `2100`, so the parser converts the superscript minutes before parsing clocks.
- The pages use Polish dates and local Polish times. Store times in the same local, timezone-naive format used by existing providers. Confirm the actual HTML date format before selecting selectors and date parsing rules.
- The news list also contains unrelated corporate notices, so entries must be classified from outage-specific title/body text; do not turn every news item into an alert. No pagination or retention limit was visible in the checked list. Recheck whether all current news entries are linked from this page and whether additional pages exist when inspecting the raw HTML.
- The browser successfully loaded the listing and representative article. A direct `Invoke-WebRequest` from this workspace failed during TLS certificate authentication; verify connectivity through Awaria's configured `reqwest` client as part of validation.
- No separate planned heat interruption was found in the available notice sample. Treat explicit failure wording as a failure and work wording such as `prace` or `planowane` as planned; add tests for both categories, with the planned wording marked as a synthetic fixture until an official example is available.

## Proposed implementation

### Rust backend and matching

- Add `src-tauri/src/energa_cieplo_kaliskie.rs` with an `AlertProvider` implementation. Reuse the existing HTTP client and HTML/date/address utilities. Fetch the news page, identify outage-related detail links, and parse title, body, event date/time, and listed locations from the detail pages. Keep selectors grounded in the raw official markup and avoid treating publication metadata as outage schedule data.
- Parse one-day and multi-day Polish schedules, colon/dot time formats, and the superscript hour/minute representation used by the official page. Preserve unknown schedule fields when the notice omits them. Review whether a dated notice with no end time should expire at local 23:59 on the stated outage date to avoid stale alerts on a retained news page; never use the publication date as a fallback event date.
- Normalize street names for locality matching, include `Kalisz` in `location`, and set `address_index`/`is_local` only when an active Kalisz address matches the notice street. Leave house-number enforcement in `Settings.filter_by_house_no` so the user's setting remains authoritative.
- Update `src-tauri/src/api_logic.rs`: add the serialized source ID, display name, Kalisz service-location mapping using the existing predicate, source string mapping, and heat incident classification. Add source-scope tests for Kalisz and a non-Kalisz address.
- Update `src-tauri/src/lib.rs` to declare and register the provider.

### Frontend and translations

- Update `public/script.js` with the new heating source, sorted alphabetically by provider ID within the heating group, using the existing `kalisz` region. Add it to the single-city guard so same-street names in other cities cannot match this source.
- Update `public/i18n.js` with English and Polish provider name and short-name entries. Reuse generic heat styling and existing alert-card fields.

### Android widgets

The app has individual source widgets and aggregate heat widgets, so include this source in the existing heat-widget pattern:

- Add `EnergaCieploKaliskieWidgetProvider.kt` and register it in `AndroidManifest.xml` using the shared `@xml/widget_single_info` resource.
- Update `WidgetConfigActivity.kt`, `BaseWidgetProvider.kt`, `WidgetUpdateWorker.kt`, and the heat-source lists in `AllWidgetProvider.kt` and `TriWidgetProvider.kt`.
- Add provider and widget labels to `res/values/strings.xml` and `res/values-en/strings.xml`.
- Keep widget alert results restricted to locally matching notices (`is_local == Some(true)`). Do not add a provider-specific widget layout or info XML.

### Home Assistant daemon and documentation

- Update `src-tauri/src/bin/awaria_daemon.rs` so the new source is categorized as a heat outage.
- Add `energa_cieplo_kaliskie` to the source allowlist in `awaria_outage_monitor/config.yaml` so the add-on accepts the new provider.
- Update provider examples or inventories in `README.md`, `README.pl.md`, `awaria_outage_monitor/README.md`, and `awaria_outage_monitor/README.pl.md` where those lists describe available sources.

## Focused test coverage

Add Rust parser and source-scope tests using a minimized fixture based on the official article. Cover:

- Published timestamp kept separate from the 1 October outage date and its 07:00–21:00 window.
- The raw superscript time markup, Polish date/time formats, and valid/malformed or incomplete schedules.
- Multiple streets, multiple building numbers, lettered numbers such as `40A`, and descriptive suffixes such as `Pawilon`.
- Multiple outage articles, unrelated corporate news, and an empty/no-outage page.
- Failure classification and planned-work classification using a clearly synthetic planned notice until an official planned example is found.
- Kalisz/local street match, non-Kalisz rejection, and house-number preference on/off leaving provider locality matching unchanged.
- Widget integration retaining only alerts marked local.

## Validation performed

- Focused provider tests: 9 passed; one live HTTP test was also run separately and passed using Awaria's configured client.
- `npm run test:backend`: 160 passed, 3 ignored.
- `npm run test:frontend`: 43 passed.
- `npm run test:android`: build and JVM tests succeeded. Gradle reported Java 21/source 8 and Gradle deprecation warnings.
- `npm run rust:check` and `cd src-tauri && cargo check --bin awaria-daemon` both succeeded.
- No physical Android widget run was performed. The only observed outage notice had expired at research time, so a live-active-notice comparison remains dependent on a future official notice.

## Approved implementation choices

1. Use `energa_cieplo_kaliskie` and “Energa Ciepło Kaliskie”, limited to Kalisz.
2. Expire a dated notice without an end time at local 23:59 on the stated outage date to avoid retaining a stale alert from the news page.
3. Use the verified HTML selectors above; confirm Awaria's configured `reqwest` client can fetch the live page during validation.

---

# Implementation Plan: MPWiK Bydgoszcz Water Alerts

- **Status:** Implemented and verified. This plan is separate from the Energa Ciepło Kaliskie plan above.
- **Research checked:** 19 October 2026
- **Utility type:** Water and wastewater network interruptions
- **Proposed provider ID:** `mpwik_bydgoszcz`
- **Proposed display name:** MPWiK Bydgoszcz
- **Official outage page:** [Awarie i wyłączenia](https://mwik.bydgoszcz.pl/awarie-i-wylaczenia/)
- **Official context:** [Zgłoszenie awarii](https://mwik.bydgoszcz.pl/zgloszenie-awarii/)

## Provider, coverage, and observed page behavior

The page is published by Miejskie Wodociągi i Kanalizacja w Bydgoszczy. Its notices identify affected streets but do not repeat the city in each row, so scope the provider to Bydgoszcz using the existing `is_bydgoszcz` city predicate and project TERYT city ID `928363`. The official contact page describes water and sewer network faults, consistent with the requested water category.

The official outage page currently renders two sections, each with `DATA`, `GODZINA`, and `ULICA` columns:

- `Planowane wyłączenie wody` is the planned-work section. On the research date it listed two rows for 19 October 2026: Focha 4, 07:00–15:00, and Modrzewiowa, 10:00–12:00.
- `Awaryjne wyłączenie wody` is the emergency section. The live page showed `BRAK` and no active rows.
- An earlier official-page search snapshot showed an emergency row for 21 September 2026, 10:10–17:00, on Skowronków, Drozdów, and Cietrzewia. This is historical page evidence, not an active notice on the research date.

Dates use `DD.MM.YYYY`. Time ranges use 24-hour clocks with either a hyphen or en dash and optional spaces (for example, `7:00-15:00` and `10:00 – 12:00`). A row may contain multiple comma-separated street names and may include a house number. The section title supplies the planned/emergency classification. The page does not show causes, a publication timestamp, or an explicit city field in the notice rows. Treat the schedule as Polish local time, using the same local, timezone-naive date strings as existing providers.

The page appears to be a single current-state listing: no pagination or notice-history control was visible, and the live page includes an explicit empty state. Browser inspection confirmed the official WordPress REST endpoint `https://mwik.bydgoszcz.pl/wp-json/wp/v2/pages/8760` (page slug `awarie-i-wylaczenia`, ID `8760`). Its JSON `content.rendered` field contains the same two notice tables, so fetch this endpoint and parse that field rather than the surrounding page. A direct PowerShell request to the HTML page failed TLS authentication, while browser access to both the page and REST endpoint succeeded; verify app-client connectivity before relying on the endpoint. The provider's update interval and how long notices remain on the page are not documented.

## Proposed implementation

### Rust backend and address matching

- Add `src-tauri/src/mpwik_bydgoszcz.rs` with an `AlertProvider` implementation, following the current water-provider pattern and reusing the configured client, Serde JSON, HTML/date/address utilities, and existing async conventions.
- Fetch the official REST page endpoint, deserialize `content.rendered`, and parse the two named sections and their table rows. Map each valid row's date and time range to `startDate` and `endDate`; map the full affected street list and a Bydgoszcz location into the alert. Preserve house numbers and the section wording in `message`, but normalize street names separately for locality matching. Do not derive outage times from page publication metadata.
- Treat `BRAK`, blank table rows, and non-notice text as no alert. Require a valid date and complete valid time range before emitting a scheduled alert; log and skip malformed or incomplete rows rather than inventing a time. Keep same-day end times on the stated date and handle a range crossing midnight only if the official format provides evidence for it.
- Preserve the section classification in the message (`Planowane wyłączenie wody` or `Awaryjne wyłączenie wody`); the existing alert model classifies this provider in the water category and has no separate planned/failure field.
- Match all listed streets against active addresses in Bydgoszcz and set `addressIndex`/`isLocal` using existing provider conventions. House-number enforcement stays in `Settings.filter_by_house_no`, so retained source addresses and the user's house-number preference remain authoritative.
- Update `src-tauri/src/api_logic.rs` with the serialized source ID, display name, Bydgoszcz applicability mapping, display-string mapping, and water classification. Add city-scope coverage for Bydgoszcz and a nearby non-Bydgoszcz city.
- Declare the provider module and register it in `src-tauri/src/lib.rs`.

### Frontend and translations

- Add `mpwik_bydgoszcz` to the water group in `public/script.js`, alphabetically by provider ID, using the existing `bydgoszcz` region and city guard so same-named streets elsewhere cannot match.
- Add Polish and English provider name and short-name strings to `public/i18n.js`. Reuse the generic water category styling and standard alert-card fields.
- Update `public/awaria-card.js` so the Home Assistant custom card assigns the provider its existing water icon and color.
- Add frontend coverage for provider selection and Bydgoszcz-only matching where the current tests expose those behaviors.

### Android widgets

The app exposes per-provider widgets and water aggregate widgets, so add this source through the existing water-widget pattern:

- Add `MpwikBydgoszczWidgetProvider.kt` and register it in `AndroidManifest.xml` with the shared `@xml/widget_single_info` resource.
- Update `WidgetConfigActivity.kt`, `WidgetUpdateWorker.kt`, `BaseWidgetProvider.kt`, and the water-source lists and enabled checks in `AllWidgetProvider.kt` and `TriWidgetProvider.kt`.
- Add the provider and widget labels to `res/values/strings.xml` and `res/values-en/strings.xml`.
- Keep widget results restricted to locally matching alerts (`isLocal == true`); do not create a provider-specific widget info XML or layout.

### Daemon, add-on, and documentation

- Add the source to the water classification in `src-tauri/src/bin/awaria_daemon.rs`.
- Add `mpwik_bydgoszcz` to the selectable source list and validation pattern in `awaria_outage_monitor/config.yaml` if both lists continue to define all providers accepted by the add-on.
- Update provider inventories and examples in `README.md`, `README.pl.md`, `awaria_outage_monitor/README.md`, and `awaria_outage_monitor/README.pl.md`.

## Focused test coverage

Add Rust parser and matching tests using minimized fixtures from the official page. Cover:

- Both section headings and classification, including planned and emergency rows.
- Polish dates, one- and two-digit hours, hyphen/en-dash ranges, optional whitespace, invalid times, and same-day start/end handling.
- Multiple comma-separated streets, house-number text such as `Focha 4`, and matching each street without provider-side house-number filtering.
- `BRAK`, blank rows, no tables, unrelated content, and malformed or incomplete entries.
- Bydgoszcz address association, rejection of a nearby non-Bydgoszcz city, and the central house-number setting enabled/disabled behavior.
- Widget-source exposure and retention of only locally matching alerts.

Add or update frontend source/region tests and Android widget-list tests in the existing patterns. No active emergency notice was visible on 19 October, so emergency parsing can be checked against the earlier official-page snapshot fixture, but live emergency behavior cannot be compared during this research pass.

## Validation commands

- Run focused Rust parser/source tests and `npm run test:backend`.
- Run `npm run test:frontend` and `npm run test:android`.
- Run `npm run rust:check` and `cd src-tauri && cargo check --bin awaria-daemon`.
- Fetch the official REST endpoint with Awaria's configured client to verify TLS, JSON shape, rendered table markup, current empty-state behavior, and parser output against the two live planned rows. Physical Android widget behavior remains device validation.

## Validation results

- `npm run test:backend`: passed, 167 passed and 4 ignored.
- `npm run test:frontend`: passed, 45 tests.
- `npm run test:android`: `BUILD SUCCESSFUL`; the provider widget source/action test compiled and ran with the suite.
- `npm run rust:check` and `cargo check --bin awaria-daemon`: passed.
- `rustfmt --edition 2024 --check src-tauri/src/mpwik_bydgoszcz.rs`: passed.
- The ignored live provider check using `NetworkState::build_client()` passed and parsed both current planned rows from the official REST endpoint; the Focha row matched the configured Bydgoszcz address.
- `git diff --check`: passed. Physical Android widget behavior was not tested on a device.

## Open questions and assumptions for review

1. Implementation uses the requested display name “MPWiK Bydgoszcz” and conventional source ID `mpwik_bydgoszcz`.
2. The REST endpoint is observable, but the page does not document its update cadence or retention period. The plan makes no claim about how long old rows remain.
3. The emergency parser fixture will use a historical official-page snapshot because the live section was empty on the research date. A future active notice is needed for live comparison.
4. Rows without a complete date and time range will be skipped until the provider documents or demonstrates another valid format.
