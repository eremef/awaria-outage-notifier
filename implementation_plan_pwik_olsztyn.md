# Implementation Plan: PWiK Olsztyn Water Outage Alerts

**Status:** Approved; implementation completed on 2026-10-21.
**Research checked:** 2026-10-21.

## Provider, coverage, and official sources

- **Provider:** Przedsiębiorstwo Wodociągów i Kanalizacji Sp. z o.o. w Olsztynie; proposed company-level ID `pwik_olsztyn`.
- **Utility:** Drinking-water supply interruptions. Do not publish sewer maintenance, general water-quality news, or a `Brak awarii` status as outages.
- **Official emergency category:** <https://pwik.olsztyn.pl/mapa-awarii/awarie/>; **RSS:** <https://pwik.olsztyn.pl/mapa-awarii/awarie/feed/>.
- **Official planned category:** <https://pwik.olsztyn.pl/mapa-awarii/planowane-przerwy-w-dostawie/>; **RSS:** <https://pwik.olsztyn.pl/mapa-awarii/planowane-przerwy-w-dostawie/feed/>.
- **Observed notice:** <https://pwik.olsztyn.pl/mapa-awarii/prace-modernizacyjne-a-sieci-wodociagowej-dnia-21-10-2026r-przy-al-pilsudskiego-89/>.
- **Coverage decision:** Start with Olsztyn and explicitly named surrounding localities in a notice. The live example includes Ostrzeszewo. The company's [2023 report](https://bip.pwik.olsztyn.pl/wp-content/uploads/2024/04/Sprawozdanie-zarzadu-2023.pdf) says it supplied water to Nikielkowo, Jonkowo, Ostrzeszewo, Zalbki, Myki, Wadąg, Gietrzwałd, and Bartąg as of 2023-12-31. That is historical evidence, not a confirmed current, street-level service map. Use an explicit locality predicate for those names, with TERYT/voivodeship disambiguation, and require notice-locality matching. Do not interpret the report's much longer sewer-service list as water coverage. Recheck the scope if a newer service list is found during implementation.

## Source behavior and field mapping

The two RSS URLs returned HTTP 200 and `application/rss+xml` on 2026-10-21. The emergency feed contained two items: a planned-work notice and `Brak awarii.`. The planned feed had no items, and its HTML category showed an empty-state message. The public WordPress REST posts endpoint returned HTTP 401, so RSS is the primary source. No browser-only fetch is required.

The current item has a `title`, `link`, `guid` (`?p=22889`), `pubDate` (`Tue, 20 Oct 2026 13:21:11 +0000`), truncated `description`, and full HTML in `content:encoded`. Its content states work on **21.10.2026, 09:00–15:00**, affecting **Al. Piłsudskiego 85, 87, 89, 89B** and **Ostrzeszewo 14, 14B**. The text describes planned work although the item is in the emergency category. `pubDate` is publication metadata, not the outage start. The `Brak awarii.` item has an empty body and must be ignored without hiding other items.

| Feed field or content | Proposed Awaria mapping |
| --- | --- |
| `guid`/`link` | Stable notice identity and source URL; deduplicate an item present in both category feeds. |
| `content:encoded` | Full plain-text message with paragraph/list boundaries preserved; use `description` only if it is complete, never its `[...]` truncation as the whole message. |
| Explicit date and clock range | `starts_at`/`ends_at` in `Europe/Warsaw`; support `dd.mm.yyyy`, optional `r.`, `HH:mm`, dash variants, and overnight ranges. |
| Locality and affected-address lines | Separate Olsztyn avenue addresses from Ostrzeszewo locality/house numbers. Normalize avenue/street prefixes and Polish diacritics for matching while retaining the original detail in the message. |
| Planned/emergency wording | Classify from notice text, not RSS category. Skip status-only posts, repairs without actual water interruption, and unrelated water-quality notices. |
| Missing end or start | Preserve unknown time; do not derive it from `pubDate` or treat publication as proof of an active outage. |

The available live notice validates the planned-work format only. An actual emergency notice, the retention window, pagination behavior, and missing-time semantics have not been observed. The initial fetch should use the bounded current feed, and implementation should inspect its item count and WordPress RSS pagination before choosing whether to fetch more pages. Expire old notices using explicit outage end times; when no end is supplied, apply a conservative, documented rule rather than leaving a stale alert indefinitely.

## Proposed changes by file

### Rust backend

- Add `src-tauri/src/pwik_olsztyn.rs` implementing `AlertProvider`. Fetch both RSS categories with Awaria's existing HTTP client; reuse `quick-xml`, `scraper`, date, address, and logging conventions from `mpwik_rzeszow.rs` and `torunskie_wodociagi.rs`. Parse full HTML and HTML entities, ignore status-only entries, classify each notice by wording, keep a stable source-based hash, and deduplicate across feeds.
- Add `AlertSource::PwikOlsztyn`, serialized `pwik_olsztyn`, water classification, and a precisely scoped Olsztyn-area predicate in `src-tauri/src/api_logic.rs`. Distinguish the city of Olsztyn in Warmińsko-Mazurskie from the Olsztyn locality already present in PWiK Częstochowa's coverage. Use TERYT identifiers where available.
- Declare/register the provider in `src-tauri/src/lib.rs` and inspect its special address-matching path. Produce one normalized alert per affected locality/address group, or use the existing multi-address representation if it accurately preserves both groups. Ensure an Ostrzeszewo address cannot match an Olsztyn street merely because both occur in one notice.
- Keep house-number enforcement in central `Settings.filter_by_house_no`. Populate numbers for the observed notice, but do not enforce them unconditionally in the provider. Preserve the whole source message for local and nonlocal alerts; keep nonlocal `location` to locality only.

### Frontend, daemon, and Home Assistant

- Add `pwik_olsztyn` to the alphabetically sorted water `SOURCES` group and an Olsztyn-area region matcher in `public/script.js`. Align its locality and street matching with Rust, including the two different Olsztyns; use existing water styling and alert-card fields. Add Polish/English labels in `public/i18n.js` and provider metadata in `public/awaria-card.js` if required by its current source lookup.
- Add the source to the water category mapping in `src-tauri/src/bin/awaria_daemon.rs`; inspect daemon provider selection. RSS can use the normal daemon network path.
- Add the source to `awaria_outage_monitor/config.yaml` and the provider lists in `awaria_outage_monitor/README.md` and `awaria_outage_monitor/README.pl.md`.

### Android and documentation

- Follow the existing PWiK/PEWIK single-source widget pattern: add `PwikOlsztynWidgetProvider.kt` with `@xml/widget_single_info`; wire manifest receiver, `BaseWidgetProvider.kt` name/type mapping, `WidgetConfigActivity.kt` selection, `WidgetUpdateWorker.kt` refresh, and `AllWidgetProvider.kt`/`TriWidgetProvider.kt` water source lists. Keep widget alerts restricted to `is_local == Some(true)` through the existing widget path. Add `provider_*` and `widget_label_*` strings in `values/strings.xml` and `values-en/strings.xml`.
- Update `README.md` and `README.pl.md` provider lists and any other affected Markdown documentation/specification found during implementation. This checkout has no root `docs/` directory at plan time.

## Validation results (2026-10-21)

- No new test code was authored. The supplied repository instructions require ARTEMIS exploration before authoring any test code, and the ARTEMIS MCP tools and CLI were unavailable in this session.
- Existing checks passed: `npm run test:frontend` (68 tests), `npm run test:backend` (234 passed, 8 ignored), `npm run test:android` (Gradle JVM unit task), `cargo check --lib --locked`, and `cargo check --bin awaria-daemon --locked`.
- The complete `cargo fmt --check` reports pre-existing formatting drift across unrelated Rust files; the new provider module was formatted directly. `git diff --check` passed.
- The official emergency RSS was fetched manually (HTTP 200). It contained the 21 October mixed Olsztyn/Ostrzeszewo notice and a separate `Brak awarii.` item. The notice lists Al. Piłsudskiego 85, 87, 89, 89B and Ostrzeszewo 14, 14B, and uses the phrase “dystrybucja wody będzie wstrzymana.” The notice's explicit 15:00 end had passed at the time of inspection, so it was correctly stale for a live parser run.
- A direct daemon invocation could not reach the provider: the existing daemon panics during startup at its root `nest_service` route before starting the fetch cycle. Physical Android widget behavior and future emergency notice formats remain unverified.

## Review questions and assumptions

1. This plan includes both emergency and planned interruptions, although the supplied URL names the emergency category. This matches the site's combined "Awarie i przerwy w dostawie wody" section and the planned notice presently filed under emergencies.
2. The 2023 water-service locality list is provisional. An explicit notice locality remains authoritative for local matching; no blanket match for every address in surrounding communes is proposed.
3. The provider ID remains `pwik_olsztyn` even if notices later cover more surrounding towns, because it identifies the company rather than a city-only feed.

**Review gate:** The user authorized delegation of implementation to GPT-6 Luna after receiving this plan on 2026-10-21.
