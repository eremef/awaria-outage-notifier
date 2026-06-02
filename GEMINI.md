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

- `npm run tauri dev`: Start desktop development server (usually on <http://localhost:1430>).
- `npm run android:dev`: Start Android development server.
- `npm run build`: Build web assets and desktop application.
- `npm run android:build`: Build Android application.

## User Rules

- **Manual Commits Only**: Never stage, commit, or push git changes automatically. Always wait for the user's explicit request (e.g., via the `/commit` workflow).
- when adding provider color/accent, adjust it for the themes colors to make it easily readable.
- use `cargo clippy` instead of `cargo check`

## Outage card layout

Use this layout when implementing the new provider. I used placeholders for the readability.

{utility_icon} {provider_name}
{start_date} - {end_date}
Miejscowość: {address_city}
{incident_type} - {description_with_streets_etc}

## Project Learnings

### JNI 0.22 Migration & Android Stabilization

- **JNI 0.22 API**: `JNIEnv` is now `Env`. `JavaVM` must be stored in a global static (e.g., `Mutex<Option<JavaVM>>`).
- **JNI Strings**: Use `jni::jni_str!("...")` for all literal method/class names and signatures to satisfy JNI 0.22 trait bounds.
- **Global State**: Critical globals (`JAVA_VM`, `ANDROID_CONTEXT`, `PSG_FETCHER_CLASS`) must be initialized during `JNI_OnLoad` or the first bridge call. Always lock and check `Option` before use.
- **Thread Attachment**: Use the closure-based `vm.attach_current_thread(|env| { ... })` for background tasks.
- **`EnvUnowned::with_env`**: Requires the `native_env` variable to be declared `mut`.
- **Widget Filtering**: Counters MUST filter by `is_local == Some(true)` to avoid showing city-wide outages that don't affect the user's specific street.
- **Provider Scrapers (Kotlin)**: Fixed PSG count logic to include city-wide outages even when no street is configured.

### Background Monitoring & Notifications

- **Android WorkManager**: Uses `BackgroundMonitorWorker` and `WidgetUpdateWorker`. Ensure `ensure_verifier_initialized` is called in each worker's entry point.
- **Settings Sync**: Kotlin side uses `loadSettings(context)` which returns `Pair<Settings, String>` (object + raw JSON). The raw JSON is passed to Rust to avoid double serialization.
- **Rust MonitorEngine**: The `MonitorEngine::process_alerts` handles deduplication and notification triggers. It relies on `ProviderCache` to avoid redundant network calls.
- **Notification Formatting**: Excessive whitespace in notifications (especially PSG) is caused by raw HTML remnants and newlines. Use `split_whitespace().collect::<Vec<_>>().join(" ")` in Rust or `replace(Regex("\\s+"), " ")` in Kotlin to normalize strings.
- **Date Parsing**: Polish providers often use dots (`.`) and "godz." prefixes. `utils.rs::parse_date` should be extended to handle these by cleaning the string (dots to hyphens, remove "godz.") before parsing with `NaiveDateTime`.

### Aquanet Matching & Local Outage Verification

- **Street & City Fields Inclusion**: In `aquanet::fetch`, matching of addresses relies on a compiled regex (generated from the user's `streetName1` / `streetName2` and matched against a `combined_text` string). Initially, this `combined_text` was constructed using only `item.title`, `item.location`, and `item.description`. However, `Aquanet` detail scraping populates `item.city` and `item.streets` separately. Because these fields were missing from `combined_text`, the regex matching failed, resulting in `is_local` being incorrectly set to `Some(false)`. This caused Android widgets (which only count `is_local == Some(true)` outages) to report 0 outages. Fixing this requires including `item.city` and `item.streets` in `combined_text`.

### Browser Automation & Scraping

- **Dynamic Dropdowns**: For sites with autocomplete (like Tauron), `type_text` must be followed by an explicit click on the suggested item `uid`.
- **API Discovery**: Use `list_network_requests` to find direct `/api/` endpoints instead of perfecting UI scrapers. Direct HTTP requests (via `reqwest`) are more robust.
- **CMP/Cookie Popups**: Often can be ignored for pixel-based input targeting, but "Accept" buttons should be clicked if they overlay target elements.

### Android Sharing & File Access

- **Internal File Sharing**: To share files from the app's internal storage (e.g., `settings.json`), `res/xml/file_paths.xml` MUST include `<files-path name="..." path="." />`.
- **Share Intent**: When starting a Share Intent from `ApplicationContext`, `Intent.FLAG_ACTIVITY_NEW_TASK` is required for the chooser activity.
- **Plugin Bug (tauri-plugin-share v2.0.5)**: Rust wrapper uses snake_case (`share_file`) while Kotlin registers camelCase (`shareFile`), causing "No command found" errors. Use direct JNI or manual command invocation as a workaround.

### Reqwest RequestBuilder & Query Parameter Type Inference
 
 - **RequestBuilder query compilation issues**: In some environments, using `.query(&[("a", "b")])` on a `reqwest::RequestBuilder` with custom feature-restricted configurations can fail to compile due to missing standard query methods or cause type-inference errors on `.send().await` or `.map_err()`.
 - **Solution**: To bypass reqwest RequestBuilder version/feature constraints, construct URLs manually using `format!` and the `urlencoding::encode` crate, e.g. `format!("https://host/api?a={}", urlencoding::encode(b))`. This is 100% robust, highly readable, and compiles flawlessly.
 
### Android Instrumentation Tests & WebView Deadlocks

- **WebView Event Loop Deadlock**: In Android instrumentation tests, using `runBlocking` on the test thread while orchestrating WebView events on `Dispatchers.Main` can lead to hangs. If `withTimeoutOrNull` triggers a timeout, it cancels the `deferred.await()` call, but the `CompletableDeferred` itself is not completed/cancelled. If a background Handler continues to post delayed tasks to the Main Looper recursively, the loop stays active forever, preventing the test runner from idling.
- **Solution**: Always use a `try/finally` block inside WebView fetching methods to ensure the `deferred` promise is explicitly completed/cancelled and that `webView.stopLoading()` and `webView.destroy()` are called on the Main thread upon cancellation or timeout.

## Common Development Workflows

- **Android Release Build**: `npx tauri android build -- --target aarch64 --apk`
- **Log Monitoring**: `adb logcat | grep awaria`
- **Isolated Rust Tests**: To avoid file locks during active builds, always run tests with a separate target directory: `cargo test --target-dir target_test`. Clean up after completion.
- **Knowledge Persistence**: The project uses MemPalace for memory storage. Ensure git hooks are active to automate `mempalace sync`. Ensure to use it during programming.
