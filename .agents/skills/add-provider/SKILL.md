---
name: add-provider
description: Research a Polish utility provider and prepare an implementation plan for adding its outage alerts to Awaria. Use when the user asks to find, evaluate, plan, or integrate a new outage provider.
---

# Add an Awaria Provider

## Workflow

Use this skill when evaluating or adding a utility outage provider. Start by inspecting the current repository and the closest provider implementations; paths and integration points in this guide are landmarks, not a substitute for checking current code.

### 1. Research the provider

- Identify the provider's official outage page and coverage area. Prefer official sources and record URLs and the date checked.
- Look for a documented or observable REST/GraphQL API, JSON feed, or RSS feed before considering HTML parsing. Use browser/network inspection capabilities available in the current session; do not assume a particular MCP server or browser tool is installed.
- For server-rendered pages without a machine-readable source, inspect representative pages and, if useful, archived outage pages. Treat publication dates separately from outage start and end times.
- Determine how notices represent cities, streets, house numbers, start/end dates, causes, and planned outages versus failures. Record Polish date and time formats, timezone assumptions, missing-value behavior, and pagination or retention limits.
- Distinguish facts verified from live data, archived examples, and assumptions. Do not claim a live parser is validated if no active notice was available.

### 2. Prepare the implementation plan

Before editing application source, create or update a root-level `implementation_plan.md`. Make the plan specific to the provider and include evidence, affected files, implementation choices, test coverage, and unresolved questions. Ask the user to review and approve that plan before changing provider source code.

Inspect current conventions and cover applicable integration points:

- **Rust backend (`src-tauri/src/`)**: add a module implementing `AlertProvider`; update the source enum, provider registration, coverage/voivodeship mapping, alert normalization, and any shared classification or formatting logic only where current code requires it. Reuse existing HTTP, parsing, date, address, and logging utilities. Preserve upstream JSON field names with Serde where needed.
- **Address matching and settings**: populate normalized city and street data and set locality metadata consistently so shared matching can associate an alert with a configured address. Keep house-number enforcement in `Settings.filter_by_house_no`; do not unconditionally filter house numbers in a provider. Follow existing TERYT and city-matching patterns where applicable.
- **Frontend (`public/`)**: update `SOURCES`, region matching, translations, and display behavior only as required by the provider's coverage. Keep `SOURCES` grouped by utility category and sort providers alphabetically by ID within each category. Check current routing and matching behavior before adding special cases. Reuse generic category styling; do not add provider-specific colors. Preserve the standard alert-card information: provider, date range, city, incident type, and useful location/cause details.
- **Android (`src-tauri/gen/android/`)**: if provider-specific or aggregate widgets expose this source, update the relevant provider widget, shared widget source/layout mapping, configuration selection, receiver declaration, aggregation lists, refresh worker, and localized resource strings. Follow current widget base classes and reuse `@xml/widget_single_info`; do not create a provider-specific widget info XML. Keep widget alerts restricted to locally matching outages (`is_local == Some(true)`). Do not add widget work if current product architecture does not expose this provider through widgets; explain the scope decision in the plan.
- **Daemon/Home Assistant**: inspect `src-tauri/src/bin/awaria_daemon.rs` and the add-on integration. Update source mappings or documentation only if the new provider should be available there.
- **Docs and translations**: update affected Polish and English translations, Android values where applicable, README/spec/docs, and provider lists. Follow the repository instruction to update relevant Markdown documentation for feature work.
- **Tests and checks**: propose focused parser/normalization/matching tests, including realistic fixtures for date/time parsing, multiple streets or notices, empty results, and house-number setting behavior. Identify relevant frontend, Rust, Android, or daemon checks from the current `package.json`; distinguish automated checks from live-provider or device validation.

Keep the implementation small and consistent with a recent provider that has a similar utility type, source format, and coverage area. Avoid adding new shared abstractions unless the plan explains why existing ones do not fit.

### 3. Review gate

- Present the completed `implementation_plan.md` and ask the user to approve it before implementation.
- Do not modify application source until the user approves the plan. Research, plan edits, and read-only repository inspection can continue while waiting.
- Do not stage, commit, or push unless the user explicitly asks.

## Plan contents

The plan should include:

1. Provider name, utility type, coverage, official source URLs, and research date.
2. Endpoint/page behavior, representative payload or markup, data-field mapping, refresh/retention behavior, and known edge cases.
3. Proposed file-by-file changes across applicable desktop, Android, daemon, localization, documentation, and test surfaces.
4. Address normalization and locality strategy, including how house-number settings remain authoritative.
5. Focused validation commands and any live, device, or network checks that cannot be covered automatically.
6. Open questions or assumptions that need review.
