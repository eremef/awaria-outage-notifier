# Awaria Project Context

## Project Overview

**Name**: Awaria
**Description**: A cross-platform desktop and mobile application built with Tauri to notify users about Tauron power outages.
**Identifier**: `xyz.eremef.awaria`

## Architecture

- **Frontend**: Located in `public/`. Seens to use vanilla HTML/JS/CSS (based on `frontendDist` configuration).
- **Backend (Core)**: Rust-based Tauri backend located in `src-tauri/`.
- **Mobile**: Android and iOS support enabled via Tauri mobile.

## Key Configuration Files

- **`src-tauri/tauri.conf.json`**: Main Tauri configuration file.
- **`package.json`**: Node.js dependencies and scripts.
- **`src-tauri/Cargo.toml`**: Rust dependencies and workspace configuration.
- **`.github/workflows/`**: CI/CD pipelines (e.g., `release.yml`).

## Development Commands

- `npm run tauri dev`: Start desktop development server (usually on http://localhost:1430).
- `npm run android:dev`: Start Android development server.
- `npm run build`: Build web assets and desktop application.
- `npm run android:build`: Build Android application.

## User Rules

- When you bump version, update it in `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `package.json` using semver versioning, even when git tag is in other format, like 1.0.0b
- when bumping `src-tauri/tauri.conf.json` use only X.X.X format, without any additional suffixes

## Project Learnings

### JNI 0.22 Migration & Android Stabilization
- **JNI 0.22 API**: `JNIEnv` is now `Env`. `JavaVM` must be stored in a global static (e.g., `Mutex<Option<JavaVM>>`).
- **Global State**: Critical globals (`JAVA_VM`, `ANDROID_CONTEXT`, `PSG_FETCHER_CLASS`) must be initialized during `JNI_OnLoad` or the first bridge call. Always lock and check `Option` before use.
- **Thread Attachment**: Use the closure-based `vm.attach_current_thread(|env| { ... })` for background tasks.
- **`EnvUnowned::with_env`**: Requires the `native_env` variable to be declared `mut`.
- **Widget Filtering**: Counters MUST filter by `is_local == Some(true)` to avoid showing city-wide outages that don't affect the user's specific street.
- **Provider Scrapers (Kotlin)**: Fixed PSG count logic to include city-wide outages even when no street is configured.

### Background Monitoring & Notifications
- **Android WorkManager**: Uses `BackgroundMonitorWorker` and `WidgetUpdateWorker`. Ensure `ensure_verifier_initialized` is called in each worker's entry point.
- **Settings Sync**: Kotlin side uses `loadSettings(context)` which returns `Pair<Settings, String>` (object + raw JSON). The raw JSON is passed to Rust to avoid double serialization.
- **Rust MonitorEngine**: The `MonitorEngine::process_alerts` handles deduplication and notification triggers. It relies on `ProviderCache` to avoid redundant network calls.

### Browser Automation & Scraping
- **Dynamic Dropdowns**: For sites with autocomplete (like Tauron), `type_text` must be followed by an explicit click on the suggested item `uid`.
- **API Discovery**: Use `list_network_requests` to find direct `/api/` endpoints instead of perfecting UI scrapers. Direct HTTP requests (via `reqwest`) are more robust.
- **CMP/Cookie Popups**: Often can be ignored for pixel-based input targeting, but "Accept" buttons should be clicked if they overlay target elements.

## Common Development Workflows

- **Android Release Build**: `npx tauri android build -- --target aarch64 --apk`
- **Log Monitoring**: `adb logcat | grep awaria`
- **Knowledge Persistence**: The project uses MemPalace for memory storage. Ensure git hooks are active to automate `mempalace sync`.


