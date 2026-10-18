# Implementation Plan: ENEA Ciepło Białystok

## Scope

Add ENEA Ciepło's heat interruption notices for Białystok as a provider separate from the existing `enea` electricity provider. Proposed company-level source ID: `enea_cieplo`, leaving room to add other ENEA Ciepło cities later. Initial coverage is limited to Białystok, Podlaskie Voivodeship.

Research checked on 18 October 2026:

- [Official ENEA Ciepło interruption page](https://www.enea.pl/strona-korporacyjna/grupa-enea/spolki/enea-cieplo/przerwy-w-dostawie-ciepla)
- [Official ENEA Ciepło company page](https://www.enea.pl/strona-korporacyjna/grupa-enea/spolki/enea-cieplo)

The official outage page identifies the utility as ENEA Ciepło Sp. z o.o. in Białystok and exposes notices containing street names and building numbers. Examples include a notice for 12 October 2026 with an 08:00–18:00 outage due to a network failure, and planned-work notices with date ranges such as 14–15 September 2026. Notices use Polish date/time text (`12.10.2026 r. od godz. 08:00 do godz.18.00`, `od 14.09.2026 r. godz. 8.00 do 15.09.2026 r. godz. 15.00`) and group multiple streets/buildings under property managers or institutions. Follow-on address lines may omit the `dot. bud. przy ul.:` prefix.

Browser inspection confirmed that the current notice is server-rendered inside a `main [tabs-target]` panel. Its desktop tab and mobile selector currently expose one notice, and the notice body is present in the page markup; no separate data API or feed was observed. The implementation therefore uses direct HTTP and HTML parsing without a WebView fallback. The page may expose more panels when more notices are published, so the parser reads each matching panel.

The page snapshot available during research contained a notice dated 12 October, already elapsed by the research date. Confirm how the site retains expired notices and filter expired alerts according to the app's existing time semantics so stale interruptions are not surfaced.

## Proposed changes

### Rust app

- Add `src-tauri/src/enea_cieplo.rs` with a separate `AlertProvider`; do not modify the existing `src-tauri/src/enea.rs` electricity RSS provider.
- Register the module and provider in `src-tauri/src/lib.rs`.
- Add `AlertSource::EneaCieplo` with serialized/display ID `enea_cieplo` in `src-tauri/src/api_logic.rs`; classify it as heat and scope current applicability to Białystok. Add or reuse a city matcher after checking existing city/TERYT helpers.
- Parse the page's notice sections into `UnifiedAlert`: locality Białystok, streets and house numbers where present, outage start/end, cause/description, and planned-versus-failure type. Mark locality metadata only when the alert can be matched to Białystok addresses. Keep house-number filtering in `Settings.filter_by_house_no` rather than filtering provider results unconditionally.
- Preserve useful upstream location text and customer/institution context where it helps users. Ignore boilerplate apology/contact paragraphs when forming duplicate alerts.

### Frontend

- Add `enea_cieplo` to the heating category in `public/script.js`, sorted alphabetically by ID within that category. Set its region to a Białystok matcher; add one only if no suitable matcher exists after checking the full object.
- Add Polish and English `source_enea_cieplo_name` and `source_enea_cieplo_short` strings in `public/i18n.js`.
- Use existing heat styling and the standard outage card fields. Do not add provider-specific colors.

### Android widgets

Existing heat providers have individual widgets and are included in aggregate widgets, so include this provider in that pattern:

- Add `EneaCieploWidgetProvider.kt` extending `BaseWidgetProvider`.
- Add the new source key to `BaseWidgetProvider.kt` label and heat-layout mappings, `WidgetConfigActivity.kt`, and `AndroidManifest.xml`; use `@xml/widget_single_info`.
- Add the provider to heat aggregation in `AllWidgetProvider.kt` and `TriWidgetProvider.kt`, and to the refresh flow in `WidgetUpdateWorker.kt`.
- Add `provider_enea_cieplo` and `widget_label_enea_cieplo` resources to both `values/strings.xml` and `values-en/strings.xml`, following nearby provider naming and label style.
- Verify widget counts continue to rely on the shared address/locality matching so only locally relevant alerts appear.

### Daemon and documentation

- Update the heat-source mapping in `src-tauri/src/bin/awaria_daemon.rs` if the new enum is not already covered by an appropriate category match; the daemon fetches the registered provider list.
- Add ENEA Ciepło Białystok to provider lists and descriptions in `README.md` and `README.pl.md`. Update any other provider inventory/spec docs found during implementation.

## Tests and validation

Add focused Rust parser and provider tests using saved representative HTML fixtures. Cover:

- planned work and failure notices, including both causes and varied Polish date/time punctuation;
- multiple property managers, streets, buildings, continuation lines, house-number suffixes, ranges, and slash-form numbers;
- notices with missing end times, malformed dates, boilerplate-only content, empty state, and repeated/archived notices;
- Białystok locality and street matching, including `filter_by_house_no` enabled and disabled;
- expired notice handling based on the app's existing alert-time behavior.

After implementation, run focused Rust tests and the relevant app checks: `npm run test:backend`, `npm run test:frontend`, `npm run test:android`, and a daemon compile check if daemon mappings change. Run live network or physical-device checks separately and report their limits; do not treat archived fixtures as proof of live-page behavior.

## Open items for review / implementation discovery

1. Resolved: the selected notice is server-rendered in the initial HTML; no separate data API or feed was observed.
2. The page's retention policy remains unknown. The app already filters ended alerts in the frontend, background monitoring, and Android widget count path.
3. The current page exposes one selected notice; the parser handles multiple panels if ENEA publishes them later.
4. The TERYT database contains two cities named Białystok. The provider uses city ID `922410` in Podlaskie and excludes the separate Białystok in Lubelskie (`886759`).

## Implementation status

Implemented after approval: the Rust provider and parser, source applicability and heat classification, frontend matching/translations, Android widgets, daemon mappings, add-on source schema, and Polish/English provider and privacy documentation. Tauron tests now inject mock base URLs per call instead of mutating process-wide environment variables, so they compile safely on Rust 2024 and can run in parallel. The full Rust suite, frontend tests, Android JVM tests, and production Rust/daemon checks passed.
