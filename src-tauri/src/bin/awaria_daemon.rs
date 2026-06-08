use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use axum::{routing::get, Json, Router, Extension};
use serde::{Deserialize, Serialize};
use app_lib::get_providers;
use app_lib::api_logic::{AddressEntry, Settings, UnifiedAlert};
use app_lib::network_state::NetworkState;
use rumqttc::{AsyncClient, MqttOptions, QoS};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct HaAddress {
    name: String,
    #[serde(rename = "cityName")]
    city_name: String,
    #[serde(rename = "streetName")]
    street_name: String,
    #[serde(default = "default_true", rename = "isActive")]
    is_active: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct HaOptions {
    addresses: Vec<HaAddress>,
    #[serde(rename = "enabled_sources")]
    enabled_sources: Vec<String>,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_true", rename = "show_other_outages")]
    show_other_outages: bool,
    #[serde(default, rename = "filter_by_house_no")]
    filter_by_house_no: bool,
    #[serde(default = "default_interval", rename = "check_interval_minutes")]
    check_interval_minutes: u64,
}

fn default_port() -> u16 {
    8000
}

fn default_true() -> bool {
    true
}

fn default_interval() -> u64 {
    15
}

type SharedAlerts = Arc<Mutex<Vec<UnifiedAlert>>>;

#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting Awaria Daemon...");

    // Initialize ring crypto provider for rustls
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Determine config path
    let config_path = std::env::var("AWARIA_CONFIG_PATH")
        .unwrap_or_else(|_| "/data/options.json".to_string());

    log::info!("Loading options from: {}", config_path);

    let options_raw = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|_| {
            log::warn!("Could not read config file at {}. Using default empty settings.", config_path);
            "{}".to_string()
        });

    let options: HaOptions = serde_json::from_str(&options_raw).unwrap_or_else(|e| {
        log::error!("Failed to parse config JSON: {}. Using default options.", e);
        HaOptions {
            addresses: Vec::new(),
            enabled_sources: Vec::new(),
            port: 8000,
            show_other_outages: true,
            filter_by_house_no: false,
            check_interval_minutes: 15,
        }
    });

    log::info!("Loaded {} addresses and {} enabled sources.", options.addresses.len(), options.enabled_sources.len());

    // Resolve Teryt
    let teryt_path = std::path::PathBuf::from(std::env::var("AWARIA_TERYT_PATH").unwrap_or_else(|_| "/usr/local/share/awaria/assets/teryt".to_string()));
    let teryt_path = if teryt_path.exists() { teryt_path } else { std::path::PathBuf::from("../src-tauri/assets/teryt") };

    let mut resolved_addresses = Vec::new();
    for ha_addr in &options.addresses {
        let mut entry = AddressEntry {
            name: ha_addr.name.clone(),
            city_name: ha_addr.city_name.clone(),
            street_name: ha_addr.street_name.clone(),
            is_active: ha_addr.is_active,
            voivodeship: "".to_string(),
            district: "".to_string(),
            commune: "".to_string(),
            street_name_1: "".to_string(),
            street_name_2: None,
            house_no: "".to_string(),
            city_id: None,
            street_id: None,
        };

        if teryt_path.exists() {
            if let Ok(cities) = app_lib::teryt::lookup_cities_by_path(&teryt_path, &ha_addr.city_name) {
                if let Some(city) = cities.into_iter().find(|c| c.city.to_lowercase() == ha_addr.city_name.to_lowercase()) {
                    entry.voivodeship = city.voivodeship;
                    entry.district = city.district;
                    entry.commune = city.commune;
                    entry.city_id = Some(city.city_id);

                    if !ha_addr.street_name.is_empty() {
                        if let Ok(streets) = app_lib::teryt::lookup_streets_by_path(&teryt_path, city.city_id, &ha_addr.street_name) {
                            if let Some(street) = streets.into_iter().find(|s| s.street_name_1.to_lowercase() == ha_addr.street_name.to_lowercase() || s.full_street_name.to_lowercase().contains(&ha_addr.street_name.to_lowercase())) {
                                entry.street_id = Some(street.street_id);
                                entry.street_name_1 = street.street_name_1.clone();
                                entry.street_name_2 = street.street_name_2.clone();
                            }
                        }
                    }
                }
            }
        }
        resolved_addresses.push(entry);
    }

    let settings = Settings {
        addresses: resolved_addresses,
        primary_address_index: Some(0),
        theme: None,
        language: Some("pl".to_string()),
        enabled_sources: Some(options.enabled_sources),
        notification_preferences: HashMap::new(),
        upcoming_notification_enabled: false,
        upcoming_notification_hours: 24,
        show_other_outages: options.show_other_outages,
        filter_by_house_no: options.filter_by_house_no,
    };

    let shared_alerts: SharedAlerts = Arc::new(Mutex::new(Vec::new()));
    let shared_alerts_clone = shared_alerts.clone();
    let settings_clone = settings.clone();
    let check_interval_minutes = options.check_interval_minutes;

    // Setup MQTT
    let mqtt_client = setup_mqtt(&settings).await;

    // Start background fetching task
    let mqtt_client_loop = mqtt_client.clone();
    tokio::spawn(async move {
        let fetch_interval = Duration::from_secs(check_interval_minutes * 60);
        loop {
            log::info!("Starting alerts fetch cycle...");
            match fetch_alerts(&settings_clone).await {
                Ok(alerts) => {
                    log::info!("Successfully fetched and processed {} alerts.", alerts.len());
                    {
                        let mut lock = shared_alerts_clone.lock().unwrap();
                        *lock = alerts.clone();
                    }

                    if let Some(client) = &mqtt_client_loop {
                        publish_mqtt_state(client, &settings_clone, &alerts).await;
                    }
                }
                Err(e) => {
                    log::error!("Error during alerts fetch cycle: {}", e);
                }
            }
            log::info!("Sleeping for {} minutes...", check_interval_minutes);
            tokio::time::sleep(fetch_interval).await;
        }
    });

    let public_dir = std::env::var("AWARIA_PUBLIC_DIR")
        .unwrap_or_else(|_| "/usr/local/share/awaria/public".to_string());
    let public_dir = if std::path::Path::new(&public_dir).exists() { public_dir } else { "../public".to_string() };

    let serve_dir = tower_http::services::ServeDir::new(&public_dir);

    // Build axum router
    let app = Router::new()
        .nest_service("/", serve_dir)
        .route("/api/alerts", get(get_alerts_handler))
        .route("/api/settings", get(get_settings_handler))
        .route("/health", get(health_handler))
        .layer(Extension(shared_alerts))
        .layer(Extension(settings));

    let addr = format!("0.0.0.0:{}", options.port);
    log::info!("Listening REST API and UI on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn setup_mqtt(settings: &Settings) -> Option<AsyncClient> {
    let mqtt_host = std::env::var("MQTT_HOST").ok()?;
    let user = std::env::var("MQTT_USER").unwrap_or_default();
    let pass = std::env::var("MQTT_PASSWORD").unwrap_or_default();
    let port: u16 = std::env::var("MQTT_PORT").unwrap_or_else(|_| "1883".to_string()).parse().unwrap_or(1883);

    log::info!("Connecting to MQTT Broker at {}:{}", mqtt_host, port);
    let mut mqttoptions = MqttOptions::new("awaria_daemon", mqtt_host, port);
    mqttoptions.set_credentials(user, pass);
    mqttoptions.set_keep_alive(Duration::from_secs(5));

    let (client, mut connection) = AsyncClient::new(mqttoptions, 10);

    tokio::spawn(async move {
        loop {
            if let Err(e) = connection.poll().await {
                log::error!("MQTT connection error: {:?}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    });

    // Publish discovery configs
    for provider in get_providers() {
        if settings.enabled_sources.as_ref().unwrap().contains(&provider.id()) {
            let topic = format!("homeassistant/sensor/awaria_{}/config", provider.id());
            let payload = serde_json::json!({
                "name": format!("Awaria {}", provider.id()),
                "state_topic": format!("homeassistant/sensor/awaria_{}/state", provider.id()),
                "json_attributes_topic": format!("homeassistant/sensor/awaria_{}/attributes", provider.id()),
                "unique_id": format!("awaria_{}", provider.id()),
                "icon": "mdi:power-plug-off",
            });
            let _ = client.publish(topic, QoS::AtLeastOnce, true, serde_json::to_vec(&payload).unwrap()).await;
        }
    }

    Some(client)
}

async fn publish_mqtt_state(client: &AsyncClient, settings: &Settings, alerts: &Vec<UnifiedAlert>) {
    let enabled = settings.enabled_sources.as_ref().unwrap();
    for provider in get_providers() {
        if !enabled.contains(&provider.id()) { continue; }

        let provider_alerts: Vec<_> = alerts.iter().filter(|a| a.source == provider.source()).collect();
        let local_count = provider_alerts.iter().filter(|a| a.is_local == Some(true)).count();

        let state_topic = format!("homeassistant/sensor/awaria_{}/state", provider.id());
        let attrs_topic = format!("homeassistant/sensor/awaria_{}/attributes", provider.id());

        let _ = client.publish(state_topic, QoS::AtLeastOnce, true, local_count.to_string()).await;
        let _ = client.publish(attrs_topic, QoS::AtLeastOnce, true, serde_json::to_vec(&serde_json::json!({"alerts": provider_alerts})).unwrap()).await;
    }
}

async fn get_alerts_handler(Extension(alerts): Extension<SharedAlerts>) -> Json<serde_json::Value> {
    let lock = alerts.lock().unwrap();
    Json(serde_json::json!({ "alerts": lock.clone(), "is_stale": false, "is_offline": false }))
}

async fn get_settings_handler(Extension(settings): Extension<Settings>) -> Json<Settings> {
    Json(settings)
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn fetch_alerts(settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    // 1. Build clients
    let client = NetworkState::build_client().map_err(|e| e.to_string())?;
    let client_http1 = NetworkState::build_client_http1().map_err(|e| e.to_string())?;

    // 2. Connectivity check
    if !NetworkState::check_internet_connection(&client).await {
        return Err("No internet connection".to_string());
    }

    let mut all_alerts = Vec::new();
    let mut tasks = Vec::new();

    let enabled_sources = settings.enabled_sources.clone().unwrap_or_default();
    let providers = get_providers();

    for provider in providers {
        if !enabled_sources.contains(&provider.id()) {
            continue;
        }

        if !app_lib::api_logic::is_provider_applicable(provider.source(), settings) {
            log::info!("Skipping {}, not applicable for active addresses", provider.id());
            continue;
        }

        // Webview-only providers are skipped on headless environment
        let is_webview = provider.id() == "psg" || provider.id() == "gpec" || provider.id() == "pwik_kalisz";
        if is_webview {
            log::warn!("WebView-only provider '{}' is not supported in headless mode and will be skipped.", provider.id());
            continue;
        }

        let c = client.clone();
        let c_h1 = client_http1.clone();
        let s = settings.clone();
        tasks.push(tokio::spawn(async move {
            let (alerts, errs) = provider.fetch(&c, &c_h1, &s, None).await;
            (provider.id(), alerts, errs)
        }));
    }

    let results = futures::future::join_all(tasks).await;
    for res in results {
        match res {
            Ok((id, alerts, errs)) => {
                if !errs.is_empty() {
                    log::warn!("Provider '{}' encountered errors: {:?}", id, errs);
                }
                log::info!("Provider '{}' returned {} alerts.", id, alerts.len());
                all_alerts.extend(alerts);
            }
            Err(e) => {
                log::error!("Fetch task panicked: {}", e);
            }
        }
    }

    // Deduplicate alerts
    let mut deduplicated = app_lib::api_logic::deduplicate_alerts(all_alerts);

    // Apply is_local and address filters
    deduplicated.retain(|alert| {
        if let Some(idx) = alert.address_index {
            if idx < settings.addresses.len() {
                return settings.addresses[idx].is_active;
            }
        }

        if alert.is_local == Some(false) {
            if let Some(loc) = &alert.location {
                if loc.contains("Wrocław") {
                    return settings.addresses.iter().any(|a| a.is_active && app_lib::api_logic::is_wroclaw(a));
                }
                if loc.contains("Warszawa") {
                    return settings.addresses.iter().any(|a| a.is_active && app_lib::api_logic::is_warszawa(a));
                }
                if loc.contains("Kraków") {
                    return settings.addresses.iter().any(|a| a.is_active && app_lib::api_logic::is_krakow(a));
                }
                for addr in settings.addresses.iter().filter(|a| a.is_active) {
                    if loc.contains(&addr.city_name) {
                        return true;
                    }
                }
                return false;
            }
        }
        true
    });

    // Sort alerts by start date
    deduplicated.sort_by(|a, b| {
        let date_cmp = match (&a.startDate, &b.startDate) {
            (Some(da), Some(db)) => da.cmp(db),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        if date_cmp != std::cmp::Ordering::Equal {
            return date_cmp;
        }
        a.source.to_string().cmp(&b.source.to_string())
    });

    Ok(deduplicated)
}
