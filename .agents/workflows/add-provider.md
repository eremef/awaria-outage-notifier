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
     - Add the provider to the `SOURCES` array (name, category, id, i18n keys). *(Note: The explicit backend matching array `matchesAddress()` and `enabledSources` in `renderAlerts()` are dynamically populated from `SOURCES`, so adding it to `SOURCES` covers these automatically!)*
     - Create the necessary city-specific helpers (e.g., `isCzestochowa(addr)` and `const hasAnyCzestochowa = addresses.some(isCzestochowa)`) and update the `otherLists` conditional blocks inside `renderAlerts()` to properly group outages for locations outside the user's primary addresses.
   - `public/i18n.js`: Add translation strings for the provider name and abbreviations.
   - `public/style.css` (or `index.css`): 
     - Add CSS variables for the provider's brand color to **all themes** defined in the file (e.g., `:root`, `[data-theme="dark"]`, `emerald`, `ocean`, `nord`, `dracula`, `sepia`, `latte`), ensuring it is readable and maintains good contrast against each theme's background color.
     - **Crucial Component Rules:** Remember to add specific class rules for the provider's card so the brand color actually applies to the UI elements:
       1. `.card.source-[new_provider] .outage-type { color: var(--[new_provider]-color); }`
       2. `.card.source-[new_provider] { border-top: 4px solid var(--[new_provider]-color); }`
       3. Also append `:not(.source-[new_provider])` to the default fallback border selector (`.card:not(...):not(.source-[new_provider]) { border-top: 4px solid #7c7c7c; }`).
       4. `.collapsible.source-[new_provider] .section-label.other { color: var(--[new_provider]-color); }` (for the Settings UI).
   - Use the standard outage card layout:
     `{utility_icon} {provider_name}`
     `{start_date} - {end_date}`
     `Miejscowość: {address_city}`
     `{incident_type} - {description_with_streets_etc}`

3. **Android Widgets (`src-tauri/gen/android/`)**:
   - **Plumbing**:
     - `BaseWidgetProvider.kt`: Add the `sourceKey` to `getSourceName()` so it correctly translates the label.
     - `WidgetConfigActivity.kt`: Add the new `*WidgetProvider` class to the `getProviderForWidget` `when` statement so users can select an address for it.
     - `AndroidManifest.xml`: Declare a new `<receiver>` for the new widget provider class, pointing to its layout/info.
     - `strings.xml` (in both `values` and `values-en`): Define `@string/widget_label_provider` and `@string/provider_name`.
     - `colors.xml` (both `values` and `values-night`): Define the provider's brand color to fit the proper theme.
   - **Widget Classes & Layouts**:
     - Create a dedicated provider widget Kotlin class (e.g., `NewProviderWidgetProvider.kt`) extending `BaseWidgetProvider`.
     - Create its info XML: `res/xml/widget_provider_info.xml` pointing to `@layout/widget_outage`.
     - `AllWidgetProvider.kt` & `TriWidgetProvider.kt`: Add the new provider source string to their data aggregation lists (e.g., `waterSources`).
   - **Workers**:
     - `WidgetUpdateWorker.kt`: Add the new provider to the background worker refresh logic.
   - **Crucial**: Outages displayed on widgets MUST be filtered by `is_local == Some(true)` so users only see outages for their configured streets.

## Step 3: Plan Review

- Request feedback from the user on the `implementation_plan.md` (set `request_feedback = true`).
- DO NOT start modifying source code until the user approves the plan.
