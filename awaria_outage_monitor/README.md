# Awaria Outage Monitor Add-on

Awaria is a headless monitor for utility outages in Poland. This Add-on seamlessly integrates Awaria into Home Assistant.

## Features

- **Ingress UI:** Access the Awaria dashboard directly from your Home Assistant sidebar.
- **MQTT Discovery:** Automatically creates sensors in Home Assistant for active outages.
- **Auto-Resolution:** Simplifies setup by auto-resolving full Teryt database locations from basic city and street names.

## Prerequisites

- **MQTT Broker:** To get sensors and triggers in Home Assistant, you must have an MQTT broker configured (e.g., the Mosquitto broker Add-on). Awaria will connect to it automatically.

## Configuration

Go to the **Configuration** tab of this Add-on to set your addresses and providers.

### Addresses

For each address you want to monitor, add an entry with:

- **name**: A friendly name (e.g., `Home`, `Office`)
- **cityName**: The exact city name (e.g., `Warszawa`, `Wrocław`)
- **streetName**: The exact street name **without "ul."** (e.g., `Marszałkowska`, `Rynek`)
- **houseNo**: The exact house number (e.g., `12`, `45/2`). Optional, but highly recommended for accurate matching (especially for providers like Tauron).
- **isActive**: `true` to enable monitoring for this address.

*Note: Awaria will automatically resolve the district, commune, and internal IDs using its bundled Teryt database.*

### Enabled Sources

Select the utility providers you want to monitor (e.g., `tauron`, `pge`, `mpwik_warszawa`).

## Usage

Once configured and started:

1. Click **Open Web UI** to view the Awaria dashboard.
2. In Home Assistant, go to **Settings -> Devices & Services -> MQTT**. You should see a new device named `Awaria` with sensors for each of your enabled providers.
3. The sensors will show the count of active local outages.
4. The sensors contain detailed JSON attributes (`alerts`) with full outage descriptions, start/end dates, and locations.

## Lovelace Custom Card

Instead of viewing the complete app interface in the sidebar, you can display outages in a premium, custom dashboard card:

1. Copy the file `public/awaria-card.js` from the repository into your Home Assistant `/config/www/` directory.
2. Go to **Settings -> Dashboards -> Click three dots in top-right -> Resources -> Add Resource**.
3. Set the URL to `/local/awaria-card.js` and select **JavaScript Module** as the Resource Type.
4. Refresh your browser page.
5. In your Lovelace dashboard, click **Add Card**, choose **Manual**, and enter the following configuration:

   ```yaml
   type: custom:awaria-card
   ```

## Calendar Sync

The add-on dynamically generates an iCalendar (`.ics`) feed of all active and upcoming outages.

1. Go to **Settings -> Devices & Services -> Add Integration -> Local Calendar** or install a third-party calendar integration (e.g. `ICS Calendar` via HACS).
2. Use the following internal URL for synchronization:

   ```text
   http://awaria-outage-monitor:8000/api/calendar.ics
   ```

   *(If you are subscribing from outside your Home Assistant instance, use `http://<your-ha-ip>:8000/api/calendar.ics`)*.

## Notification Blueprint (Automations)

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

## Important Note on Settings

Because Home Assistant manages the configuration of this Add-on, the "Settings" menu inside the Awaria Web UI is disabled. All configuration changes (adding addresses, changing providers) must be done via the HA Add-on **Configuration** tab.
