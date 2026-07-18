[English](README.md) | [Polski](README.pl.md)

# Application for Warning and Alerting of Repairs and Infrastructure Accidents

<p align="center">
  <img height="600" alt="image" src="https://github.com/user-attachments/assets/c15ed4a0-52b1-4cc7-9920-b63b87b60fc2" />
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
  - **GPEC Gdańsk**: Heat outages in Gdańsk.
  - **SEC Szczecin**: Heat outages in Szczecin.
  - **LPEC Lublin**: Heat outages in Lublin.
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
  - **Gdańskie Wodociągi**: Water maintenance and failures in Gdańsk.
  - **PUK Rokietnica**: Water maintenance and failures in Rokietnica.
  - **MPWiK Lublin**: Water maintenance and failures in Lublin.

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
- **Accessibility**: Built with WCAG 2.2 AAA partial compliance in mind. Features a dedicated High-Contrast mode with fully visible custom checkboxes and dynamically colored scalable SVG icons for superior readability.
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

## Home Assistant Add-on (Highly Experimental)

Awaria is a headless monitor for utility outages in Poland. This Add-on seamlessly integrates Awaria into Home Assistant.

To add this repository to your Home Assistant Add-on store:

1. Go to **Settings** > **Add-ons** > **Add-on Store**.
2. Click the three dots in the top right and select **Repositories**.
3. Add the repository URL: `https://github.com/eremef/awaria-outage-notifier`.
4. Locate **Awaria Outage Monitor** in the list and click **Install**.

### Features

- **Ingress UI:** Access the Awaria dashboard directly from your Home Assistant sidebar.
- **MQTT Discovery:** Automatically creates sensors in Home Assistant for active outages.
- **Auto-Resolution:** Simplifies setup by auto-resolving full Teryt database locations from basic city and street names.
- **Dynamic Calendar Feed:** Exposes calendar data over an iCalendar API for simple calendar sync.
- **Lovelace Custom Card:** Includes a premium dashboard card to view local outages cleanly.
- **Native Events:** Fires native HA events (`awaria_outage`) when a new outage is detected.

### Prerequisites

- **MQTT Broker:** To get sensors and triggers in Home Assistant, you must have an MQTT broker configured (e.g., the Mosquitto broker Add-on). Awaria will connect to it automatically.

### Configuration

Go to the **Configuration** tab of this Add-on to set your addresses and providers.

#### Addresses

For each address you want to monitor, add an entry with:

- **name**: A friendly name (e.g., `Home`, `Office`)
- **cityName**: The exact city name (e.g., `Warszawa`, `Wrocław`)
- **streetName**: The exact street name **without "ul."** (e.g., `Marszałkowska`, `Rynek`)
- **houseNo**: The exact house number (e.g., `12`, `45/2`). Optional, but highly recommended for accurate matching (especially for providers like Tauron).
- **isActive**: `true` to enable monitoring for this address.

*Note: Awaria will automatically resolve the district, commune, and internal IDs using its bundled Teryt database.*

#### Enabled Sources

Select the utility providers you want to monitor (e.g., `tauron`, `pge`, `mpwik_warszawa`).

### Usage

Once configured and started:

1. Click **Open Web UI** to view the Awaria dashboard.
2. In Home Assistant, go to **Settings -> Devices & Services -> MQTT**. You should see a new device named `Awaria` with sensors for each of your enabled providers.
3. The sensors will show the count of active local outages.
4. The sensors contain detailed JSON attributes (`alerts`) with full outage descriptions, start/end dates, and locations.

### Lovelace Custom Card

Instead of viewing the complete app interface in the sidebar, you can display outages in a premium, custom dashboard card:

1. Copy the file `public/awaria-card.js` from the repository into your Home Assistant `/config/www/` directory.
2. Go to **Settings -> Dashboards -> Click three dots in top-right -> Resources -> Add Resource**.
3. Set the URL to `/local/awaria-card.js` and select **JavaScript Module** as the Resource Type.
4. Refresh your browser page.
5. In your Lovelace dashboard, click **Add Card**, choose **Manual**, and enter the following configuration:

   ```yaml
   type: custom:awaria-card
   ```

### Calendar Sync

The add-on dynamically generates an iCalendar (`.ics`) feed of all active and upcoming outages.

1. Install the **ICS Calendar** integration (available via HACS), as Home Assistant's native Local Calendar does not support loading remote/online `.ics` files.
2. Use the following internal URL for synchronization:

    ```text
    http://<repo-hash>-awaria-outage-monitor:8000/api/calendar.ics
    ```

    *Note: Replace `<repo-hash>` with the 8-character hash generated by Home Assistant for your custom repository (e.g., `385dfded-awaria-outage-monitor`). You can find this hash by visiting the Add-on page in your Home Assistant settings and looking at the browser's address bar; the hash is the prefix in the URL (e.g. `.../addon/385dfded_awaria_outage_monitor/info`). If you installed the add-on locally (not from a repository), use `local-awaria-outage-monitor`.*

    *(If you are subscribing from outside your Home Assistant instance, use `http://<your-ha-ip>:8000/api/calendar.ics`)*.

### Notification Blueprint (Automations)

Awaria fires a native `awaria_outage` event in Home Assistant whenever a new local outage is detected. You can import the following Blueprint to send notifications directly to your phone Companion App:

```yaml
blueprint:
  name: Awaria Outage Notifications
  description: Send a notification to your mobile phone when a new local outage is detected.
  domain: automation
  input:
    notify_device:
      name: Notification Device
      description: The notify service to send the notification to.
      selector:
        action: {}

trigger:
  - trigger: event
    event_type: awaria_outage

action:
  - run_action: !input notify_device
    data:
      title: "⚠️ Wyłączenie: {{ trigger.event.data.source | upper }}"
      message: >-
        Lokalizacja: {{ trigger.event.data.location }}
        Termin: {{ trigger.event.data.startDate }} do {{ trigger.event.data.endDate }}
        
        Opis: {{ trigger.event.data.message }}
```

### Important Note on Settings

Because Home Assistant manages the configuration of this Add-on, the "Settings" menu inside the Awaria Web UI is disabled. All configuration changes (adding addresses, changing providers) must be done via the HA Add-on **Configuration** tab.

## Troubleshooting

- **Widget shows "?"**: The settings haven't been configured yet. Open the main app and set your location.
- **EOF Errors**: Most likely a temporary race condition during settings sync. The app includes resilient logic to retry or fall back to defaults.
- **Missing Alerts**: Check if you have the specific outage category enabled in the settings. **Note**: For new users, all sources are disabled by default.

## License

[MIT](LICENSE)

## [Third-party licenses](THIRD-PARTY-LICENSES.md)
