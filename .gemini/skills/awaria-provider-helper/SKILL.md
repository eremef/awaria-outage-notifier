---
name: awaria-provider-helper
description: Workflow and patterns for integrating new utility providers (Power, Water, Gas, Heat) into the Awaria app. Use this when the user asks to add a new outage source, research a provider's API, or implement provider-specific logic across Rust, Frontend, and Android.
---

# Awaria Provider Helper

This skill guides you through the end-to-end process of adding a new utility provider to the Awaria application.

## Workflow Overview

1.  **Research & API Discovery**: Identify the data source (API/Scraper). See [references/discovery.md](references/discovery.md).
2.  **Implementation Planning**: Create an `implementation_plan.md` artifact. **Do not code until the plan is approved.**
3.  **Backend Implementation (Rust)**: Implement the `AlertProvider` trait. See [references/rust.md](references/rust.md).
4.  **Frontend Integration (JS/CSS)**: Add to `SOURCES`, implement UI grouping, and theme colors. See [references/frontend.md](references/frontend.md).
5.  **Android Plumbing (Kotlin)**: Add widgets and background worker support. See [references/android.md](references/android.md).
6.  **Validation**: Verify local matching (`is_local`) and notification formatting.

## Core Mandates

- **Theme Consistency**: Every new provider MUST have color variables defined for ALL themes in `style.css`.
- **Local Matching**: Ensure `item.city` and `item.streets` are populated in the Rust fetcher to enable street-level filtering on Android widgets.
- **Normalization**: Clean up Polish date strings (remove "godz.", dots to hyphens) before parsing.
- **Throttled Parallelism**: Use the project's existing async patterns to keep fetching efficient.

## Outage Card Template

Use this structure for UI consistency:
```text
{utility_icon} {provider_name}
{start_date} - {end_date}
Miejscowość: {address_city}
{incident_type} - {description_with_streets_etc}
```
