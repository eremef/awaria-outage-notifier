[English](README.md) | [Polski](README.pl.md)

# Application for Warning and Alerting of Repairs and Infrastructure Accidents

<p align="center">
   <img height="600" alt="image" src="https://github.com/user-attachments/assets/108932b2-0fe0-4a19-8769-61d4962835ac" />
</p>

A modern desktop (Tauri) and Android application providing real-time alerts for planned and emergency outages. **AWARIA** aggregates data from multiple utility providers into a centralized interface.

## Downloads

[https://eremef.xyz/awaria](https://eremef.xyz/awaria)

## Supported Sources

- **⚡ Tauron (Power)**: Planned maintenance and emergency power outages.
- **⚡ Energa (Power)**: Planned power outages (Northern Poland).
- **⚡ Enea (Power)**: Planned maintenance (Western Poland).
- **⚡ PGE (Power)**: Planned power outages (Eastern/Central Poland).
- **⚡ Stoen (Power)**: Planned power outages (Warsaw area).
- **🔥 Fortum (Heat)**: Planned and current heat/hot water outages.
- **💧 MPWiK (Water)**: Water failures and maintenance work (currently Wrocław area).

## Android app

<p align="center">
  <img height="600" alt="image" src="https://github.com/user-attachments/assets/2760977f-67a5-465d-9e59-0629b0b958b5" />
</p>

## Features

- **Multi-Source Logic**: Aggregates alerts from different utility providers (Power, Water, etc.).
- **Source Selection**: Customize which types of outages you want to see in the settings.
- **Multi-Address Support**: Monitor up to 20 different locations simultaneously.
- **Smart Address Matching**: Highlights alerts affecting your specific address (or addresses). **Note**: Currently, matching is performed at the city and street level; building-specific filtering by house number is not yet implemented.
- **Real-time Push Notifications**: Receive instant alerts on your desktop or mobile device when a new outage is detected for your location.
- **Background Monitoring**: Automatically checks for updates in the background, even when the app is minimized or closed.
- **Throttled Parallelism**: Modern backend logic that fetches from all providers simultaneously with smart retries for maximum reliability.
- **Premium Design**:
  - **Modern Interface**: Indigo-based "friendly" UI with vibrant source indicators (Rose/Sky).
  - **Collapsible categories**: Organized view of "Your Location" vs "Other Outages".
  - **Responsive Dark/Light mode**: Native transition support.
- **Android Widgets**:
  - **Individual Source Widgets**: Separate widgets for all providers.
  - **Optimized Layout**: Compact 1x1 design showing alert counts for your specific street.
  - **One-tap refresh**: Tap the widget to trigger an immediate update.
  - **Shared configuration**: Settings sync automatically from the main app.
- **Privacy First**: No cloud accounts. Your location and settings stay on your device.

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

## Architecture

- **Frontend**: Vanilla HTML/JS/CSS in `public/`. Indigo design system with custom HSL tokens.
- **Backend (Rust)**: `src-tauri/src/lib.rs` orchestrates asynchronous fetching from multiple APIs and converts them to a `UnifiedAlert` format.
- **Android Widgets**: Native implementation utilizing a `BaseWidgetProvider` with specific providers for each utility (e.g. `TauronWidgetProvider`, `StoenWidgetProvider`). Includes a `WorkManager` background worker for periodic updates.

## Settings

Settings are stored in `settings.json` in the app's data directory:

- **Desktop**: `%APPDATA%\xyz.eremef.awaria\` (Windows)
- **Android**: `/data/user/0/xyz.eremef.awaria/files/`

## Troubleshooting

- **Widget shows "?"**: The settings haven't been configured yet. Open the main app and set your location.
- **EOF Errors**: Most likely a temporary race condition during settings sync. The app includes resilient logic to retry or fall back to defaults.
- **Missing Alerts**: Check if you have the specific outage category enabled in the settings. **Note**: For new users, all sources are disabled by default.
