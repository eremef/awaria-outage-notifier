mod api_logic;
mod enea;
mod energa;
mod fortum;
mod mpwik;
mod pge;
mod network_state;
mod state_db;
mod stoen;
mod tauron;
mod teryt;
mod utils;
mod cache;
mod psg;
mod wmk;
mod tauron_heat;

use crate::network_state::NetworkState;
use api_logic::{
    load_settings_from_path, save_settings_to_path,
    AddressEntry, Settings, UnifiedAlert,
    AlertProvider,
    is_wroclaw, is_warszawa, is_krakow,
};
use tauri::command;
use tauri::AppHandle;
use tauri::Manager;
// use tauri::Url; // unused
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_dialog::DialogExt;
// share extension is not used as we call the plugin directly
use api_logic::{DatabaseInterface, NotificationProvider, MonitorEngine};

#[cfg(target_os = "android")]
use jni::{
    objects::{Global, JClass, JObject, JString},
    sys::{jint, jstring},
    Env, EnvUnowned,
};

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use crate::state_db::DbState;
use futures::future::join_all;
use teryt::{TerytCity, TerytStreet};


const MAX_CONCURRENT_REQUESTS: usize = 5;

#[cfg(target_os = "android")]
static ANDROID_CONTEXT: Mutex<Option<std::sync::Arc<Global<JObject<'static>>>>> = Mutex::new(None);
#[cfg(target_os = "android")]
static PSG_FETCHER_CLASS: Mutex<Option<std::sync::Arc<Global<JClass<'static>>>>> = Mutex::new(None);
#[cfg(target_os = "android")]
static WIDGET_UTILS_CLASS: Mutex<Option<std::sync::Arc<Global<JClass<'static>>>>> = Mutex::new(None);
#[cfg(target_os = "android")]
static JAVA_VM: Mutex<Option<jni::JavaVM>> = Mutex::new(None);

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
async fn is_battery_optimization_ignored(_app: AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        let vm_guard = JAVA_VM.lock().unwrap();
        if let Some(vm) = &*vm_guard {
            return vm.attach_current_thread(|env| {
                let context_guard = ANDROID_CONTEXT.lock().unwrap();
                let context_ref = context_guard.as_ref().ok_or_else(|| jni::errors::Error::JavaException)?;
                
                let class_guard = WIDGET_UTILS_CLASS.lock().unwrap();
                let class_ref = class_guard.as_ref().ok_or_else(|| jni::errors::Error::JavaException)?;
                let class_obj = class_ref.as_obj();
                let class_local = env.new_local_ref(class_obj)?;

                let result = env.call_static_method(
                    unsafe { jni::objects::JClass::from_raw(env, class_local.as_raw()) },
                    jni::jni_str!("isIgnoringBatteryOptimizations"),
                    jni::jni_sig!("(Landroid/content/Context;)Z"),
                    &[jni::objects::JValue::Object(context_ref.as_obj())],
                )?;
                let ignored = result.z()?;
                log::info!("Battery optimization check: ignored={}", ignored);
                Ok(ignored)
            }).map_err(|e: jni::errors::Error| {
                log::error!("Battery optimization check failed: {:?}", e);
                e.to_string()
            });
        }
        log::warn!("Battery optimization check: JAVA_VM not available, returning false fallback");
        return Ok(false);
    }

    #[cfg(not(target_os = "android"))]
    Ok(true)
}

#[command]
async fn request_battery_optimization_ignore(_app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        log::info!("request_battery_optimization_ignore command called");
        let vm_guard = JAVA_VM.lock().unwrap();
        if let Some(vm) = &*vm_guard {
            return vm.attach_current_thread(|env| {
                let context_guard = ANDROID_CONTEXT.lock().unwrap();
                let context_ref = context_guard.as_ref().ok_or_else(|| jni::errors::Error::JavaException)?;
                
                let class_guard = WIDGET_UTILS_CLASS.lock().unwrap();
                let class_ref = class_guard.as_ref().ok_or_else(|| jni::errors::Error::JavaException)?;
                let class_obj = class_ref.as_obj();
                let class_local = env.new_local_ref(class_obj)?;

                env.call_static_method(
                    unsafe { jni::objects::JClass::from_raw(env, class_local.as_raw()) },
                    jni::jni_str!("requestIgnoreBatteryOptimizations"),
                    jni::jni_sig!("(Landroid/content/Context;)V"),
                    &[jni::objects::JValue::Object(context_ref.as_obj())],
                )?;
                Ok(())
            }).map_err(|e: jni::errors::Error| {
                log::error!("Battery optimization request failed: {:?}", e);
                e.to_string()
            });
        }
        return Err("JAVA_VM not initialized".to_string());
    }

    #[cfg(not(target_os = "android"))]
    Ok(())
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

#[command]
async fn export_settings(app: AppHandle) -> Result<String, String> {
    let settings_path = app.path().app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");

    #[cfg(target_os = "android")]
    {
        log::info!("Export (Android): triggering export for {:?}", settings_path);
        
        let ctx_guard = ANDROID_CONTEXT.lock().unwrap();
        if let Some(ctx) = ctx_guard.as_ref() {
            let vm_guard = JAVA_VM.lock().unwrap();
            let wu_class_guard = WIDGET_UTILS_CLASS.lock().unwrap();
            if let (Some(vm), Some(wu_class)) = (vm_guard.as_ref(), wu_class_guard.as_ref()) {
                let msg = vm.attach_current_thread(|env| {
                    let path_jstring = env.new_string(settings_path.to_string_lossy())?;
                    let name_jstring = env.new_string("settings.json")?;
                    
                    let result = env.call_static_method(
                        &**wu_class,
                        jni::jni_str!("exportSettings"),
                        jni::jni_sig!("(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"),
                        &[
                            jni::objects::JValue::Object(ctx.as_obj()),
                            jni::objects::JValue::Object(path_jstring.as_ref()),
                            jni::objects::JValue::Object(name_jstring.as_ref()),
                        ],
                    )?;
                    
                    let msg_obj = result.l()?;
                    let msg_rust = if !msg_obj.is_null() {
                        let msg_jstr = unsafe { jni::objects::JString::from_raw(env, msg_obj.as_raw() as jstring) };
                        #[allow(deprecated)]
                        env.get_string(&msg_jstr).map(|s| s.into()).unwrap_or_else(|_| "Export successful (could not parse path)".to_string())
                    } else {
                        "Export failed (null response)".to_string()
                    };
                    
                    log::info!("Export (Android): JNI export success: {}", msg_rust);
                    Ok(msg_rust)
                }).map_err(|e: jni::errors::Error| {
                    log::error!("Export (Android): JNI export failed: {}", e);
                    e.to_string()
                })?;
                return Ok(msg);
            }
        }
        
        log::error!("Export (Android): Android context or VM not initialized");
        return Err("Android environment not ready".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_dialog::DialogExt;
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        app.dialog()
            .file()
            .set_title("Export Settings")
            .set_file_name("settings.json")
            .add_filter("JSON", &["json"])
            .save_file(move |path| {
                let _ = tx.send(path);
            });

        if let Some(path) = rx.await.map_err(|e| e.to_string())? {
            let dest_path = path.into_path().map_err(|e| e.to_string())?;
            std::fs::copy(&settings_path, &dest_path).map_err(|e| e.to_string())?;
            return Ok(format!("Saved to {:?}", dest_path));
        }
        Err("cancel".to_string())
    }
}

#[command]
async fn import_settings(app: tauri::AppHandle<tauri::Wry>, cache_state: tauri::State<'_, cache::CacheState>) -> Result<Option<Settings>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();

    log::info!("Import: triggering pick_file dialog...");
    app.dialog()
        .file()
        .set_title("Import Settings")
        .add_filter("JSON", &["json"])
        .pick_file(move |path| {
            let _ = tx.send(path);
        });

    let file_path = rx.await.map_err(|e| {
        log::error!("Import: channel receive failed: {}", e);
        e.to_string()
    })?;

    if let Some(path) = file_path {
        
        #[cfg(target_os = "android")]
        let json = {
            // On Android, we might get a content:// URI
            let is_content = match &path {
                tauri_plugin_dialog::FilePath::Url(u) => u.as_str().starts_with("content:"),
                _ => false,
            };
            
            if is_content {
                let url_str = match &path {
                    tauri_plugin_dialog::FilePath::Url(u) => u.to_string(),
                    _ => unreachable!(),
                };
                
                let ctx_guard = ANDROID_CONTEXT.lock().unwrap();
                let vm_guard = JAVA_VM.lock().unwrap();
                let wu_class_guard = WIDGET_UTILS_CLASS.lock().unwrap();
                
                if let (Some(ctx), Some(vm), Some(wu_class)) = (ctx_guard.as_ref(), vm_guard.as_ref(), wu_class_guard.as_ref()) {
                    let content = vm.attach_current_thread(|env| {
                        let url_j = env.new_string(&url_str)?;
                        let result = env.call_static_method(
                            &**wu_class,
                            jni::jni_str!("readUri"),
                            jni::jni_sig!("(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;"),
                            &[
                                jni::objects::JValue::Object(ctx.as_obj()),
                                jni::objects::JValue::Object(url_j.as_ref()),
                            ],
                        )?;
                        
                        let msg_obj = result.l()?;
                        if msg_obj.is_null() {
                            return Ok("".to_string());
                        }
                        let msg_jstr = unsafe { jni::objects::JString::from_raw(env, msg_obj.as_raw() as jstring) };
                        #[allow(deprecated)]
                        let msg_rust: String = env.get_string(&msg_jstr).map(|s| s.into()).unwrap_or_default();
                        Ok(msg_rust)
                    }).map_err(|e: jni::errors::Error| {
                        log::error!("Import (Android): JNI readUri failed: {}", e);
                        e.to_string()
                    })?;
                    
                    if content.is_empty() {
                        return Err("Failed to read settings file (empty content)".to_string());
                    }
                    content
                } else {
                    return Err("Android environment not ready for import".to_string());
                }
            } else {
                // Not a content URI, try normal path
                let path_buf = path.as_path().ok_or_else(|| "Invalid path".to_string())?.to_path_buf();
                fs::read_to_string(&path_buf).map_err(|e| e.to_string())?
            }
        };
        
        #[cfg(not(target_os = "android"))]
        let json = {
            let path_buf = path.as_path().ok_or_else(|| "Invalid path".to_string())?.to_path_buf();
            fs::read_to_string(&path_buf).map_err(|e| e.to_string())?
        };
        
        let settings: Settings = serde_json::from_str(&json).map_err(|e| {
            log::error!("Import: JSON deserialization failed: {}", e);
            format!("Invalid settings file: {}", e)
        })?;
        
        let path = settings_path(&app).map_err(|e| {
            log::error!("Import: settings_path resolution failed: {}", e);
            e
        })?;
        
        save_settings_to_path(&path, &settings).map_err(|e| {
            log::error!("Import: save_settings_to_path failed: {}", e);
            e
        })?;
        
        cache_state.clear();
        log::info!("Import: success!");
        return Ok(Some(settings));
    }
    
    log::info!("Import: user cancelled dialog");
    Ok(None)
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
        Box::new(wmk::WmkProvider),
        Box::new(tauron_heat::TauronHeatProvider),
    ]
}

async fn fetch_all_alerts_internal(
    app: &AppHandle,
    sources: Option<Vec<String>>,
) -> Result<api_logic::AlertsResponse, String> {
    let db_state = app.state::<DbState>();
    let cache_state = app.state::<cache::CacheState>();

    // 1. Check Cache (only on full refresh)
    if sources.is_none() {
        if let Some(cached) = cache_state.get() {
            log::info!("Serving fetch_all_alerts from cache ({} items)", cached.len());
            return Ok(api_logic::AlertsResponse { alerts: cached, is_stale: false });
        }
    }
    let path = settings_path(app)?;
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

            // --- VOIVODESHIP PREFILTRATION ---
            if !api_logic::is_provider_applicable(provider.source(), &s_arc) {
                log::info!("fetch_all_alerts: skipping {}, not applicable for active addresses", provider.id());
                continue;
            }

            let s_p = Arc::clone(&s_arc);
            let sem = semaphore.clone();
            let c = client.clone();
            let c_h1 = client_http1.clone();
            let app_h = app.clone();
            tasks.push(tauri::async_runtime::spawn(async move {
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
        let notifier = RealNotification(app);
        let engine = MonitorEngine::new(&db_adapter, &notifier, &s);
        engine.process_alerts(all_alerts.clone());
    }

    if all_alerts.is_empty() && !errors.is_empty() {
        if let Some(cached) = cache_state.get_stale() {
            log::warn!("Fetch failed ({} errors), falling back to stale cache ({} items)", errors.len(), cached.len());
            return Ok(api_logic::AlertsResponse { alerts: cached, is_stale: true });
        }
        return Err("ERR_NO_INTERNET".to_string());
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
                    if desc.contains("Kraków") {
                        return s.addresses.iter().any(|a| a.is_active && is_krakow(a));
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

    Ok(api_logic::AlertsResponse { alerts: all_alerts, is_stale: false })
}

#[tauri::command]
async fn fetch_all_alerts(
    app: AppHandle,
    sources: Option<Vec<String>>,
) -> Result<api_logic::AlertsResponse, String> {
    fetch_all_alerts_internal(&app, sources).await
}

#[cfg(not(mobile))]
fn start_background_monitoring(app: AppHandle) {
    log::info!("Starting background monitoring task (interval: 30 minutes)");
    tauri::async_runtime::spawn(async move {
        // Use a slightly offset interval to avoid hitting everything exactly at once on startup
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30 * 60));
        loop {
            interval.tick().await;
            log::info!("Background monitoring: starting fetch cycle...");
            match fetch_all_alerts_internal(&app, None).await {
                Ok(response) => log::info!("Background monitoring: cycle completed successfully ({} alerts found).", response.alerts.len()),
                Err(e) => log::error!("Background monitoring: cycle failed: {}", e),
            }
        }
    });
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
    let _ = rustls::crypto::ring::default_provider().install_default();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let conn = state_db::init_db(app.handle())?;
            state_db::prune_old_alerts(&conn, 30)?;
            app.manage(DbState { conn: Mutex::new(conn) });
            app.manage(cache::CacheState::new());

            app.manage(network_state::NetworkState::new()?);

            if cfg!(debug_assertions) || cfg!(target_os = "android") {
                let _ = app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                );
            }

            #[cfg(not(mobile))]
            start_background_monitoring(app.handle().clone());

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
            get_app_version,
            is_battery_optimization_ignored,
            request_battery_optimization_ignore,
            export_settings,
            import_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ── Android JNI exports ───────────────────────────────────

#[cfg(target_os = "android")]
fn ensure_verifier_initialized(env: &mut Env, context: &JObject) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if INITIALIZED.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    log::info!("Attempting to initialize rustls-platform-verifier and android_logger...");
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("xyz.eremef.awaria"),
    );
    
    let class_loader: JObject = match env.call_method(
        &context, 
        jni::jni_str!("getClassLoader"), 
        jni::jni_sig!("()Ljava/lang/ClassLoader;"), 
        &[]
    ) {
        Ok(r) => r.l().expect("ClassLoader is not an object"),
        Err(e) => {
            log::error!("Failed to get ClassLoader: {:?}", e);
            return;
        }
    };
    
    let class_loader_wrapped = unsafe { jni::objects::JClassLoader::from_raw(env, class_loader.as_raw()) };

    let vm = match env.get_java_vm() {
        Ok(vm) => {
            if let Ok(mut g_vm) = JAVA_VM.lock() {
                *g_vm = Some(vm.clone());
            }
            vm
        },
        Err(e) => {
            log::error!("Failed to get JavaVM: {:?}", e);
            return;
        }
    };

    let context_ref = match env.new_global_ref(&context) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create global ref for context: {:?}", e);
            return;
        }
    };
    
    let context_clone = match env.new_global_ref(&context) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create global ref for context clone: {:?}", e);
            return;
        }
    };

    let loader_ref = match env.new_global_ref(class_loader_wrapped) {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to create global ref for loader: {:?}", e);
            return;
        }
    };

    log::info!("Calling rustls_platform_verifier::android::init_with_refs...");
    
    if let Ok(mut g_ctx) = ANDROID_CONTEXT.lock() {
        *g_ctx = Some(std::sync::Arc::new(context_clone));
    }

    // Also find and cache our PsgWebViewFetcher class
    log::info!("Caching PsgWebViewFetcher class...");
    match env.find_class(jni::jni_str!("xyz/eremef/awaria/PsgWebViewFetcher")) {
        Ok(cls) => {
            if let Ok(cls_ref) = env.new_global_ref(cls) {
                if let Ok(mut g_psg) = PSG_FETCHER_CLASS.lock() {
                    *g_psg = Some(std::sync::Arc::new(cls_ref));
                    log::info!("PsgWebViewFetcher class cached successfully.");
                }
            }
        }
        Err(e) => {
            log::error!("Failed to find PsgWebViewFetcher class: {:?}", e);
        }
    }

    log::info!("Caching WidgetUtils class...");
    match env.find_class(jni::jni_str!("xyz/eremef/awaria/WidgetUtils")) {
        Ok(cls) => {
            if let Ok(cls_ref) = env.new_global_ref(cls) {
                if let Ok(mut g_wu) = WIDGET_UTILS_CLASS.lock() {
                    *g_wu = Some(std::sync::Arc::new(cls_ref));
                    log::info!("WidgetUtils class cached successfully.");
                }
            }
        }
        Err(e) => {
            log::error!("Failed to find WidgetUtils class: {:?}", e);
        }
    }

    rustls_platform_verifier::android::init_with_refs(vm, context_ref, loader_ref);
    INITIALIZED.store(true, std::sync::atomic::Ordering::SeqCst);
    log::info!("rustls-platform-verifier initialized successfully.");
}

#[cfg(target_os = "android")]
#[allow(non_snake_case, deprecated)]
#[no_mangle]
pub extern "C" fn Java_xyz_eremef_awaria_WidgetUtils_fetchCountFromRust(
    mut native_env: EnvUnowned,
    _class: JClass,
    context: JObject,
    provider_id: JString,
    settings_json: JString,
) -> jint {
    let final_count = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(-1));
    let final_count_clone = final_count.clone();

    let _ = native_env.with_env(move |env| {
        ensure_verifier_initialized(env, &context);
        
        #[allow(deprecated)]
        let provider_id: String = env.get_string(&provider_id).map(|s| s.into()).unwrap_or_default();
        #[allow(deprecated)]
        let settings_str: String = env.get_string(&settings_json).map(|s| s.into()).unwrap_or_default();

        let settings: Settings = match serde_json::from_str(&settings_str) {
            Ok(s) => s,
            Err(_) => return Ok::<_, jni::errors::Error>(()),
        };

        let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(_) => return Ok::<_, jni::errors::Error>(()),
        };

        let count = rt.block_on(async {
            let client = match network_state::NetworkState::build_client() {
                Ok(c) => c,
                Err(_) => return -1,
            };
            let client_http1 = match network_state::NetworkState::build_client_http1() {
                Ok(c) => c,
                Err(_) => return -1,
            };

            let providers = get_providers();
            let provider = providers.iter().find(|p| p.id() == provider_id);

            match provider {
                Some(p) => {
                    if !api_logic::is_provider_applicable(p.source(), &settings) {
                        return 0;
                    }
                    let (mut alerts, _) = p.fetch(&client, &client_http1, &settings, None).await;
                    let now = chrono::Utc::now();
                    alerts.retain(|alert| {
                        if let Some(end_str) = &alert.endDate {
                            if let Some(end_dt) = utils::parse_date(end_str) {
                                return end_dt >= now;
                            }
                        }
                        true
                    });
                    let grouped = api_logic::deduplicate_alerts(alerts);
                    grouped.iter().filter(|a| a.is_local == Some(true)).count() as jint
                }
                None => -1
            }
        });
        
        final_count_clone.store(count, std::sync::atomic::Ordering::Relaxed);
        Ok::<_, jni::errors::Error>(())
    });
    
    final_count.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(target_os = "android")]
struct JniNotification;

#[cfg(target_os = "android")]
impl NotificationProvider for JniNotification {
    fn show_notification(&self, title: String, body: String, hash: String) {
        let context_guard = ANDROID_CONTEXT.lock().unwrap();
        let context_ref = match &*context_guard {
            Some(r) => r,
            None => {
                log::error!("JniNotification: ANDROID_CONTEXT is None");
                return;
            }
        };

        let vm_guard = JAVA_VM.lock().unwrap();
        let vm = match &*vm_guard {
            Some(v) => v,
            None => {
                log::error!("JniNotification: JAVA_VM is None");
                return;
            }
        };

        let res = vm.attach_current_thread(|env| {
            let title_j = env.new_string(&title).unwrap();
            let body_j = env.new_string(&body).unwrap();
            let hash_j = env.new_string(&hash).unwrap();
            
            log::info!("JniNotification: Calling WidgetUtils.showNotification via JNI...");
            let res = env.call_static_method(
                jni::jni_str!("xyz/eremef/awaria/WidgetUtils"),
                jni::jni_str!("showNotification"),
                jni::jni_sig!("(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)V"),
                &[
                    jni::objects::JValue::Object(context_ref.as_obj()),
                    jni::objects::JValue::Object(&title_j.into()),
                    jni::objects::JValue::Object(&body_j.into()),
                    jni::objects::JValue::Object(&hash_j.into()),
                ],
            );
            if let Err(e) = res {
                log::error!("JniNotification: JNI call failed: {:?}", e);
            }
            Ok::<(), jni::errors::Error>(())
        });
        if let Err(e) = res {
            log::error!("JniNotification: Failed to attach thread: {:?}", e);
        }
    }
}

#[cfg(target_os = "android")]
#[allow(non_snake_case, deprecated)]
#[no_mangle]
pub extern "C" fn Java_xyz_eremef_awaria_WidgetUtils_fetchAndNotifyFromRust(
    mut native_env: EnvUnowned,
    _class: JClass,
    context: JObject,
    settings_json: JString,
) {
    let _ = native_env.with_env(|env| {
        ensure_verifier_initialized(env, &context);
        
        #[allow(deprecated)]
        let settings_str: String = env.get_string(&settings_json).map(|s| s.into()).unwrap_or_default();
        if settings_str.is_empty() { return Ok::<(), jni::errors::Error>(()); }

        let settings: Settings = match serde_json::from_str(&settings_str) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Background monitoring: failed to deserialize settings JSON: {}. JSON prefix: {}", e, &settings_str[..settings_str.len().min(100)]);
                return Ok(());
            }
        };

        let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(_) => return Ok(()),
        };

        rt.block_on(async {
            let client = match network_state::NetworkState::build_client() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Background monitoring: failed to build client: {}", e);
                    return;
                }
            };
            let client_http1 = match network_state::NetworkState::build_client_http1() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Background monitoring: failed to build http1 client: {}", e);
                    return;
                }
            };

            let providers = get_providers();
            let enabled_sources = settings.enabled_sources.clone().unwrap_or_default();
            
            let files_dir: JObject = match env.call_method(&context, jni::jni_str!("getFilesDir"), jni::jni_sig!("()Ljava/io/File;"), &[]) {
                Ok(v) => v.l().expect("getFilesDir returned non-object"),
                Err(e) => {
                    log::error!("Background monitoring: failed to call getFilesDir: {:?}", e);
                    return;
                },
            };
            let path_j: JObject = match env.call_method(&files_dir, jni::jni_str!("getAbsolutePath"), jni::jni_sig!("()Ljava/lang/String;"), &[]) {
                Ok(v) => v.l().expect("getAbsolutePath returned non-object"),
                Err(e) => {
                    log::error!("Background monitoring: failed to call getAbsolutePath: {:?}", e);
                    return;
                },
            };
            let path_jstr = unsafe { jni::objects::JString::from_raw(env, path_j.as_raw()) };
            #[allow(deprecated)]
            let path_str: String = env.get_string(&path_jstr).map(|s| s.into()).unwrap_or_default();
            let files_dir_path = std::path::PathBuf::from(path_str);
            
            // Standard Tauri v2 path on Android is <files_dir>/<identifier>/state.db
            let tauri_path = state_db::get_db_path_from_files_dir(files_dir_path.clone());
            // Legacy path used by previous versions of background monitor was <files_dir>/state.db
            let legacy_path = files_dir_path.join(state_db::STATE_DB_NAME);
            
            let db_path = if tauri_path.exists() {
                tauri_path
            } else if legacy_path.exists() {
                log::info!("Background monitoring: found legacy state.db at {:?}. Migrating to {:?}", legacy_path, tauri_path);
                if let Some(parent) = tauri_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                // Try to migrate
                match std::fs::rename(&legacy_path, &tauri_path) {
                    Ok(_) => tauri_path,
                    Err(e) => {
                        log::error!("Background monitoring: failed to migrate state.db: {}. Falling back to legacy path.", e);
                        legacy_path
                    }
                }
            } else {
                // New install or first run: ensure directory exists and use standard path
                if let Some(parent) = tauri_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                tauri_path
            };
            
            log::info!("Background monitoring: using database at {:?}", db_path);
            
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => {
                    let mut c = c;
                    let _ = state_db::ensure_schema(&mut c);
                    c
                },
                Err(e) => {
                    log::error!("Background monitoring: failed to open database at {:?}: {}", db_path, e);
                    return;
                },
            };
            
            let conn_mutex = Mutex::new(conn);
            let db_adapter = RealDatabase(&conn_mutex);
            let notifier = JniNotification;
            let engine = MonitorEngine::new(&db_adapter, &notifier, &settings);

            log::info!("Background monitoring (Rust): starting fetch tasks for {} providers. addresses={}, preferences={}", 
                enabled_sources.len(), 
                settings.addresses.len(),
                settings.notification_preferences.len()
            );

            let mut tasks = Vec::new();
            let s_arc = std::sync::Arc::new(settings.clone());

            for provider in providers {
                if !enabled_sources.contains(&provider.id()) {
                    continue;
                }
                if !api_logic::is_provider_applicable(provider.source(), &s_arc) {
                    continue;
                }
                let c = client.clone();
                let ch1 = client_http1.clone();
                let s = s_arc.clone();
                tasks.push(tokio::spawn(async move {
                    provider.fetch(&c, &ch1, &s, None).await
                }));
            }

            let results = futures::future::join_all(tasks).await;
            let mut all_alerts = Vec::new();
            for res in results {
                match res {
                    Ok((alerts, _)) => all_alerts.extend(alerts),
                    Err(e) => log::error!("Provider task panicked: {:?}", e),
                }
            }

            log::info!("Background monitoring (Rust): fetched {} raw alerts. Filtering expired...", all_alerts.len());
            
            // Filter out expired alerts
            let now = chrono::Utc::now();
            all_alerts.retain(|alert| {
                if let Some(end_str) = &alert.endDate {
                    if let Some(end_dt) = utils::parse_date(end_str) {
                        return end_dt >= now;
                    }
                }
                true
            });

            let deduplicated = api_logic::deduplicate_alerts(all_alerts);
            log::info!("Background monitoring (Rust): processing {} deduplicated alerts.", deduplicated.len());
            engine.process_alerts(deduplicated);
            log::info!("Background monitoring (Rust): monitoring cycle complete.");
        });
        Ok(())
    });
}

#[cfg(target_os = "android")]
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn Java_xyz_eremef_awaria_WidgetUtils_initVerifier(
    mut native_env: EnvUnowned,
    _class: JClass,
    context: JObject,
) {
    let _ = native_env.with_env(|env| {
        ensure_verifier_initialized(env, &context);
        Ok::<(), jni::errors::Error>(())
    });
}

#[cfg(target_os = "android")]
pub async fn get_psg_html_android() -> Result<String, String> {
    let context_ref = ANDROID_CONTEXT.lock().map_err(|_| "Android Context lock poisoned".to_string())?.clone().ok_or("Android Context not initialized")?;
    let class_ref = PSG_FETCHER_CLASS.lock().map_err(|_| "PsgWebViewFetcher lock poisoned".to_string())?.clone().ok_or("PsgWebViewFetcher class not cached")?;
    
    #[allow(deprecated)]
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let vm_guard = JAVA_VM.lock().unwrap();
        let vm = vm_guard.as_ref().unwrap();
        let res: Result<String, jni::errors::Error> = vm.attach_current_thread(|env| {
            let context = context_ref.as_obj();
            
            let result = env.call_static_method(
                &*class_ref,
                jni::jni_str!("fetchHtmlNative"),
                jni::jni_sig!("(Landroid/content/Context;)Ljava/lang/String;"),
                &[jni::objects::JValue::Object(context)]
            )?;
            
            let html_obj = result.l()?;
            if html_obj.is_null() {
                return Ok(String::new());
            }
            
            let html_jstr = unsafe { jni::objects::JString::from_raw(env, html_obj.as_raw() as jstring) };
            #[allow(deprecated)]
            let html: String = env.get_string(&html_jstr).map(|s| s.into()).unwrap_or_default();
            
            Ok(html)
        });
        
        match res {
            Ok(html) => {
                if html.is_empty() {
                    Err("Native PSG fetch returned null".to_string())
                } else {
                    Ok(html)
                }
            },
            Err(e) => Err(e.to_string())
        }
    }).await.map_err(|e: tokio::task::JoinError| e.to_string())?
}
