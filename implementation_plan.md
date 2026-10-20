# Implementation Plan: Toruńskie Wodociągi Water Alerts

**Status:** Implemented on `codex/torunskie-wodociagi` on 2026-10-20
**Research checked:** 2026-10-20

## Provider and coverage

- **Provider:** Toruńskie Wodociągi Sp. z o.o.
- **Proposed source ID:** `torunskie_wodociagi`
- **Utility category:** Water
- **Official outage page:** <https://wodociagi.torun.com.pl/aktualnosci/awarie-wodociagow/314-awarie-i-prace-planowe>
- **Coverage evidence:** The company’s 2025 water-quality notice identifies its Toruń public water supply as serving Toruń and the municipalities of Lubicz, Łysomice, Obrowo, Wielka Nieszawka, and Zławieś Wielka. The city identifier already used by Awaria for Toruń is `929448`.
- **Initial applicability proposal:** Toruń plus those five municipalities, matched using the existing address city and commune fields. The bulletin sample checked so far is a Toruń street notice. Toruńskie Wodociągi also documents bulk water delivery to Ciechocinek; the current bulletin does not show whether it publishes resident-facing alerts for that wholesale customer, so do not match Ciechocinek or Inowrocław unless the bulletin provides direct evidence.

Official references:

- Outage and planned-work bulletin: <https://wodociagi.torun.com.pl/aktualnosci/awarie-wodociagow/314-awarie-i-prace-planowe>
- 2025 water-quality notice and named supply area: <https://www.wodociagi.torun.com.pl/res/2025/Ocena_jakosci_wody_Torun_I_polrocze_2025.pdf>
- Company water-network description: <https://www.wodociagi.torun.com.pl/o-nas/siec-wodociagowa>
- Toruń city description of its municipal water supplier: <https://torun.pl/pl/miasto/wodociagi-i-kanalizacja>

## Source behavior and observed data

- The outage URL returned HTTP 200 in a browser. The Windows command-line request could not complete TLS negotiation, so that failure is not evidence that the public page is unavailable to Awaria. The implementation should use the repository’s configured Rustls HTTP client and verify it separately.
- The article is server-rendered HTML (`article-314`) using the site’s YOOtheme template. It contains no advertised `link rel="alternate"` feed. The observed page load made no XHR/fetch calls for outage data; no official API, JSON feed, or RSS endpoint surfaced in the targeted search.
- The article has separate tables for **Awaria wodociągowa**, **Awaria kanalizacyjna**, **Prace na sieci wodociągowej**, **Prace na sieci kanalizacyjnej**, and a separate **Prace inwestycyjne** table. Empty tables use `brak` placeholder cells. There is no visible pagination or incident archive on this page; it appears to be a current bulletin that is edited in place.
- The article metadata says it was published on 2025-11-04 and last modified on 2026-10-20. Those values are page metadata, not outage dates.
- The live water-work row checked on 2026-10-20 contains:
  - date: `22.10.2026`;
  - work address: `ul. Batorego 42-44`;
  - details: a planned interruption on 22.10.2026 from 7:30 to 10:00, due to DN 100 water-main reconstruction;
  - affected properties on Kościuszki, Batorego, Dworcowa, and Świętopełka, with individual house numbers, ranges, and letter suffixes.
- The current water-failure table is empty, so no live example of an unplanned water-failure row or its missing-time behavior was available. The separate investment table gives month/year dates and project descriptions, not water-service interruption times.

## Proposed parsing and normalization

- Add a small HTML parser for the fixed official article URL. Reuse existing HTTP retry, HTML, date, and address utilities where they fit; do not add a new shared abstraction without a demonstrated need.
- Associate each table with its section heading. Parse only water-supply failures and water-network works. Skip `brak` rows, sewer-only rows, and investment projects because they do not establish a potable-water interruption.
- Use the date cell and incident details together. Parse `dd.mm.yyyy` dates and Polish 24-hour time ranges such as `7:30-10:00` as Europe/Warsaw local time. Keep article publication/update timestamps out of outage start/end values.
- Preserve the full source details, including cause and the complete affected-property list. Use affected streets and house numbers for matching; the address cell can describe the work site rather than every affected property.
- For rows with no time range, retain the stated date without inventing a time or end. Leave an unknown end unset. A water failure remains current while it appears in the current-failure table; planned work expires at its parsed end when one is available. This fallback needs a fixture and a live notice example before being treated as validated behavior.
- Generate a stable alert hash from normalized row content and its parsed schedule so refreshing the same bulletin does not create a new alert, while a materially changed schedule or location does.

## Proposed file changes after approval

### Rust backend

- Add `src-tauri/src/torunskie_wodociagi.rs` with the provider fetcher, section-aware table parser, normalization, address matching, and deterministic fixture tests.
- Update `src-tauri/src/api_logic.rs` with `AlertSource::TorunskieWodociagi`, its serialized ID/display string, the Toruń-plus-five-municipality applicability checker, and coverage tests.
- Update `src-tauri/src/lib.rs` to declare and register the provider. Keep house-number enforcement in the existing central `apply_alert_filtering` path. Since one bulletin row can cover several streets while `UnifiedAlert` carries one `addressIndex`, verify that the central filter can correctly evaluate every relevant active address when house-number filtering is enabled; make the smallest central adjustment and add a regression test if needed.
- Update `src-tauri/src/bin/awaria_daemon.rs` to classify the new source as a water alert for Home Assistant output.

### Frontend and translations

- Update `public/script.js` with the water source entry, the applicability matcher for Toruń and the covered municipalities, and the source-specific locality filter only if current generic matching requires it. Keep water sources grouped by category and alphabetized by ID.
- Add the provider’s English and Polish names and short labels to `public/i18n.js`. Reuse generic water-category styling; do not add provider-specific colors.
- Review `public/awaria-card.js` for any explicit source-name or category map and update it only if required.

### Android widgets

- Add `TorunskieWodociagiWidgetProvider.kt` using the existing base class and water icon/color.
- Register it in `AndroidManifest.xml`, `WidgetConfigActivity.kt`, and `WidgetUpdateWorker.kt`.
- Add its source label/category mapping in `BaseWidgetProvider.kt`, include the source in the water aggregation lists in `AllWidgetProvider.kt` and `TriWidgetProvider.kt`, and add Polish and English resource strings.
- Reuse `@xml/widget_single_info`; do not add provider-specific widget XML. Keep widget alerts restricted to `is_local == Some(true)`.

### Daemon, add-on, and documentation

- Add `torunskie_wodociagi` to the source allowlist in `awaria_outage_monitor/config.yaml` and to the English and Polish add-on README examples.
- Update root `README.md` and `README.pl.md` provider lists or descriptions where the source list is documented. There is no root `docs/` directory in the current checkout.
- No add-on transport changes are expected beyond the source classification and allowlist, because the add-on consumes the daemon’s normalized alert output.

## Focused tests and checks

- Rust fixtures: populated and empty tables; skip `brak`; include water failures and planned water work; exclude sewer and investment rows; parse date-only and timed rows; handle missing end times; preserve multi-street details, ranges, Polish characters, and alphanumeric house suffixes; ignore malformed rows; stable hashes; and verify matching across Toruń and covered communes.
- House-number tests: confirm that the central setting remains authoritative, including a listed and an unlisted house on the same street and a matching address on a later street in a multi-street notice.
- Frontend tests: verify source registration, category, label, and locality matching for the covered service area.
- Android JVM coverage: verify provider source/category mappings if the existing widget tests provide a suitable seam.
- Proposed automated checks after implementation: `npm run test:backend`, `npm run test:frontend`, `npm run test:android`, `npm run rust:check`, `cd src-tauri && cargo check --bin awaria-daemon`, `cd src-tauri && cargo fmt -- --check`, and `git diff --check`.
- Separately verify that the configured Rustls client can fetch the official page. A deterministic fixture test is still required because the page contents change and may have no active outage during a later run. Android JVM tests do not validate a physical widget/device run.

## Approved decisions and implementation record

- Approved source ID: `torunskie_wodociagi`.
- Applicability covers Toruń, Lubicz, Łysomice, Obrowo, Wielka Nieszawka, and Zławieś Wielka. Ciechocinek and Inowrocław remain excluded unless this bulletin publishes their resident-facing interruptions.
- Date-only failures keep their published date and no fabricated end. Sewer-only notices and monthly investment projects are excluded.
- The Rust provider, central house-number matching, frontend, Android provider and aggregate widgets, daemon source label, add-on allowlist, and bilingual documentation were implemented in this worktree.

Validation completed:

- `npm run test:frontend`: 58 tests passed.
- `npm run test:backend`: 200 passed, 8 ignored.
- `npm run test:android`: Android JVM unit tests succeeded.
- `npm run rust:check`: passed.
- `cargo check --bin awaria-daemon`: passed.
- The live source check fetched and parsed the bulletin using Awaria's configured Rustls client.
- `git diff --check` and a file-specific `rustfmt --check` for the new provider module passed. Repository-wide `cargo fmt -- --check` reports pre-existing formatting drift in other Rust files.
- A physical Android device/widget run was not performed.
