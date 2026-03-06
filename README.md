# Tauron Outages Notifier


<p align="center">
  <img width="802" height="632" alt="image" src="https://github.com/user-attachments/assets/537753c4-411d-4b76-9f5a-4709f00b56cf" />
</p>

A desktop (Tauri) and Android app to check for planned power outages in your area using the Tauron API.

## Downloads

https://eremef.xyz/tauron-notifier

## Android app

<p align="center">
 <img width="300" alt="Image" src="https://github.com/user-attachments/assets/7e9cefe4-16b4-498c-bfaf-cb998cf22a40" />
</p>

## Features

- **Desktop & Android Support**: Built with Tauri v2.
- **Dynamic Configuration**: Set your City, Street, and House Number in the app.
- **Android Widget**:
  - Shows outage count for your specific street.
  - "Tap to refresh" functionality.
  - Reads settings shared with the main app.
- **Smart Filtering**: Displays outages relevant to your specific address while still showing other outages in the area.
- **Language setting**: Choice between Polish and English
- **Themes setting**: Choice between light and dark theme

## Prerequisites

- Node.js (v18+)
- Rust (stable)
- Android Studio & SDK (for Android builds)
- Global Tauri CLI: `npm install -g @tauri-apps/cli`

## Setup

1. Install dependencies:

   ```bash
   npm install
   ```

## Development

### Desktop

Run the desktop app in development mode:

```bash
npm run dev
```

### Android

Run on a connected Android device or emulator:

```bash
npm run android
```

## Building

### Desktop app

Build the release bundle:

```bash
npm run build
```

### Android APK

Build the Android APK (unsigned/debug):

```bash
npm run android:build
```

The APK will be located at:
`src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk`

> **Note**: You may need to sign the APK or configure signing keys in `build.gradle.kts` for a release version.

## Architecture

- **Frontend**: Vanilla HTML/JS/CSS in `public/`.
- **Backend (Rust)**: `src-tauri/src/lib.rs` handles API requests (`fetch_outages`, `lookup_city`, `lookup_street`) and file persistence.
- **Android Widget**: `src-tauri/gen/android/app/src/main/java/xyz/eremef/tauron_notifier/OutageWidgetProvider.kt` implements the home screen widget using native Android APIs and reads the shared `settings.json`.

## Settings

Settings are stored in `settings.json` in the app's data directory:

- **Desktop**: `%APPDATA%\xyz.eremef.tauron_notifier\` (Windows)
- **Android**: `/data/user/0/xyz.eremef.tauron_notifier/files/`

## Troubleshooting

- **Widget shows "?"**: The settings haven't been configured yet. Open the main app, go to Settings, and save your location.
- **API Errors**: The Tauron API might be down or blocking requests. The app uses a hardcoded `Referer` header to mimic a browser.
