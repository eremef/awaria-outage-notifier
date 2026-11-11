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
- Distinguish facts verified from live data, archived examples, synthetic examples, and assumptions. Record the source URL and check date for representative examples, and note which formats have actually been observed. Do not claim a live parser is validated if no active notice was available or the observed examples do not cover the claimed behavior.
- Treat the provider page or feed as a changing source: determine how notices are separated, updated, removed, and paginated where evidence allows. Distinguish a valid empty result from a failed request, malformed response, or unrecognized page structure.

### 2. Prepare the implementation plan

Before editing application source, create or update dedicated implementation plan in '/plans' subfolder. Make the plan specific to the provider and include evidence, affected files, implementation choices, test coverage, and unresolved questions. Ask the user to review and approve that plan before changing provider source code.

Inspect current conventions and cover applicable integration points:

- **Rust backend (`src-tauri/src/`)**: add a module implementing `AlertProvider`; update the source enum, provider registration, coverage/voivodeship mapping, alert normalization, and any shared classification or formatting logic only where current code requires it. Reuse existing HTTP, parsing, date, address, and logging utilities. Preserve upstream JSON field names with Serde where needed.
- **Address matching and settings**: populate normalized city and street data and set locality metadata consistently so shared matching can associate an alert with a configured address. Keep house-number enforcement in `Settings.filter_by_house_no`; do not unconditionally filter house numbers in a provider. Follow existing TERYT and city-matching patterns where applicable.
- **Notice scope and location parsing**: identify which text names the affected locality and addresses. Distinguish those from repair-site addresses, route references, headings, and cross-street boundaries. Define when a notice is city-wide versus address-specific, and preserve the association between each street and its house numbers or ranges. Do not broaden an alert's locality based only on the provider's overall service area.
- **Frontend (`public/`)**: update `SOURCES`, region matching, translations, and display behavior only as required by the provider's coverage. Keep `SOURCES` grouped by utility category and sort providers alphabetically by ID within each category. Check current routing and matching behavior before adding special cases. Reuse generic category styling; do not add provider-specific colors. Preserve the standard alert-card information: provider, date range, city, incident type, and useful location/cause details.
- **Android (`src-tauri/gen/android/`)**: new single-provider widget choices use `SingleProviderCatalog.kt` and the generic `SingleWidgetProvider`; add its stable source ID, localized display-name resource, utility type, exact legacy-component mapping (for receiver parity), aliases, and location terms there. Do not add a new provider-specific picker receiver or widget info XML. Existing provider-specific receiver classes and `@xml/widget_single_info` registrations are legacy compatibility components: keep them registered and keep the worker updating them, because users may already have placed those widgets. The old component list is separate from the generic catalog's user-facing picker. Update aggregate widget source lists only when the provider is meant to appear in those totals, audit `WidgetUpdateWorker`, and add catalog/manifest parity coverage. Preserve PSG's dedicated fetch path and local outage filtering (`is_local == Some(true)`).
- **Daemon/Home Assistant**: inspect `src-tauri/src/bin/awaria_daemon.rs` and the add-on integration. Decide add-on eligibility separately from desktop registration by checking whether the provider works headlessly and whether its fetch path has the required configuration and dependencies. If eligible, update catalog metadata and generated allowlist/config as applicable, then run their validation; document exclusions and their reason.
- **Docs and translations**: update affected Polish and English translations, Android values where applicable, README/spec/docs, and provider lists. Follow the repository instruction to update relevant Markdown documentation for feature work.
- **Tests and checks**: propose focused parser/normalization/matching tests using fixtures labeled by provenance (official live, official archived, or synthetic). Cover date/time parsing, multiple streets or notices, valid empty results versus fetch/parse/structure errors, and house-number setting behavior. Where supported by source evidence, also cover city-wide and address-specific scope, affected addresses versus repair locations, cross-street boundaries, number ranges or parity, same-named streets in different localities, pagination and update/removal behavior, duplicate or changed notices, and stale, expired, open-ended, or year-boundary dates. Identify relevant frontend, Rust, Android, or daemon checks from the current `package.json`; distinguish automated checks from live-provider or device validation and state what sparse live evidence leaves unverified.

Keep the implementation small and consistent with a recent provider that has a similar utility type, source format, and coverage area. Avoid adding new shared abstractions unless the plan explains why existing ones do not fit.

### 3. Review gate

- Present the completed implementation plan and ask the user to approve it before implementation.
- Do not modify application source until the user approves the plan. Research, plan edits, and read-only repository inspection can continue while waiting.
- Do not stage, commit, or push unless the user explicitly asks.

## Plan contents

The plan should include:

1. Provider name, utility type, coverage, official source URLs, and research date.
2. Endpoint/page behavior, representative payload or markup with provenance, data-field mapping, notice identity and update/removal behavior, refresh/retention and pagination behavior, error-versus-empty semantics, and known edge cases.
3. Proposed file-by-file changes across applicable desktop, Android, daemon, localization, documentation, and test surfaces.
4. Address normalization and locality strategy, including how house-number settings remain authoritative.
5. Focused validation commands and any live, device, or network checks that cannot be covered automatically; identify which parser and matching behaviors lack representative source evidence.
6. Open questions or assumptions that need review.
