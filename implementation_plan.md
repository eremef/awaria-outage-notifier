# Implementation Plan: MPEC Rzeszów

## Scope and research

Add MPEC Rzeszów heat-supply interruption notices as the separate provider ID `mpec_rzeszow`. The notice archive and MPEC's site identify Rzeszów as the service area; scope address matching to Rzeszów (TERYT city ID `974133`).

Research checked on 18 October 2026:

- [Official MPEC Rzeszów communications page](https://mpecrzeszow.pl/komunikaty)
- [Official RSS feed linked by the communications page](https://mpecrzeszow.pl/komunikaty?format=feed&type=rss)
- [Official MPEC Rzeszów homepage](https://mpecrzeszow.pl/)

The Joomla communications page links to both RSS and Atom feeds. The RSS endpoint was opened and inspected in a browser. It returned 100 recent `<item>` entries, with the newest published 16 October 2026 and the oldest 26 May 2026. The HTML archive showed four pages, using `?start=100`, `?start=200`, and `?start=300`; the feed is the preferred source and avoids fetching the archive pages individually.

An RSS item has a title, permalink, publication date, and an HTML `description` containing the notice. Publication date is separate from the outage schedule and must not be used as the outage date. Examples include an Oct. 15 post announcing Oct. 23, an Oct. 13 post with separate schedules and addresses for Oct. 16 and Oct. 19, and an Oct. 8 update describing a same-day extension. Posts may cover ordinary heating (`c.o.`), domestic hot-water preparation (`c.w.u.`), or both depending on whether the heating season has begun; preserve this context in the alert text.

Schedule text uses Polish dates (`23.10.2026 r.`) and times in forms such as `od godz. 8:00 do godz. 20:00` and `8<sup>00</sup>`. The RSS HTML must be converted to text while normalizing superscript minutes (`8<sup>00</sup>` to `8:00`) before parsing. Notices may group many streets/building numbers or list separate date/time/address blocks in a single post. Some end times are intentionally vague (`do późnych godzin popołudniowych`, `do czasu usunięcia awarii`); keep those ends unknown. `24:00` should be normalized as midnight at the start of the following day. An update notice that says the failure is fixed and supply has resumed should not be emitted as a new interruption.

## Proposed changes

### Rust app

- Add `src-tauri/src/mpec_rzeszow.rs` implementing `AlertProvider`; fetch the verified RSS feed with the existing `reqwest` client and parse XML with existing `quick-xml` support.
- Parse each RSS item's title and HTML description into outage schedule segment(s). Keep `pubDate` and permalink separate from the outage schedule. Normalize HTML block boundaries and superscript times. Split a notice that assigns different dates/times to different buildings into separate alerts so address matching does not attach the wrong schedule to a building.
- Preserve each segment's street/building text, cause, and whether it concerns central heating or hot-water preparation in `message`/`location`. Classify explicit failures separately in the description; exclude restoration/update messages that say supply has resumed.
- Add `AlertSource::MpecRzeszow` with serialized ID `mpec_rzeszow`, display name, Rzeszów service matching, and heat notification classification in `src-tauri/src/api_logic.rs`. Reuse the existing city ID or add the smallest Rzeszów city matcher. Register the provider in `src-tauri/src/lib.rs`.
- Match configured Rzeszów addresses by normalized street text and retain house numbers in the full notice for the central `Settings.filter_by_house_no` path. Do not filter house numbers inside the provider. Set `is_local` and `address_index` only for matching Rzeszów addresses.
- Use only the current feed (100 entries in the sample) per refresh. Ignore expired notices under existing app time handling, and avoid walking all four historical archive pages unless feed retention is later shown to omit active notices.

### Frontend and translations

- Add `mpec_rzeszow` to the heating group in `public/script.js`, sorted by provider ID, with Rzeszów-only region matching.
- Add `source_mpec_rzeszow_name` and `source_mpec_rzeszow_short` in Polish and English in `public/i18n.js`.
- Reuse generic heating visuals and preserve standard alert-card fields plus useful building/cause and c.o./c.w.u. details.

### Android widgets

The app currently exposes individual heat widgets and aggregate heat counts, so include the provider in that pattern:

- Add `MpecRzeszowWidgetProvider.kt` and a receiver in `AndroidManifest.xml`, using `@xml/widget_single_info`.
- Update provider-name and heat-category mappings in `BaseWidgetProvider.kt`, class selection in `WidgetConfigActivity.kt`, and refresh handling in `WidgetUpdateWorker.kt`.
- Add `mpec_rzeszow` to heat aggregation/enabled-source lists in `TriWidgetProvider.kt` and `AllWidgetProvider.kt`.
- Add provider and widget labels to both `values/strings.xml` and `values-en/strings.xml`.
- Preserve the widget rule that only locally matching alerts (`is_local == Some(true)`) are displayed.

### Daemon, add-on, docs, and tests

- The daemon gets registered providers through `get_providers()`; add the new source to its exhaustive heat summary match if required. Add `mpec_rzeszow` to `awaria_outage_monitor/config.yaml`'s accepted source list.
- Update `README.md` and `README.pl.md` provider inventories and any other provider list/spec found during implementation.
- Add Rust RSS parser and matching tests using representative fixture items. Cover: normal same-day schedule, `24:00`, multiple independent building schedules, multiple streets and house-number formats, superscript time markup, failure and planned-work text, hot-water versus central-heating details, restoration/update exclusion, missing/invalid dates or vague end times, empty feed, duplicate items, Rzeszów locality matching, and the central house-number setting behavior.

## Validation

After implementation, run the focused MPWiK tests and `npm run test:backend`, `npm run test:frontend`, and `npm run test:android`; run `npm run rust:check` and a daemon compile check if its source changes. Verify the live RSS with the app's Rust HTTP client separately from saved-fixture tests. Report the feed's current date range and whether active notices were present. JVM widget tests do not replace physical-device validation. Do not stage, commit, or push.

## Remaining limitations

1. The RSS feed retains 100 items across about five months in the checked snapshot. Monitor this if unusually long-running outages could remain active after falling outside the feed.
2. Vague end-time wording such as “until late afternoon” and “until the failure is fixed” remains unknown by design; the app can still show the alert without an invented end time.

## Implementation status

Implemented after approval. The live RSS request using Awaria's Rust HTTP client succeeded and parsed notices. Validation passed: `npm run test:backend` (142 passed, 2 ignored), `npm run test:frontend` (40 passed), `npm run test:android`, `npm run rust:check`, focused MPEC parser tests (5 passed), and the live MPEC feed test. `git diff --check` passed. No physical Android-device validation was performed. The previous MPWiK Rzeszów implementation plan was preserved at `plans/implementation_plan_mpwik_rzeszow.md` while this root plan was active.

# Implementation Plan: WM Radom

## Scope and research

Add water-supply interruption notices from Wodociągi Miejskie w Radomiu Sp. z o.o. (WM Radom) as a separate provider, provisionally using the source ID `wm_radom`. Research checked on 18 October 2026.

Official sources:

- [WM Radom outage notices](https://www.woda.radom.pl/index.php/awaryjne-i-planowane-wylaczenia/)
- [WM Radom service area](https://www.woda.radom.pl/index.php/uslugi/)
- [Official 2026 drinking-water assessment for Radom](https://www.gov.pl/attachment/c5272827-05ad-43b6-a9a2-fa02d9f4f564)

WM Radom's service page names Radom and the surrounding municipalities of Wolanów, Jastrzębia, Zakrzew, Jedlnia-Letnisko, Skaryszew, Jedlińsk, and Kowala. The current TERYT data identifies Radom as SIMC `0972750` (app `city_id` 972750) and Cerekiew in Zakrzew as SIMC `0642253` (app `city_id` 642253). The official 2026 water assessment also mentions a partial supply area in Gózd, while WM Radom's service page does not; keep Gózd out of the initial supported-area list until this discrepancy is resolved. The provider page describes network service by municipality, not a complete list of served villages, so locality matching must use the actual place names in each notice where available.

The search-indexed copy of the outage page, last crawled in the prior week, exposed three representative notices: a water-main failure affecting Działkowa, Sierpowa, and Perzanowska on 6 October 2026; a failure on Potkanowska on 5 October; and planned network modernisation on Złota in Cerekiew on 6 October from 9:00 to 13:00. The TERYT street data confirms Działkowa, Sierpowa, and Potkanowska are Radom streets, and the Cerekiew example is in Zakrzew. All sampled notices had expired by the research date; no active notice was verified.

Direct requests to the outage page timed out during this research. Search did not surface a documented RSS/JSON endpoint, but the page's source and common CMS feed/API endpoints could not be inspected live, so the absence of a machine-readable feed is not confirmed. The extracted page presents multiple notice texts together, with no visible publication timestamps, archive limit, or pagination in the indexed copy. Exact HTML grouping and refresh retention remain to be confirmed before writing the parser. A live parser must not be described as validated based on these cached examples.

Observed notice fields are embedded in Polish prose rather than a structured payload: incident/cause (`awaria`, `modernizacja`), interruption date (`06. 10. 2026 r.`, `05-10-2026 r.`, or `06.10.2026r.`), optional time range (`9.00 – 13.00`), locality when supplied (`miejscowość Cerekiew`), and one or more streets. Some notices omit times and house numbers. These notice dates and times are the outage schedule; any CMS publication date discovered in the actual markup must remain separate. Missing hours or an unknown restoration time must remain unknown instead of being inferred. A date-only notice will be stored and shown as a date without a fabricated midnight time; such notices are considered current through the stated calendar day and dropped after that day to avoid retaining stale notices from the page.

## Proposed changes

### Rust app

- Add `src-tauri/src/wm_radom.rs` implementing `AlertProvider`. Prefer an official feed/API only if a live endpoint is verified; otherwise fetch the official page with the existing HTTP client and parse the confirmed notice containers with `scraper`.
- Normalize visible text and block boundaries before parsing. Emit one alert per independent notice; keep all streets from a shared schedule together. Parse the observed dotted and hyphenated Polish dates and dotted/colon time ranges. Distinguish explicit failures from planned work, and leave start/end times absent where the notice does not state them. Format date-only alerts without a time and omit an empty end of the displayed date range.
- Add `AlertSource::WmRadom`, display name and provider registration in `src-tauri/src/api_logic.rs` and `src-tauri/src/lib.rs`; include the source in water notification classification and `Display`/ID mappings.
- Add a shared service-area matcher based on Radom (SIMC `0972750`) and the municipalities explicitly listed by WM Radom. Use configured city/commune fields and TERYT IDs where available; parse an explicit notice locality and match it against configured locality before matching street text. For a notice with no locality, treat the observed Radom street notices as Radom-only. Do not include Gózd until its conflicting scope evidence is confirmed.
- Preserve full notice location, including any house numbers, and leave house-number enforcement in `Settings.filter_by_house_no`. Set `is_local` and `address_index` only when provider area, locality, and street matching support the configured address.

### Frontend and translations

- Add `wm_radom` to the water `SOURCES` group in `public/script.js`, keeping provider IDs alphabetized. Add Radom-area matching for the listed service municipalities and explicit Cerekiew-style village localities.
- Add Polish and English full and short names in `public/i18n.js`. Reuse the existing generic water card styling and retain the standard provider, date range, city/locality, incident type, streets, and cause details.

### Android widgets

The current app exposes individual water-provider widgets and aggregate water counts. Include WM Radom in that existing pattern:

- Add `WmRadomWidgetProvider.kt` and its receiver in `AndroidManifest.xml`, using `@xml/widget_single_info`.
- Update source-name/category mappings in `BaseWidgetProvider.kt`, provider selection in `WidgetConfigActivity.kt`, and widget refresh handling in `WidgetUpdateWorker.kt`.
- Add `wm_radom` to the water-source lists in `TriWidgetProvider.kt` and `AllWidgetProvider.kt`, plus Polish and English provider/widget labels in the Android string resources.
- Keep widget alerts restricted to locally matching outages (`is_local == Some(true)`).

### Daemon, add-on, docs, and tests

- Update the water-source summary match in `src-tauri/src/bin/awaria_daemon.rs` if required by its current exhaustive match, and add `wm_radom` to the accepted provider IDs in `awaria_outage_monitor/config.yaml`.
- Update `README.md`, `README.pl.md`, and any current provider inventory or specification found during implementation.
- Add fixture-based Rust parser and matching tests for each observed date format, dotted and colon times, planned work versus failures, multiple streets in one notice, explicit localities (including Cerekiew), missing times, malformed/empty page content, notice deduplication, local versus out-of-area matching, and the central house-number setting behavior.

## Validation

After implementation, run the focused WM Radom parser/matching tests, `npm run test:backend`, `npm run test:frontend`, `npm run test:android`, `npm run rust:check`, and a compile check for `awaria-daemon` if its source changes. Check `git diff --check`. Recheck the official page live and report whether an active notice was available. Network-based parser verification and physical Android-device validation remain separate from fixture and JVM test results.

## Open questions and assumptions

1. The live page's exact HTML, feed/API availability, and notice retention/pagination could not be verified because direct requests timed out. Confirm these before choosing the concrete parser selector and refresh strategy.
2. WM Radom's service-area page lists seven surrounding municipalities, while the official 2026 assessment mentions partial coverage in Gózd instead of Jastrzębia. This plan follows WM Radom's own page and excludes Gózd until confirmed.
3. The sampled page copy had only past notices as of 18 October 2026. Initial parser tests will need saved representative fixtures; a live active-notice comparison may not be available.

## Implementation status

Implemented after approval. Added the `wm_radom` provider with date-only notices, failure/planned-work classification, Radom and listed-municipality matching, and house-number filtering left to the shared settings path. Registered the provider in frontend settings, Rust, the daemon add-on's accepted source list, Android widgets, and Polish/English provider labels and documentation.

Validation passed: `npm run test:backend` (151 passed, 2 ignored), `npm run test:frontend` (42 passed), `npm run rust:check`, `npm run test:android` (`BUILD SUCCESSFUL`), and `git diff --check`. Direct access to the official outage page still timed out; the indexed copy available for review contained only past notices dated 5–6 October 2026, so no live parser comparison or active notice was verified. No physical Android-device validation was performed.
