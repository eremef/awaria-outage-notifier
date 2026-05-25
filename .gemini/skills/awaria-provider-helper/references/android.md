# Android Widget & Worker Plumbing

## 1. Widget Provider

1. Create `src-tauri/gen/android/app/src/main/java/xyz/eremef/awaria/NewProviderWidgetProvider.kt`.
2. Inherit from `BaseWidgetProvider`.
3. Declare in `AndroidManifest.xml` as a `<receiver>`.

## 2. Resource Mapping

- **`BaseWidgetProvider.kt`**: Map `sourceKey` to the i18n label in `getSourceName()`.
- **`WidgetConfigActivity.kt`**: Map the widget class to the provider key in `getProviderForWidget`.

## 3. UI Resources

- **`strings.xml`**: Define labels (Polish/English).
- **`colors.xml`**: Define the brand color (Light/Night).

## 4. Aggregation

- **`AllWidgetProvider.kt` / `TriWidgetProvider.kt`**: If this belongs to a category (e.g., Water), add its key to the relevant category list (e.g., `waterSources`).

## 5. Background Work

- **`WidgetUpdateWorker.kt`**: Ensure the provider is included in the refresh loop.

## 6. Crucial Verification

All widgets MUST filter alerts by `is_local == Some(true)`. Verify that the Rust fetcher correctly identifies local outages based on configured streets.
