use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use axum::{routing::get, Json, Router, Extension};
use serde::{Deserialize, Serialize};
use app_lib::get_providers;
use app_lib::api_logic::{AddressEntry, Settings, UnifiedAlert};
use app_lib::network_state::NetworkState;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct HaOptions {
    addresses: Vec<AddressEntry>,
    #[serde(rename = "enabled_sources")]
    enabled_sources: Vec<String>,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_true", rename = "show_other_outages")]
    show_other_outages: bool,
}

fn default_port() -> u16 {
    8000
}

fn default_true() -> bool {
    true
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
        }
    });

    log::info!("Loaded {} addresses and {} enabled sources.", options.addresses.len(), options.enabled_sources.len());

    let settings = Settings {
        addresses: options.addresses,
        primary_address_index: Some(0),
        theme: None,
        language: Some("pl".to_string()),
        enabled_sources: Some(options.enabled_sources),
        notification_preferences: HashMap::new(),
        upcoming_notification_enabled: false,
        upcoming_notification_hours: 24,
        show_other_outages: options.show_other_outages,
    };

    let shared_alerts: SharedAlerts = Arc::new(Mutex::new(Vec::new()));
    let shared_alerts_clone = shared_alerts.clone();
    let settings_clone = settings.clone();

    // Start background fetching task
    tokio::spawn(async move {
        let fetch_interval = Duration::from_secs(15 * 60); // Fetch every 15 minutes
        loop {
            log::info!("Starting alerts fetch cycle...");
            match fetch_alerts(&settings_clone).await {
                Ok(alerts) => {
                    log::info!("Successfully fetched and processed {} alerts.", alerts.len());
                    let mut lock = shared_alerts_clone.lock().unwrap();
                    *lock = alerts;
                }
                Err(e) => {
                    log::error!("Error during alerts fetch cycle: {}", e);
                }
            }
            log::info!("Sleeping for 15 minutes...");
            tokio::time::sleep(fetch_interval).await;
        }
    });

    // Build axum router
    let app = Router::new()
        .route("/alerts", get(get_alerts_handler))
        .route("/health", get(health_handler))
        .layer(Extension(shared_alerts));

    let addr = format!("0.0.0.0:{}", options.port);
    log::info!("Listening REST API on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_alerts_handler(Extension(alerts): Extension<SharedAlerts>) -> Json<Vec<UnifiedAlert>> {
    let lock = alerts.lock().unwrap();
    Json(lock.clone())
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
