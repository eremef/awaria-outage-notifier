[English](README.md) | [Polski](README.pl.md)

# Application for Warning and Alerting of Repairs and Infrastructure Accidents

<p align="center">
  <img height="600" alt="image" src="https://github.com/user-attachments/assets/2901b970-f0ff-45f8-93d3-e00ad6564289" />
</p>

A modern desktop (Tauri) and Android application providing real-time alerts for planned and emergency outages. **AWARIA** aggregates data from multiple utility providers into a centralized interface.

## Downloads
- [Google Play store](https://play.google.com/store/apps/details?id=xyz.eremef.awaria)
- [https://eremef.xyz/awaria](https://eremef.xyz/awaria)

## Other links
- [Facebook](https://www.facebook.com/awaria.info)

## Supported Providers

- **⚡ Power**
  - **Tauron**: Planned maintenance and emergency power outages.
  - **Energa**: Planned power outages (Northern Poland).
  - **Enea**: Planned maintenance (Western Poland).
  - **PGE**: Planned power outages (Eastern/Central Poland).
  - **Stoen**: Planned power outages (Warsaw area).
- **🔥 Gas - PSG**: Planned and current gas outages.
- **🌡️ Heat**
  - **Fortum**: Planned and current heat/hot water outages.
  - **Tauron Ciepło**: Heat outages (Southern Poland).
  - **Veolia Warszawa**: Heat outages in Warsaw.
  - **Veolia Poznań**: Heat outages in Poznań.
  - **Veolia Łódź**: Heat outages in Łódź.
- **💧 Water**
  - **MPWiK Wrocław**: Water failures and maintenance work in Wrocław.
  - **MPWiK Warszawa**: Water failures and repairs in Warsaw.
  - **WMK**: Water maintenance and failures in Kraków.
  - **Aquanet**: Water maintenance and failures in Poznań and surrounding areas.
  - **Katowickie Wodociągi**: Water maintenance and failures in Katowice.
  - **ZWiK Łódź**: Water maintenance and failures in Łódź.
  - **PWiK Kalisz**: Water maintenance and failures in Kalisz.
  - **PWiK Częstochowa**: Water maintenance and failures in Częstochowa.
  - **Wodociągi Płockie**: Water maintenance and failures in Płock.

## Android app

## Features

- **Multi-Source Logic**: Aggregates alerts from different utility providers (Power, Water, etc.).
- **Source Selection**: Customize which types of outages you want to see in the settings.
- **Multi-Address Support**: Monitor up to 20 different locations simultaneously.
- **Smart Address Matching**: Highlights alerts affecting your specific address (or addresses) using the official **TERYT database** for accurate street and city lookup.
- **Real-time Push Notifications**: Receive instant alerts on your desktop or mobile device when a new outage is detected for your location.
- **Background Monitoring**: Automatically checks for updates in the background (30-minute interval on desktop).
- **Smart Prefiltration**: Optimizes network usage by only querying providers applicable to your selected voivodeships.
- **Throttled Parallelism**: Modern backend logic that fetches from all providers simultaneously with smart retries for maximum reliability.
- **Settings Portability**: Easily **Export and Import** your settings (addresses and enabled sources) to move between devices or back up your configuration.
- **Android Optimizations**: 
  - **Battery Management**: Built-in support to request exclusion from battery optimizations to ensure reliable background checks and notifications.
  - **Native Widgets**: Quick access to alert counts directly from your home screen.
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
