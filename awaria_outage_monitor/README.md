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
- **isActive**: `true` to enable monitoring for this address.

*Note: Awaria will automatically resolve the district, commune, and internal IDs using its bundled Teryt database.*

### Enabled Sources
Select the utility providers you want to monitor (e.g., `tauron`, `pge`, `mpwik_warszawa`).

## Usage
Once configured and started:
1. Click **Open Web UI** to view the Awaria dashboard.
2. In Home Assistant, go to **Settings -> Devices & Services -> MQTT**. You should see a new device named `Awaria` with sensors for each of your enabled providers.
3. The sensors will show the count of active local outages.
4. The sensors contain detailed JSON attributes (`alerts`) with full outage descriptions, start/end dates, and locations. You can use these in Home Assistant Templates to trigger notifications!

## Important Note on Settings
Because Home Assistant manages the configuration of this Add-on, the "Settings" menu inside the Awaria Web UI is disabled. All configuration changes (adding addresses, changing providers) must be done via the HA Add-on **Configuration** tab.
