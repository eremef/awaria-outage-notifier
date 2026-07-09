---
description: Research and plan integration of a new utility provider into Awaria.
---

# Add New Provider Workflow

You are tasked with researching a new utility provider and preparing a comprehensive implementation plan to integrate it into the Awaria application.

Follow these steps systematically:

## Step 1: Research and API Discovery

- Determine the provider's website.
- Prioritize finding direct APIs. Use the `chrome-devtools-mcp` tools (e.g., `list_network_requests`, `navigate_page`) to monitor network traffic while interacting with the provider's outage map or list.
- Look for REST APIs, GraphQL endpoints, JSON files, or RSS feeds. Avoid complex HTML scraping if an API exists.
- If the webpage is static/not dynamically generated and there are no APIs, RSS, or JSON feeds, eventually use `web.archive.org` to investigate previous outages on historic links (to help build the parser).
- Data requirements to locate:
  - **Address**: cities, streets, house numbers where the outage happens.
  - **Time**: start date, end date (if provided). *Note: Watch out for Polish date formats (e.g., dots or "godz." prefixes), as they require custom parsing in Rust.*
  - **Description**: cause, or detailed location info.
  - **Incident Type**: planned outage ("planowana") vs. failure ("awaria").

## Step 2: Implementation Planning

Once the data source is identified, create an `implementation_plan.md` artifact. The plan must outline how to integrate the provider across the following components consistently:

- **Codebase Consistency**: Ensure that all new Rust, Kotlin, and frontend code closely mirrors the style, abstractions, error handling, and naming conventions of existing providers. Reuse existing utility functions and layout templates whenever possible.
- Do not forget about translations, i18n, Android values, etc.

1. **Rust Backend (`src-tauri/`)**:
   - `src-tauri/src/api_logic.rs`: Add the provider to the `AlertSource` enum, implement its `service_voivodeships`, add formatting logic (e.g., `awaria wody`), and create a city helper (`is_cityname`).
   - `src-tauri/src/lib.rs`: Export the new module (`pub mod new_provider`) and register it in the `PROVIDERS` list initialization (`Box::new(new_provider::NewProvider)`).
   - Create the new fetcher module (e.g., `new_provider.rs`) conforming to `AlertProvider`. Ensure the scraping logic correctly populates `item.city` and `item.streets` so the local regex matcher works (setting `is_local = Some(true)`).

2. **Frontend (`public/`)**:
   - `public/script.js`:
     - Add the provider to the `SOURCES` array (name, category, id, i18n keys). If it is a strictly local provider, also include a `region` key matching its coverage area (e.g., `region: 'czestochowa'`). *(Note: The explicit backend matching array `matchesAddress()` and `enabledSources` in `renderAlerts()` are dynamically populated from `SOURCES`, so adding it to `SOURCES` covers these automatically!)*
     - **Important**: In the `SOURCES` array, utilities must be grouped by category (e.g., Power, Gas, Heating, Water), and within each category, providers MUST be sorted in alphabetical order (by `id`) as it's also the order of display.
     - If the provider operates in a brand new region not yet defined, create the necessary city-specific matcher function (e.g., `czestochowa: (addr) => { ... }`) inside the `REGION_MATCHERS` object at the top of the file.
     - You do NOT need to manually route the provider inside `renderAlerts()` anymore, as it automatically groups outages based on the `region` attribute defined in `SOURCES`.
   - `public/i18n.js`: Add translation strings for the provider name and abbreviations.
   - `public/style.css` (or `index.css`):
     - We resigned from provider-specific colors. There is no need to add new CSS color variables or specific `.card.source-[new_provider]` classes. The card will use the default styling or generic utility category classes.
   - Use the standard outage card layout:
     `{utility_icon} {provider_name}`
     `{start_date} - {end_date}`
     `Miejscowość: {address_city}`
     `{incident_type} - {description_with_streets_etc}`

3. **Android Widgets (`src-tauri/gen/android/`)**:
   - **Plumbing**:
     - `BaseWidgetProvider.kt`: 
       - Add the `sourceKey` to `getSourceName()` so it correctly translates the label.
       - Add the new provider's `sourceKey` to the appropriate layout group (power, heat, gas, or water) in the `when (sourceKey)` statements for both `layoutId` and `layoutSmallId`. This is critical for dynamically inflating the correct widget layout.
     - `WidgetConfigActivity.kt`: Add the new `*WidgetProvider` class to the `getProviderForWidget` `when` statement so users can select an address for it.
     - `AndroidManifest.xml`: Declare a new `<receiver>` for the new widget provider class, pointing to the shared info XML: `@xml/widget_single_info`. **CRITICAL**: Do NOT create a custom `[provider]_widget_info.xml` file in `res/xml/`. Always reuse `@xml/widget_single_info` for new providers.
     - `strings.xml` (in both `values` and `values-en`): Define the widget and provider names using STRICTLY the following naming convention to match the rest of the application:
       `<string name="provider_[id]">[Short Provider Name]</string>`
       `<string name="widget_label_[id]">[Full Provider Name]</string>`
       (e.g., `<string name="provider_sec">SEC</string>` and `<string name="widget_label_sec">SEC Szczecin</string>`). Do NOT use prefixes like `widget_name_` or `source_` for Android resources. Do NOT prefix the value with `Awaria - ` or add utility type suffixes like `(Woda)`. Just the provider name.
   - **Widget Classes & Layouts**:
     - Create a dedicated provider widget Kotlin class (e.g., `NewProviderWidgetProvider.kt`) extending `BaseWidgetProvider`. It must correctly override `refreshAction`, `primaryColorRes`, `iconResId`, `labelKey`, and `sourceKey`. Do NOT manually override `updateAppWidget` or manually inflate layouts—`BaseWidgetProvider` handles this using the shared layouts.
     - `AllWidgetProvider.kt` & `TriWidgetProvider.kt`: Add the new provider source string to their data aggregation lists (e.g., `waterSources`).
   - **Workers**:
     - `WidgetUpdateWorker.kt`: Add the new provider to the background worker refresh logic.
   - **Crucial**: Outages displayed on widgets MUST be filtered by `is_local == Some(true)` so users only see outages for their configured streets.

## Step 3: Plan Review

- Request feedback from the user on the `implementation_plan.md` (set `request_feedback = true`).
- DO NOT start modifying source code until the user approves the plan.
