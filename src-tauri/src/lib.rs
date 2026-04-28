mod api_logic;
mod enea;
mod energa;
mod fortum;
mod mpwik;
mod pge;
mod network_state;
use crate::network_state::NetworkState;
mod state_db;
mod stoen;
mod tauron;
mod teryt;
mod utils;
mod cache;
mod psg;

use api_logic::{
    load_settings_from_path, save_settings_to_path,
    AddressEntry, Settings, UnifiedAlert,
    AlertProvider, is_wroclaw, is_warszawa,
};
use tauri::command;
use tauri::AppHandle;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;
use api_logic::{DatabaseInterface, NotificationProvider, MonitorEngine};

#[cfg(target_os = "android")]
use jni::{
    objects::{JClass, JString, JObject},
    sys::jint,
    JNIEnv,
};

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use crate::state_db::DbState;
use futures::future::join_all;
use teryt::{TerytCity, TerytStreet};


const MAX_CONCURRENT_REQUESTS: usize = 5;

// ── Trait implementations for production ──────────────────

struct RealDatabase<'a>(&'a Mutex<rusqlite::Connection>);

impl<'a> DatabaseInterface for RealDatabase<'a> {
    fn is_alert_seen(&self, provider: &str, hash: &str) -> Result<bool, String> {
        let conn = self.0.lock().map_err(|e| e.to_string())?;
        state_db::is_alert_seen(&conn, provider, hash)
    }

    fn mark_alert_as_seen(&self, provider: &str, hash: &str) -> Result<(), String> {
        let conn = self.0.lock().map_err(|e| e.to_string())?;
        state_db::mark_alert_as_seen(&conn, provider, hash)
    }
}

struct RealNotification<'a>(&'a AppHandle);

impl<'a> NotificationProvider for RealNotification<'a> {
    fn show_notification(&self, title: String, body: String, hash: String) {
        self.0.notification()
            .builder()
            .title(title)
            .body(body.clone())
            .large_body(body)
            .icon("ic_notification")
            .extra("hash", hash)
            .show()
            .ok();
    }
}


fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("settings.json"))
}


// ── TERYT local lookups ───────────────────────────────────

#[command]
async fn teryt_lookup_city(app: AppHandle, city_name: String) -> Result<Vec<TerytCity>, String> {
    teryt::lookup_cities(&app, &city_name)
}

#[command]
async fn teryt_lookup_street(
    app: AppHandle,
    city_id: u64,
    street_name: String,
) -> Result<Vec<TerytStreet>, String> {
    teryt::lookup_streets(&app, city_id, &street_name)
}

// ── Settings persistence ──────────────────────────────────

#[command]
async fn save_settings(
    app: AppHandle,
    cache_state: tauri::State<'_, cache::CacheState>,
    settings: Settings,
) -> Result<(), String> {
    let path = settings_path(&app)?;
    save_settings_to_path(&path, &settings)?;
    cache_state.clear();
    Ok(())
}

#[command]
async fn load_settings(app: AppHandle) -> Result<Option<Settings>, String> {
    let path = settings_path(&app)?;
    let settings = load_settings_from_path(&path)?;
    log::info!("load_settings: loaded={:?}", settings.is_some());
    if let Some(ref s) = settings {
        log::info!("load_settings: addresses={}", s.addresses.len());
    }
    Ok(settings)
}

#[command]
async fn add_address(
    app: AppHandle,
    cache_state: tauri::State<'_, cache::CacheState>,
    address: AddressEntry,
) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    let mut settings = load_settings_from_path(&path)?.unwrap_or_default();

    if settings.addresses.len() >= 20 {
        return Err("Maximum of 20 addresses allowed".to_string());
    }

    settings.addresses.push(address);
    if settings.primary_address_index.is_none() {
        settings.primary_address_index = Some(0);
    }

    save_settings_to_path(&path, &settings)?;
    cache_state.clear();
    Ok(settings)
}

#[command]
async fn remove_address(
    app: AppHandle,
    cache_state: tauri::State<'_, cache::CacheState>,
    index: usize,
) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    let mut settings = load_settings_from_path(&path)?.unwrap_or_default();

    if index >= settings.addresses.len() {
        return Err("Invalid address index".to_string());
    }

    settings.addresses.remove(index);

    if let Some(ref mut primary) = settings.primary_address_index {
        if *primary >= settings.addresses.len() {
            *primary = settings.addresses.len().saturating_sub(1);
        }
        if settings.addresses.is_empty() {
            *primary = 0;
        }
    }
    if settings.addresses.is_empty() {
        settings.primary_address_index = None;
    }

    save_settings_to_path(&path, &settings)?;
    cache_state.clear();
    Ok(settings)
}

#[command]
async fn set_primary_address(
    app: AppHandle,
    cache_state: tauri::State<'_, cache::CacheState>,
    index: usize,
) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    let mut settings = load_settings_from_path(&path)?.unwrap_or_default();

    if index >= settings.addresses.len() {
        return Err("Invalid address index".to_string());
    }

    settings.primary_address_index = Some(index);
    save_settings_to_path(&path, &settings)?;
    cache_state.clear();
    Ok(settings)
}

#[command]
async fn update_address(
    app: AppHandle,
    cache_state: tauri::State<'_, cache::CacheState>,
    index: usize,
    address: AddressEntry,
) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    let mut settings = load_settings_from_path(&path)?.unwrap_or_default();

    if index >= settings.addresses.len() {
        return Err("Invalid address index".to_string());
    }

    settings.addresses[index] = address;
    save_settings_to_path(&path, &settings)?;
    cache_state.clear();
    Ok(settings)
}


fn get_providers() -> Vec<Box<dyn AlertProvider>> {
    vec![
        Box::new(tauron::TauronProvider),
        Box::new(mpwik::MpwikProvider),
        Box::new(fortum::FortumProvider),
        Box::new(energa::EnergaProvider),
        Box::new(enea::EneaProvider),
        Box::new(pge::PgeProvider),
        Box::new(stoen::StoenProvider),
        Box::new(psg::PsgProvider),
    ]
}

#[command]
async fn fetch_all_alerts(
    app: AppHandle,
    db_state: tauri::State<'_, DbState>,
    cache_state: tauri::State<'_, cache::CacheState>,
    sources: Option<Vec<String>>,
) -> Result<Vec<UnifiedAlert>, String> {
    // 1. Check Cache (only on full refresh)
    if sources.is_none() {
        if let Some(cached) = cache_state.get() {
            log::info!("Serving fetch_all_alerts from cache ({} items)", cached.len());
            return Ok(cached);
        }
    }
    let path = settings_path(&app)?;
    let settings = load_settings_from_path(&path)?;
    let settings_orig = settings.clone();

    let mut all_alerts: Vec<UnifiedAlert> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    let mut enabled_sources = settings
        .as_ref()
        .and_then(|s| s.enabled_sources.clone())
        .unwrap_or_default();

    if let Some(ref requested) = sources {
        enabled_sources.retain(|s| requested.contains(s));
    }

    log::info!(
        "fetch_all_alerts: enabled_sources={:?}, addresses={}",
        enabled_sources,
        settings.as_ref().map(|s| s.addresses.len()).unwrap_or(0)
    );

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
    let mut tasks = Vec::new();

    if let Some(s) = settings {
        let s_arc = Arc::new(s.clone());
        let providers = get_providers();
        let net_state = app.state::<NetworkState>();
        let client = net_state.get_client().await?;
        let client_http1 = net_state.get_client_http1().await?;

        for provider in providers {
            if !enabled_sources.contains(&provider.id()) {
                continue;
            }

            let s_p = Arc::clone(&s_arc);
            let sem = semaphore.clone();
            let c = client.clone();
            let c_h1 = client_http1.clone();
            let app_h = app.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                provider.fetch(&c, &c_h1, &s_p, Some(&app_h)).await
            }));
        }

        let results = join_all(tasks).await;

        for res in results {
            match res {
                Ok((mut alerts, errs)) => {
                    for alert in &mut alerts {
                        alert.hash = Some(alert.to_hash());
                    }
                    all_alerts.extend(alerts);
                    errors.extend(errs);
                }
                Err(e) => errors.push(format!("Task execution error: {}", e)),
            }
        }

        // --- DEDUPLICATE BY HASH (Smart Merging) ---
        all_alerts = api_logic::deduplicate_alerts(all_alerts);

        // --- SORT BY DATE (ASCENDING) ---
        all_alerts.sort_by(|a, b| {
            let date_cmp = match (&a.startDate, &b.startDate) {
                (Some(da), Some(db)) => da.cmp(db),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            if date_cmp != std::cmp::Ordering::Equal {
                return date_cmp;
            }
            // Stability fallback: sort by source name
            a.source.to_string().cmp(&b.source.to_string())
        });

        // --- PROCESS NEW ALERTS AND NOTIFY ---
        let db_adapter = RealDatabase(&db_state.conn);
        let notifier = RealNotification(&app);
        let engine = MonitorEngine::new(&db_adapter, &notifier, &s);
        engine.process_alerts(all_alerts.clone());
    }

    if all_alerts.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }

    // Final filter to ensure no alerts from disabled addresses/cities slip through
    if let Some(ref s) = settings_orig {
        all_alerts.retain(|alert| {
            if let Some(idx) = alert.address_index {
                if idx < s.addresses.len() {
                    return s.addresses[idx].is_active;
                }
            }
            
            // For general city alerts
            if alert.is_local == Some(false) {
                if let Some(desc) = &alert.description {
                    if desc.contains("Wrocław") {
                        return s.addresses.iter().any(|a| a.is_active && is_wroclaw(a));
                    }
                    if desc.contains("Warszawa") {
                        return s.addresses.iter().any(|a| a.is_active && is_warszawa(a));
                    }
                    // For other cities (dynamic check based on active addresses)
                    for addr in s.addresses.iter().filter(|a| a.is_active) {
                        if desc.contains(&addr.city_name) {
                            return true;
                        }
                    }
                    return false; // Skip if no active address in this city
                }
            }

            true
        });
    }

    if sources.is_none() {
        cache_state.set(all_alerts.clone());
    }

    Ok(all_alerts)
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_state::NetworkState;
    use crate::enea::CompiledEneaRegex;

    #[tokio::test]
    async fn test_fetch_enea_outages_real_backend() {
        let client = NetworkState::build_client().unwrap();
        let items = enea::fetch_all_enea_outages(&client, &[7]).await.unwrap();
        
        let compiled = CompiledEneaRegex::new("Kicin", "Poznańska", &None);
        let kicin_items: Vec<_> = items.into_iter()
            .filter(|i| i.matches_address_compiled(&compiled))
            .collect();
            
        println!("Found Kicin / Poznańska items: {}", kicin_items.len());
        // assert!(!kicin_items.is_empty()); // Might be empty depending on current outages

        if !kicin_items.is_empty() {
            let unified = kicin_items[0].to_unified();
            println!("Unified structure: {:?}", unified);
        }
    }
}



#[command]
async fn teryt_city_has_streets(app: AppHandle, city_id: u64) -> Result<bool, String> {
    teryt::city_has_streets(&app, city_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let conn = state_db::init_db(app.handle())?;
            state_db::prune_old_alerts(&conn, 30)?;
            app.manage(DbState { conn: Mutex::new(conn) });
            app.manage(cache::CacheState::new());

            #[cfg(target_os = "android")]
            {
                use tauri::Manager;
                log::info!("Checking for webview windows to initialize rustls-platform-verifier...");
                
                // Fallback: If we can't initialize here, NetworkState::new() might panic if it uses rustls immediately.
                // However, we now initialize it in MainActivity.onCreate() which is called very early.
                
                let windows = app.webview_windows();
                for (label, window) in windows {
                    log::info!("Initializing verifier for window during setup: {}", label);
                    let _ = window.with_webview(|webview| {
                        webview.jni_handle().exec(|env, context, _webview| {
                            ensure_verifier_initialized(env, context);
                        });
                    });
                }
            }

            app.manage(network_state::NetworkState::new()?);

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            fetch_all_alerts,
            teryt_lookup_city,
            teryt_lookup_street,
            teryt_city_has_streets,
            save_settings,
            load_settings,
            add_address,
            remove_address,
            set_primary_address,
            update_address,
            get_app_version
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ── Android JNI exports ───────────────────────────────────

#[cfg(target_os = "android")]
fn ensure_verifier_initialized(env: &mut JNIEnv, context: &JObject) {
    static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if INITIALIZED.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    android_logger::init_once(
        android_logger::Config::default().with_tag("AWARIA_RUST").with_max_level(log::LevelFilter::Info),
    );

    log::info!("Attempting to initialize rustls-platform-verifier...");
    let class_loader = match env.call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[]) {
        Ok(r) => r.l().expect("ClassLoader is not an object"),
        Err(e) => {
            log::error!("Failed to get ClassLoader: {:?}", e);
            return;
        }
    };

    let vm = match env.get_java_vm() {
        Ok(vm) => vm,
        Err(e) => {
            log::error!("Failed to get JavaVM: {:?}", e);
            return;
        }
    };

    let context_ref = match env.new_global_ref(context) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create global ref for context: {:?}", e);
            return;
        }
    };

    let loader_ref = match env.new_global_ref(class_loader) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create global ref for loader: {:?}", e);
            return;
        }
    };

    log::info!("Calling rustls_platform_verifier::android::init_with_refs...");
    rustls_platform_verifier::android::init_with_refs(vm, context_ref, loader_ref);
    INITIALIZED.store(true, std::sync::atomic::Ordering::SeqCst);
    log::info!("rustls-platform-verifier initialized successfully.");
}

#[cfg(target_os = "android")]
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn Java_xyz_eremef_awaria_WidgetUtils_fetchCountFromRust(
    mut env: JNIEnv,
    _class: JClass,
    context: JObject,
    provider_id: JString,
    settings_json: JString,
) -> jint {
    ensure_verifier_initialized(&mut env, &context);
    let provider_id: String = match env.get_string(&provider_id) {
        Ok(s) => s.into(),
        Err(_) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", "Invalid providerId");
            return -1;
        }
    };

    let settings_str: String = match env.get_string(&settings_json) {
        Ok(s) => s.into(),
        Err(_) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", "Invalid settings JSON");
            return -1;
        }
    };

    let settings: Settings = match serde_json::from_str(&settings_str) {
        Ok(s) => s,
        Err(e) => {
            let _ = env.throw_new("java/lang/IllegalArgumentException", format!("JSON parse error: {}", e));
            return -1;
        }
    };

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = env.throw_new("java/lang/RuntimeException", format!("Tokio runtime error: {}", e));
                return -1;
            }
        };

    rt.block_on(async {
        let client = match network_state::NetworkState::build_client() {
            Ok(c) => c,
            Err(e) => {
                let _ = env.throw_new("java/io/IOException", format!("Failed to build HTTP client: {}", e));
                return -1;
            }
        };
        let client_http1 = match network_state::NetworkState::build_client_http1() {
            Ok(c) => c,
            Err(e) => {
                let _ = env.throw_new("java/io/IOException", format!("Failed to build HTTP/1.1 client: {}", e));
                return -1;
            }
        };

        let providers = get_providers();
        let provider = providers.iter().find(|p| p.id() == provider_id);

        match provider {
            Some(p) => {
                let (mut alerts, errors) = p.fetch(&client, &client_http1, &settings, None).await;
                if alerts.is_empty() && !errors.is_empty() {
                    let _ = env.throw_new("java/io/IOException", errors.join("; "));
                    return -1;
                }
                
                let now = chrono::Utc::now();
                
                // 1. Assign hashes and filter out expired (past) outages
                alerts.retain(|alert| {
                    if let Some(end_str) = &alert.endDate {
                        if let Some(end_dt) = utils::parse_date(end_str) {
                            return end_dt >= now;
                        }
                    }
                    true
                });

                // Deduplicate
                let grouped_alerts = api_logic::deduplicate_alerts(alerts);

                // --- COUNT LOCAL ALERTS ---
                let mut count = 0;
                for alert in &grouped_alerts {
                    if alert.is_local == Some(true) {
                        count += 1;
                    }
                }
                count as jint
            }
            None => {
                let _ = env.throw_new("java/lang/IllegalArgumentException", format!("Unknown provider: {}", provider_id));
                -1
            }
        }
    })
}

#[cfg(target_os = "android")]
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn Java_xyz_eremef_awaria_WidgetUtils_initVerifier(
    mut env: JNIEnv,
    _class: JClass,
    context: JObject,
) {
    ensure_verifier_initialized(&mut env, &context);
}
