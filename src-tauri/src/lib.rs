#[cfg(all(test, target_os = "windows", target_env = "msvc"))]
#[link(
    name = "awaria_windows_test_manifest",
    kind = "static",
    modifiers = "+whole-archive"
)]
unsafe extern "C" {}

pub mod api_logic;
mod aquanet;
mod aqua_bielsko;
mod enea;
mod enea_cieplo;
mod eco_sa;
mod energa_cieplo_kaliskie;
mod energa;
mod fortum;
mod mpwik;
mod mpwik_warsaw;
mod pge;
mod pge_torun_heat;
mod pec_gliwice;
pub mod network_state;
mod state_db;
mod stoen;
mod tauron;
pub mod teryt;
pub mod utils;
mod cache;
mod psg;
mod wmk;
mod tauron_heat;
mod katowickie_wodociagi;
pub mod kpec_bydgoszcz;
mod veolia_warszawa;
mod veolia_poznan;
pub mod veolia_lodz;
pub mod zwik_lodz;
pub mod zwik_zielona_gora;
pub mod wodociagi_plockie;
pub mod wodociagi_kieleckie;
pub mod torunskie_wodociagi;
pub mod wodociagi_bialostockie;
pub mod pwik_kalisz;
pub mod pwik_czestochowa;
pub mod gdanskie_wodociagi;
pub mod gpec;
pub mod opec_gdynia;
pub mod pewik_gdynia;
pub mod puk_rokietnica;
pub mod sec;
pub mod lpec;
pub mod mpwik_lublin;
pub mod mpwik_rzeszow;
pub mod pwik_olsztyn;
pub mod pwik_gorzow;
pub mod pwik_gliwice;
pub mod wik_opole;
pub mod lpwik_legnica;
pub mod bytomskie_wodociagi;
pub mod dabrowskie_wodociagi;
pub mod mwik_bydgoszcz;
pub mod mwik_koszalin;
pub mod sosnowieckie_wodociagi;
pub mod tarnowskie_wodociagi;
pub mod zabrzanskie_wodociagi;
pub mod pwik_ruda_slaska;
pub mod pwik_rybnik;
pub mod mpec_rzeszow;
pub mod mpec_kielce;
pub mod epec_elblag;
pub mod wm_radom;

use crate::network_state::NetworkState;
use api_logic::{
    load_settings_from_path, save_settings_to_path,
    AddressEntry, Settings, UnifiedAlert,
    AlertProvider,
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


const MAX_CONCURRENT_API_REQUESTS: usize = 15;
const MAX_CONCURRENT_WEBVIEW_REQUESTS: usize = 3;

#[cfg(target_os = "android")]
static ANDROID_CONTEXT: Mutex<Option<std::sync::Arc<Global<JObject<'static>>>>> = Mutex::new(None);
#[cfg(target_os = "android")]
static PSG_FETCHER_CLASS: Mutex<Option<std::sync::Arc<Global<JClass<'static>>>>> = Mutex::new(None);
#[cfg(target_os = "android")]
static PWIK_KALISZ_FETCHER_CLASS: Mutex<Option<std::sync::Arc<Global<JClass<'static>>>>> = Mutex::new(None);
#[cfg(target_os = "android")]
static GPEC_FETCHER_CLASS: Mutex<Option<std::sync::Arc<Global<JClass<'static>>>>> = Mutex::new(None);
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

fn clear_all_caches(app: &AppHandle, cache_state: &cache::CacheState) {
    cache_state.clear();
    if let Some(db_state) = app.try_state::<DbState>()
        && let Ok(conn) = db_state.conn.lock() {
            let _ = state_db::set_kv(&conn, "alerts_cache", "");
        }
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
    clear_all_caches(&app, &cache_state);
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
    clear_all_caches(&app, &cache_state);
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
    clear_all_caches(&app, &cache_state);
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
    clear_all_caches(&app, &cache_state);
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
    clear_all_caches(&app, &cache_state);
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
        
        clear_all_caches(&app, &cache_state);
        log::info!("Import: success!");
        return Ok(Some(settings));
    }
    
    log::info!("Import: user cancelled dialog");
    Ok(None)
}


pub fn get_providers() -> Vec<Box<dyn AlertProvider>> {
    vec![
        Box::new(tauron::TauronProvider),
        Box::new(mpwik::MpwikProvider),
        Box::new(mpwik_warsaw::MpwikWarszawaProvider),
        Box::new(fortum::FortumProvider),
        Box::new(energa::EnergaProvider),
        Box::new(enea::EneaProvider),
        Box::new(enea_cieplo::EneaCieploProvider),
        Box::new(eco_sa::EcoSaProvider),
        Box::new(energa_cieplo_kaliskie::EnergaCieploKaliskieProvider),
        Box::new(pge::PgeProvider),
        Box::new(pge_torun_heat::PgeTorunHeatProvider),
        Box::new(stoen::StoenProvider),
        Box::new(psg::PsgProvider),
        Box::new(wmk::WmkProvider),
        Box::new(tauron_heat::TauronHeatProvider),
        Box::new(aquanet::AquanetProvider),
        Box::new(katowickie_wodociagi::KatowickieWodociagiProvider),
        Box::new(veolia_warszawa::VeoliaWarszawaProvider),
        Box::new(veolia_poznan::VeoliaPoznanProvider),
        Box::new(veolia_lodz::VeoliaLodzProvider),
        Box::new(zwik_lodz::ZwikLodzProvider),
        Box::new(zwik_zielona_gora::ZwikZielonaGoraProvider),
        Box::new(wodociagi_plockie::WodociagiPlockieProvider),
        Box::new(pewik_gdynia::PewikGdyniaProvider),
        Box::new(wodociagi_kieleckie::WodociagiKieleckieProvider),
        Box::new(torunskie_wodociagi::TorunskieWodociagiProvider),
        Box::new(wodociagi_bialostockie::WodociagiBialostockieProvider),
        Box::new(pwik_kalisz::PwikKaliszProvider),
        Box::new(pwik_czestochowa::PwikCzestochowaProvider),
        Box::new(gdanskie_wodociagi::GdanskieWodociagiProvider),
        Box::new(gpec::GpecProvider),
        Box::new(opec_gdynia::OpecGdyniaProvider),
        Box::new(kpec_bydgoszcz::KpecBydgoszczProvider),
        Box::new(puk_rokietnica::PukRokietnicaProvider),
        Box::new(sec::SecProvider),
        Box::new(lpec::LpecProvider),
        Box::new(mpwik_lublin::MpwikLublinProvider),
        Box::new(mpwik_rzeszow::MpwikRzeszowProvider),
        Box::new(pwik_olsztyn::PwikOlsztynProvider),
        Box::new(wik_opole::WikOpoleProvider),
        Box::new(lpwik_legnica::LpwikLegnicaProvider),
        Box::new(bytomskie_wodociagi::BytomskieWodociagiProvider),
        Box::new(dabrowskie_wodociagi::DabrowskieWodociagiProvider),
        Box::new(aqua_bielsko::AquaBielskoProvider),
        Box::new(pwik_gorzow::PwikGorzowProvider),
        Box::new(pwik_gliwice::PwikGliwiceProvider),
        Box::new(mwik_bydgoszcz::MwikBydgoszczProvider),
        Box::new(mwik_koszalin::MwikKoszalinProvider),
        Box::new(sosnowieckie_wodociagi::SosnowieckieWodociagiProvider),
        Box::new(tarnowskie_wodociagi::TarnowskieWodociagiProvider),
        Box::new(zabrzanskie_wodociagi::ZabrzanskieWodociagiProvider),
        Box::new(pwik_ruda_slaska::PwikRudaSlaskaProvider),
        Box::new(pwik_rybnik::PwikRybnikProvider),
        Box::new(mpec_rzeszow::MpecRzeszowProvider),
        Box::new(mpec_kielce::MpecKielceProvider),
        Box::new(epec_elblag::EpecElblagProvider),
        Box::new(pec_gliwice::PecGliwiceProvider),
        Box::new(wm_radom::WmRadomProvider),
    ]
}

fn torunskie_street_segment(location: &str, street: &str) -> Option<String> {
    let normalized_location = utils::clean_text(location);
    let normalized_street = utils::clean_text(utils::strip_street_prefixes(street));
    let last_street_word = normalized_street
        .split(|character: char| !character.is_alphanumeric())
        .rfind(|word| word.chars().count() > 2 && !word.chars().all(char::is_numeric))?;
    let street_word_index = normalized_location.find(last_street_word)?;
    let after_street_character = street_word_index + last_street_word.chars().count();
    let start_byte = location
        .char_indices()
        .nth(after_street_character)
        .map(|(byte_index, _)| byte_index)
        .unwrap_or(location.len());
    let tail = &location[start_byte..];
    let normalized_tail = utils::clean_text(tail);
    let next_street_start = [
        "ul.", "ul ", "ulica ", "al.", "al ", "aleja ", "pl.", "pl ", "plac ",
    ]
    .iter()
    .filter_map(|prefix| normalized_tail.find(prefix))
    .min();
    let end_byte = next_street_start
        .and_then(|character_index| {
            tail.char_indices()
                .nth(character_index)
                .map(|(index, _)| index)
        })
        .unwrap_or(tail.len());
    Some(tail[..end_byte].to_string())
}

fn torunskie_notice_matches_house(address: &api_logic::AddressEntry, location: &str) -> bool {
    let normalized_location = utils::clean_text(location);
    let street_names = utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );

    street_names
        .into_iter()
        .filter(|street| !street.trim().is_empty())
        .any(|street| {
            let street = utils::clean_text(utils::strip_street_prefixes(&street));
            let street_words: Vec<&str> = street
                .split(|character: char| !character.is_alphanumeric())
                .filter(|word| word.chars().count() > 2 && !word.chars().all(char::is_numeric))
                .collect();
            if street_words.is_empty() {
                return false;
            }
            let location_words: Vec<&str> = normalized_location
                .split(|character: char| !character.is_alphanumeric())
                .collect();
            let street_is_listed = street_words.iter().all(|word| {
                location_words
                    .iter()
                    .any(|location_word| location_word == word || location_word.starts_with(word))
            });
            if !street_is_listed {
                return false;
            }
        let Some(street_segment) = torunskie_street_segment(location, street.as_str()) else {
            return false;
        };
        if has_unlisted_house_suffix(&address.house_no, &street_segment) {
            return false;
        }
        utils::match_house_number(&address.house_no, &street_segment, None)
        })
}

fn has_unlisted_house_suffix(house_no: &str, street_segment: &str) -> bool {
    static HOUSE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let house_re = HOUSE_RE.get_or_init(|| regex::Regex::new(r"(?i)\d+\s*[a-z]?").unwrap());
    let requested = utils::clean_text(house_no).replace(' ', "");
    let requested_base: String = requested
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    if requested_base.is_empty()
        || !requested
            .chars()
            .any(|character| character.is_ascii_alphabetic())
    {
        return false;
    }

    let listed_houses: Vec<String> = house_re
        .find_iter(street_segment)
        .map(|value| utils::clean_text(value.as_str()).replace(' ', ""))
        .collect();
    if listed_houses.iter().any(|listed| listed == &requested) {
        return false;
    }

    listed_houses.iter().any(|listed| {
        let listed_base: String = listed
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect();
        listed_base == requested_base
            && listed
                .chars()
                .any(|character| character.is_ascii_alphabetic())
    })
}

fn opec_gdynia_notice_matches_house(address: &api_logic::AddressEntry, location: &str) -> bool {
    let street_names = utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let requested = address.house_no.trim();
    static HOUSE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let house_re = HOUSE_RE.get_or_init(|| regex::Regex::new(r"(?i)\d+\s*[a-z]?").unwrap());
    static RANGE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let range_re =
        RANGE_RE.get_or_init(|| regex::Regex::new(r"(?i)(\d+)\s*(?:-|–|—|do)\s*(\d+)").unwrap());

    let requested_house = house_re
        .find(requested)
        .map(|value| utils::clean_text(value.as_str()).replace(' ', ""));
    let Some(requested_house) = requested_house else {
        return true;
    };
    let requested_number = requested_house
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse::<u32>()
        .ok();
    let has_suffix = requested_house
        .chars()
        .any(|character| character.is_ascii_alphabetic());

    for street in street_names
        .into_iter()
        .filter(|street| !street.trim().is_empty())
    {
        let street_words: Vec<String> = utils::clean_text(utils::strip_street_prefixes(&street))
            .split(|character: char| !character.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(str::to_owned)
            .collect();
        if street_words.is_empty() {
            continue;
        }

        for segment in location.split(';') {
            let segment_words: Vec<String> = utils::clean_text(segment)
                .split(|character: char| !character.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_owned)
                .collect();
            if !segment_words
                .windows(street_words.len())
                .any(|candidate| candidate == street_words.as_slice())
            {
                continue;
            }

            let listed_houses: Vec<String> = house_re
                .find_iter(segment)
                .map(|value| utils::clean_text(value.as_str()).replace(' ', ""))
                .collect();
            if listed_houses.is_empty() {
                return true;
            }
            if listed_houses.iter().any(|house| house == &requested_house) {
                return true;
            }
            if has_suffix {
                continue;
            }

            if let Some(number) = requested_number {
                if range_re.captures_iter(segment).any(|captures| {
                    let start = captures.get(1).and_then(|value| value.as_str().parse::<u32>().ok());
                    let end = captures.get(2).and_then(|value| value.as_str().parse::<u32>().ok());
                    matches!((start, end), (Some(start), Some(end)) if number >= start.min(end) && number <= start.max(end))
                }) {
                    return true;
                }
                let normalized_segment = utils::clean_text(segment);
                if (normalized_segment.contains("nieparzyste")
                    || normalized_segment.contains("nieparz"))
                    && !number.is_multiple_of(2)
                {
                    return true;
                }
                if (normalized_segment.contains("parzyste") || normalized_segment.contains("parz"))
                    && !normalized_segment.contains("nieparz")
                    && number.is_multiple_of(2)
                {
                    return true;
                }
            }
        }
    }

    false
}

fn apply_alert_filtering(alerts: &mut Vec<UnifiedAlert>, settings: &Option<Settings>) {
    let s = match settings {
        Some(s) => s,
        None => return,
    };

    if s.filter_by_house_no {
        for alert in alerts.iter_mut() {
            if alert.source == api_logic::AlertSource::LpwikLegnica
                && alert.is_local == Some(true)
            {
                let message = alert.message.as_deref().unwrap_or_default();
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(
                            &alert.source,
                            address,
                        )
                        && lpwik_legnica::address_matches_notice(message, location, address)
                        && lpwik_legnica::house_number_matches_notice(
                            message, location, address,
                        )
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                    if let Some((locality, _)) = location.split_once(';') {
                        alert.location = Some(locality.to_string());
                    }
                }
                continue;
            }

            if alert.source == api_logic::AlertSource::AquaBielsko
                && alert.is_local == Some(true)
            {
                let notice = alert.message.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(&alert.source, address)
                        && aqua_bielsko::location_matches_address(notice, address)
                        && (!s.filter_by_house_no
                            || aqua_bielsko::house_number_matches_notice(notice, address))
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                }
                continue;
            }

            if alert.source == api_logic::AlertSource::PwikGorzow
                && alert.is_local == Some(true)
            {
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(&alert.source, address)
                        && pwik_gorzow::location_matches_address(location, address)
                        && (!s.filter_by_house_no
                            || pwik_gorzow::house_number_matches_notice(location, address))
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                }
                continue;
            }

            if matches!(
                alert.source,
                api_logic::AlertSource::PecGliwice | api_logic::AlertSource::PwikGliwice
            ) && alert.is_local == Some(true)
            {
                let notice = alert.message.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    if !address.is_active
                        || !api_logic::is_address_applicable_for_provider(&alert.source, address)
                    {
                        return false;
                    }

                    match alert.source {
                        api_logic::AlertSource::PecGliwice => {
                            pec_gliwice::location_matches_address(notice, address)
                                && (!s.filter_by_house_no
                                    || pec_gliwice::house_number_matches_notice(notice, address))
                        }
                        api_logic::AlertSource::PwikGliwice => {
                            pwik_gliwice::location_matches_address(notice, address)
                                && (!s.filter_by_house_no
                                    || pwik_gliwice::house_number_matches_notice(notice, address))
                        }
                        _ => false,
                    }
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                }
                continue;
            }

            if alert.source == api_logic::AlertSource::BytomskieWodociagi
                && alert.is_local == Some(true)
            {
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(
                            &alert.source,
                            address,
                        )
                        && bytomskie_wodociagi::location_matches_address(location, address)
                        && bytomskie_wodociagi::house_number_matches_notice(location, address)
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                    if let Some((locality, _)) = location.split_once(';') {
                        alert.location = Some(locality.to_string());
                    }
                }
                continue;
            }

            if alert.source == api_logic::AlertSource::DabrowskieWodociagi
                && alert.is_local == Some(true)
            {
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(
                            &alert.source,
                            address,
                        )
                        && dabrowskie_wodociagi::location_matches_address(location, address)
                        && dabrowskie_wodociagi::house_number_matches_notice(location, address)
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                }
                continue;
            }

            if alert.source == api_logic::AlertSource::EpecElblag
                && alert.is_local == Some(true)
            {
                let message = alert.message.as_deref().unwrap_or_default();
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(
                            &alert.source,
                            address,
                        )
                        && epec_elblag::address_matches_notice(message, location, address)
                        && epec_elblag::house_number_matches_notice(message, location, address)
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                }
                continue;
            }

            if alert.source == api_logic::AlertSource::PewikGdynia
                && alert.is_local == Some(true)
            {
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(
                            &alert.source,
                            address,
                        )
                        && pewik_gdynia::address_house_matches_notice(address, location)
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                    if let Some((locality, _)) = location.split_once(';') {
                        alert.location = Some(locality.to_string());
                    }
                }
                continue;
            }

            if alert.source == api_logic::AlertSource::OpecGdynia
                && alert.is_local == Some(true)
            {
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(
                            &alert.source,
                            address,
                        )
                        && opec_gdynia_notice_matches_house(address, location)
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                    if let Some((locality, _)) = location.split_once(';') {
                        alert.location = Some(locality.to_string());
                    }
                }
                continue;
            }

            if alert.source == api_logic::AlertSource::TorunskieWodociagi
                && alert.is_local == Some(true)
            {
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(
                            &alert.source,
                            address,
                        )
                        && torunskie_notice_matches_house(address, location)
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.is_local = Some(false);
                    if let Some((locality, _)) = location.split_once(';') {
                        alert.location = Some(locality.to_string());
                    }
                }
                continue;
            }

            if alert.source == api_logic::AlertSource::PwikOlsztyn
                && alert.is_local == Some(true)
            {
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(
                            &alert.source,
                            address,
                        )
                        && pwik_olsztyn::location_matches_address(location, address)
                        && utils::match_house_number(
                            &address.house_no,
                            location,
                            if !address.street_name_1.is_empty() {
                                Some(address.street_name_1.as_str())
                            } else if !address.street_name.is_empty() {
                                Some(address.street_name.as_str())
                            } else {
                                None
                            },
                        )
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                    alert.is_local = Some(true);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                    if let Some((locality, _)) = location.split_once(';') {
                        alert.location = Some(locality.to_string());
                    }
                }
                continue;
            }

            if matches!(
                alert.source,
                api_logic::AlertSource::WikOpole | api_logic::AlertSource::EcoSa
            ) && alert.is_local == Some(true)
            {
                let location = alert.location.as_deref().unwrap_or_default();
                let matched_address = s.addresses.iter().enumerate().find(|(_, address)| {
                    address.is_active
                        && api_logic::is_address_applicable_for_provider(&alert.source, address)
                        && match alert.source {
                            api_logic::AlertSource::WikOpole => {
                                wik_opole::location_matches_address(location, address)
                            }
                            api_logic::AlertSource::EcoSa => {
                                eco_sa::location_matches_address(location, address)
                            }
                            _ => false,
                        }
                        && {
                            let number_spec = if alert.source == api_logic::AlertSource::EcoSa {
                                eco_sa::number_spec_for_address(location, address)
                            } else {
                                location.to_string()
                            };
                            let street = if alert.source == api_logic::AlertSource::WikOpole {
                                if !address.street_name_1.is_empty() {
                                    Some(address.street_name_1.as_str())
                                } else if !address.street_name.is_empty() {
                                    Some(address.street_name.as_str())
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            utils::match_house_number(&address.house_no, &number_spec, street)
                        }
                });
                if let Some((index, _)) = matched_address {
                    alert.address_index = Some(index);
                } else {
                    alert.address_index = None;
                    alert.is_local = Some(false);
                    alert.location = Some(
                        location
                            .split([';', '\n'])
                            .next()
                            .unwrap_or(location)
                            .to_string(),
                    );
                }
                continue;
            }

            if alert.is_local == Some(true)
                && let Some(idx) = alert.address_index
                    && idx < s.addresses.len() {
                        let addr = &s.addresses[idx];
                        let is_street_wide_only = matches!(
                            alert.source,
                            api_logic::AlertSource::TauronHeat | api_logic::AlertSource::AquaBielsko
                        );
                        if !is_street_wide_only {
                            let mut spec = if alert.source
                                == api_logic::AlertSource::ZwikZielonaGora
                            {
                                alert.location.clone().unwrap_or_default()
                            } else {
                                String::new()
                            };
                            if alert.source != api_logic::AlertSource::ZwikZielonaGora {
                                if let Some(msg) = &alert.message {
                                    spec.push_str(msg);
                                }
                                if let Some(loc) = &alert.location {
                                    if !spec.is_empty() {
                                        spec.push(' ');
                                    }
                                    spec.push_str(loc);
                                }
                            }
                            let street_to_match = if !addr.street_name_1.is_empty() {
                                Some(addr.street_name_1.as_str())
                            } else if !addr.street_name.is_empty() {
                                Some(addr.street_name.as_str())
                            } else {
                                None
                            };
                            if !utils::match_house_number(&addr.house_no, &spec, street_to_match) {
                                alert.is_local = Some(false);
                            }
                        }
                    }
        }
    }

    alerts.retain(|alert| {
        if let Some(idx) = alert.address_index
            && idx < s.addresses.len() {
                return s.addresses[idx].is_active;
            }
        
        // For general city alerts
        if alert.is_local == Some(false)
            && let Some(loc) = &alert.location {
                if loc.contains("Wrocław") {
                    return s.addresses.iter().any(|a| a.is_active);
                }
                if loc.contains("Warszawa") {
                    return s.addresses.iter().any(|a| a.is_active);
                }
                if loc.contains("Kraków") {
                    return s.addresses.iter().any(|a| a.is_active);
                }
                // For other cities (dynamic check based on active addresses)
                for addr in s.addresses.iter().filter(|a| a.is_active) {
                    if loc.contains(&addr.city_name) {
                        return true;
                    }
                }
                return false; // Skip if no active address in this city
            }

        true
    });
}

async fn fetch_all_alerts_internal(
    app: &AppHandle,
    sources: Option<Vec<String>>,
    cached_only: Option<bool>,
) -> Result<api_logic::AlertsResponse, String> {
    let db_state = app.state::<DbState>();
    let cache_state = app.state::<cache::CacheState>();

    // 1. If cached_only is true, load instantly from the persistent database cache
    if cached_only.unwrap_or(false) {
        let conn = db_state.conn.lock().unwrap();
        if let Ok(Some(json_str)) = state_db::get_kv(&conn, "alerts_cache")
            && let Ok(mut cached_alerts) = serde_json::from_str::<Vec<UnifiedAlert>>(&json_str) {
                log::info!("Serving fetch_all_alerts: persistent SQLite cache hits ({} items)", cached_alerts.len());
                let path = settings_path(app)?;
                let settings = load_settings_from_path(&path)?;
                apply_alert_filtering(&mut cached_alerts, &settings);
                return Ok(api_logic::AlertsResponse { alerts: cached_alerts, is_stale: true, is_offline: false });
            }
        return Ok(api_logic::AlertsResponse { alerts: Vec::new(), is_stale: true, is_offline: false });
    }

    // 2. Check Memory Cache (full refresh) or per-source cache (single-provider refresh)
    if sources.is_none() {
        if let Some(mut cached) = cache_state.get() {
            log::info!("Serving fetch_all_alerts from cache ({} items)", cached.len());
            let path = settings_path(app)?;
            let settings = load_settings_from_path(&path)?;
            apply_alert_filtering(&mut cached, &settings);
            return Ok(api_logic::AlertsResponse { alerts: cached, is_stale: false, is_offline: false });
        }
    } else if let Some(ref requested_sources) = sources {
        let mut cached_results = Vec::new();
        let mut all_hit = true;
        for src_id in requested_sources {
            match cache_state.get_source(src_id) {
                Some(cached) => cached_results.extend(cached),
                None => { all_hit = false; break; }
            }
        }
        if all_hit {
            log::info!("Serving fetch_all_alerts for {:?} from per-source cache ({} items)", requested_sources, cached_results.len());
            let path = settings_path(app)?;
            let settings = load_settings_from_path(&path)?;
            apply_alert_filtering(&mut cached_results, &settings);
            return Ok(api_logic::AlertsResponse { alerts: cached_results, is_stale: false, is_offline: false });
        }
    }
    let path = settings_path(app)?;
    let settings = load_settings_from_path(&path)?;
    log::debug!("fetch_all_alerts_internal: Loaded settings={:?}", settings);
    let settings_orig = settings.clone();

    let mut all_alerts: Vec<UnifiedAlert> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut attempted_providers = 0;
    let mut failed_providers = 0;

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

    let api_semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_API_REQUESTS));
    let webview_semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_WEBVIEW_REQUESTS));
    let mut tasks = Vec::new();

    if let Some(s) = settings {
        let s_arc = Arc::new(s.clone());
        let providers = get_providers();
        let net_state = app.state::<NetworkState>();
        let client = net_state.get_client().await?;
        
        // --- INTERNET CONNECTIVITY CHECK ---
        if !NetworkState::check_internet_connection(&client).await {
            log::warn!("Active internet check failed. Device appears to be offline.");
            let mut recovered_alerts = Vec::new();
            let conn = db_state.conn.lock().unwrap();
            if let Ok(Some(json_str)) = state_db::get_kv(&conn, "alerts_cache")
                && let Ok(cached_alerts) = serde_json::from_str::<Vec<UnifiedAlert>>(&json_str) {
                    recovered_alerts = cached_alerts;
                }
            
            apply_alert_filtering(&mut recovered_alerts, &settings_orig);
            if !recovered_alerts.is_empty() {
                return Ok(api_logic::AlertsResponse { alerts: recovered_alerts, is_stale: true, is_offline: true });
            }
            if let Some(mut cached) = cache_state.get_stale() {
                apply_alert_filtering(&mut cached, &settings_orig);
                return Ok(api_logic::AlertsResponse { alerts: cached, is_stale: true, is_offline: true });
            }
            return Err("ERR_NO_INTERNET".to_string());
        }

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
            let is_webview = provider.id() == "psg" || provider.id() == "gpec" || provider.id() == "pwik_kalisz";
            let sem = if is_webview {
                webview_semaphore.clone()
            } else {
                api_semaphore.clone()
            };
            let c = client.clone();
            let c_h1 = client_http1.clone();
            let app_h = app.clone();
            tasks.push(tauri::async_runtime::spawn(async move {
                let _permit = sem.acquire().await.ok();
                let source = provider.source();
                let fetch_result = provider.fetch(&c, &c_h1, &s_p, Some(&app_h)).await;
                (source, fetch_result)
            }));
        }

        attempted_providers = tasks.len();
        let results = join_all(tasks).await;

        let mut failed_sources = Vec::new();

        for res in results {
            match res {
                Ok((source, (mut alerts, errs))) => {
                    if !errs.is_empty() && alerts.is_empty() {
                        failed_providers += 1;
                        failed_sources.push(source);
                    }
                    for alert in &mut alerts {
                        alert.hash = Some(alert.to_hash());
                    }
                    all_alerts.extend(alerts);
                    errors.extend(errs);
                }
                Err(e) => {
                    failed_providers += 1;
                    errors.push(format!("Task execution error: {}", e));
                }
            }
        }

        // --- RECOVER FAILED PROVIDERS FROM CACHE ---
        if !failed_sources.is_empty() {
            log::info!("Attempting to recover alerts for {} failed providers from persistent cache", failed_sources.len());
            let conn = db_state.conn.lock().unwrap();
            if let Ok(Some(json_str)) = state_db::get_kv(&conn, "alerts_cache")
                && let Ok(cached_alerts) = serde_json::from_str::<Vec<UnifiedAlert>>(&json_str) {
                    let mut recovered = 0;
                    for old_alert in cached_alerts {
                        if failed_sources.contains(&old_alert.source) {
                            all_alerts.push(old_alert);
                            recovered += 1;
                        }
                    }
                    log::info!("Recovered {} alerts from persistent cache", recovered);
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

        // --- APPLY ALERT FILTERING BEFORE PROCESSING NOTIFICATIONS ---
        apply_alert_filtering(&mut all_alerts, &settings_orig);

        // --- PROCESS NEW ALERTS AND NOTIFY ---
        let db_adapter = RealDatabase(&db_state.conn);
        let notifier = RealNotification(app);
        let engine = MonitorEngine::new(&db_adapter, &notifier, &s);
        engine.process_alerts(all_alerts.clone());
    }

    if failed_providers == attempted_providers && attempted_providers > 0 {
        if !all_alerts.is_empty() {
            log::warn!("Fetch failed for all providers, but recovered {} items from persistent cache", all_alerts.len());
            apply_alert_filtering(&mut all_alerts, &settings_orig);
            return Ok(api_logic::AlertsResponse { alerts: all_alerts, is_stale: true, is_offline: true });
        }
        // For a specific-source refresh, don't throw ERR_NO_INTERNET — JS suppresses UI errors
        // for single-source fetches anyway (see script.js specificSource check). A hard Err would
        // still cause a noisy console error and break the merge logic on the frontend.
        if sources.is_some() {
            log::warn!("Fetch failed for all {} providers in single-source mode, returning empty stale response", attempted_providers);
            return Ok(api_logic::AlertsResponse { alerts: vec![], is_stale: true, is_offline: false });
        }
        if let Some(mut cached) = cache_state.get_stale() {
            log::warn!("Fetch failed for all {} providers ({} errors), falling back to stale cache ({} items)", attempted_providers, errors.len(), cached.len());
            apply_alert_filtering(&mut cached, &settings_orig);
            return Ok(api_logic::AlertsResponse { alerts: cached, is_stale: true, is_offline: true });
        }
        return Err("ERR_NO_INTERNET".to_string());
    }

    apply_alert_filtering(&mut all_alerts, &settings_orig);

    if sources.is_none() {
        cache_state.set(all_alerts.clone());

        // Save to persistent database cache
        if let Ok(json_str) = serde_json::to_string(&all_alerts) {
            let conn = db_state.conn.lock().unwrap();
            let _ = state_db::set_kv(&conn, "alerts_cache", &json_str);
        }
    } else if let Some(ref requested_sources) = sources {
        // Save per-source cache for each fetched source
        for src_id in requested_sources {
            let src_alerts: Vec<_> = all_alerts.iter()
                .filter(|a| a.source.to_string() == *src_id)
                .cloned()
                .collect();
            cache_state.set_source(src_id, src_alerts);
        }
    }

    log::info!("app_lib: fetch_all_alerts_internal returning {} alerts", all_alerts.len());
    Ok(api_logic::AlertsResponse { alerts: all_alerts, is_stale: false, is_offline: false })
}

#[tauri::command]
async fn fetch_all_alerts(
    app: AppHandle,
    sources: Option<Vec<String>>,
    cached_only: Option<bool>,
) -> Result<api_logic::AlertsResponse, String> {
    fetch_all_alerts_internal(&app, sources, cached_only).await
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
            match fetch_all_alerts_internal(&app, None, None).await {
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

    #[cfg(feature = "smoke-tests")]
    #[tokio::test]
    #[ignore]
    async fn test_opec_gdynia_live_api() {
        let client = NetworkState::build_client().expect("Failed to build HTTP client");
        let client_http1 = NetworkState::build_client_http1()
            .expect("Failed to build HTTP/1.1 client");
        let settings = Settings {
            addresses: vec![api_logic::AddressEntry {
                name: "Mock Gdynia".to_string(),
                city_name: "Gdynia".to_string(),
                voivodeship: "Pomorskie".to_string(),
                street_name: "Unruga".to_string(),
                street_name_1: "Unruga".to_string(),
                house_no: "90".to_string(),
                is_active: true,
                ..Default::default()
            }],
            ..Default::default()
        };

        let provider = opec_gdynia::OpecGdyniaProvider;
        let (alerts, errors) = provider
            .fetch(&client, &client_http1, &settings, None)
            .await;
        assert!(errors.is_empty(), "OPEC Gdynia live API returned errors: {errors:?}");
        println!("OPEC Gdynia live API parsed {} current alerts", alerts.len());
    }

    #[cfg(feature = "smoke-tests")]
    #[tokio::test]
    #[ignore]
    async fn test_pewik_gdynia_live_publications() {
        let client = NetworkState::build_client().expect("Failed to build HTTP client");
        let client_http1 = NetworkState::build_client_http1()
            .expect("Failed to build HTTP/1.1 client");
        let settings = Settings {
            addresses: vec![api_logic::AddressEntry {
                name: "Mock Gdynia".to_string(),
                city_name: "Gdynia".to_string(),
                voivodeship: "Pomorskie".to_string(),
                street_name: "Owsiana".to_string(),
                street_name_1: "Owsiana".to_string(),
                house_no: "12".to_string(),
                is_active: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        let provider = pewik_gdynia::PewikGdyniaProvider;
        let (alerts, errors) = provider
            .fetch(&client, &client_http1, &settings, None)
            .await;
        assert!(
            errors.iter().all(|error| error.contains("planned notice")),
            "PEWIK Gdynia emergency publication failed: {errors:?}"
        );
        println!(
            "PEWIK Gdynia live publications parsed {} alerts; additional endpoint errors: {errors:?}",
            alerts.len()
        );
    }

    #[test]
    fn pewik_house_filter_checks_all_listed_streets_centrally() {
        let mut settings = Settings {
            filter_by_house_no: true,
            addresses: vec![
                api_logic::AddressEntry {
                    city_name: "Gdynia".to_string(),
                    voivodeship: "Pomorskie".to_string(),
                    street_name: "Owsiana".to_string(),
                    street_name_1: "Owsiana".to_string(),
                    house_no: "5".to_string(),
                    is_active: true,
                    ..Default::default()
                },
                api_logic::AddressEntry {
                    city_name: "Gdynia".to_string(),
                    voivodeship: "Pomorskie".to_string(),
                    street_name: "Lniana".to_string(),
                    street_name_1: "Lniana".to_string(),
                    house_no: "12".to_string(),
                    is_active: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let make_alert = || UnifiedAlert {
            source: api_logic::AlertSource::PewikGdynia,
            message: Some("Awaria wodociągowa: Owsianej 1-4 i Lnianej 10-14".to_string()),
            location: Some("Miejscowość: Gdynia; Owsianej 1-4; Lnianej 10-14".to_string()),
            address_index: Some(0),
            is_local: Some(true),
            ..Default::default()
        };

        let mut alerts = vec![make_alert()];
        apply_alert_filtering(&mut alerts, &Some(settings.clone()));
        assert_eq!(alerts[0].address_index, Some(1));
        assert_eq!(alerts[0].is_local, Some(true));

        settings.addresses[1].house_no = "16".to_string();
        let mut alerts = vec![make_alert()];
        apply_alert_filtering(&mut alerts, &Some(settings));
        assert_eq!(alerts[0].address_index, None);
        assert_eq!(alerts[0].is_local, Some(false));
        assert_eq!(alerts[0].location.as_deref(), Some("Miejscowość: Gdynia"));
        assert!(alerts[0].message.as_deref().unwrap().contains("Owsianej 1-4"));
    }

    #[test]
    fn opec_house_filter_checks_all_addresses_and_matches_suffixes_exactly() {
        let mut settings = Settings {
            filter_by_house_no: true,
            addresses: vec![
                api_logic::AddressEntry {
                    city_name: "Gdynia".to_string(),
                    voivodeship: "Pomorskie".to_string(),
                    street_name: "Podgórska".to_string(),
                    street_name_1: "Podgórska".to_string(),
                    house_no: "99".to_string(),
                    is_active: true,
                    ..Default::default()
                },
                api_logic::AddressEntry {
                    city_name: "Gdynia".to_string(),
                    voivodeship: "Pomorskie".to_string(),
                    street_name: "Unruga".to_string(),
                    street_name_1: "Unruga".to_string(),
                    house_no: "90".to_string(),
                    is_active: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let make_alert = || UnifiedAlert {
            source: api_logic::AlertSource::OpecGdynia,
            message: Some("Awaria ciepła: Gdynia street notice".to_string()),
            location: Some(
                "Miejscowość: Gdynia; ul. Unruga 90, 92; ul. Podgórska 1, 2, 2A, 3, 5, 7, 9, 11"
                    .to_string(),
            ),
            address_index: Some(0),
            is_local: Some(true),
            ..Default::default()
        };

        let mut alerts = vec![make_alert()];
        apply_alert_filtering(&mut alerts, &Some(settings.clone()));
        assert_eq!(alerts[0].is_local, Some(true));
        assert_eq!(alerts[0].address_index, Some(1));

        settings.addresses[1].house_no = "90A".to_string();
        let mut alerts = vec![make_alert()];
        apply_alert_filtering(&mut alerts, &Some(settings));
        assert_eq!(alerts[0].is_local, Some(false));
        assert_eq!(alerts[0].address_index, None);
        assert_eq!(alerts[0].location.as_deref(), Some("Miejscowość: Gdynia"));
        assert!(alerts[0].message.as_deref().unwrap().contains("street notice"));
    }

    #[tokio::test]
    async fn test_fetch_enea_outages_real_backend() {
        let client = NetworkState::build_client().unwrap();
        let items = enea::fetch_all_enea_outages(&client, &[7]).await.unwrap();
        
        let compiled = CompiledEneaRegex::new("Kicin", "Poznańska", "Poznańska", &None);
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

    // Also find and cache our GpecWebViewFetcher class
    log::info!("Caching GpecWebViewFetcher class...");
    match env.find_class(jni::jni_str!("xyz/eremef/awaria/GpecWebViewFetcher")) {
        Ok(cls) => {
            if let Ok(cls_ref) = env.new_global_ref(cls) {
                if let Ok(mut g_gpec) = GPEC_FETCHER_CLASS.lock() {
                    *g_gpec = Some(std::sync::Arc::new(cls_ref));
                    log::info!("GpecWebViewFetcher class cached successfully.");
                }
            }
        }
        Err(e) => {
            log::error!("Failed to find GpecWebViewFetcher class: {:?}", e);
        }
    }

    log::info!("Caching PwikKaliszFetcher class...");
    match env.find_class(jni::jni_str!("xyz/eremef/awaria/PwikKaliszFetcher")) {
        Ok(cls) => {
            if let Ok(cls_ref) = env.new_global_ref(cls) {
                if let Ok(mut g) = PWIK_KALISZ_FETCHER_CLASS.lock() {
                    *g = Some(std::sync::Arc::new(cls_ref));
                    log::info!("PwikKaliszFetcher class cached successfully.");
                }
            }
        }
        Err(e) => {
            log::error!("Failed to find PwikKaliszFetcher class: {:?}", e);
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
#[unsafe(no_mangle)]
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
                    log::info!("Provider: {}", p.source());
                    if !api_logic::is_provider_applicable(p.source(), &settings) {
                        log::info!("is_provider_applicable: provider not applicable for this region");
                        return 0;
                    }
                    let (mut alerts, _) = p.fetch(&client, &client_http1, &settings, None).await;
                    let now = chrono::Utc::now();
                    alerts.retain(|alert| {
                        if let Some(end_str) = &alert.endDate {
                            if let Some(end_dt) = utils::parse_date(end_str) {
                                log::debug!("end_dt: {}, now: {}", end_dt, now);
                                return end_dt >= now;
                            }
                        }
                        true
                    });
                    let grouped = api_logic::deduplicate_alerts(alerts);
                    log::info!("Alerts count after filtering (is_local): {}", grouped.iter().filter(|a| a.is_local == Some(true)).count());
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
#[unsafe(no_mangle)]
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

            let mut deduplicated = api_logic::deduplicate_alerts(all_alerts);
            apply_alert_filtering(&mut deduplicated, &Some(settings.clone()));
            log::info!("Background monitoring (Rust): processing {} deduplicated alerts.", deduplicated.len());
            engine.process_alerts(deduplicated);
            log::info!("Background monitoring (Rust): monitoring cycle complete.");
        });
        Ok(())
    });
}

#[cfg(target_os = "android")]
#[allow(non_snake_case)]
#[unsafe(no_mangle)]
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
        let vm = {
            let vm_guard = JAVA_VM.lock().unwrap();
            vm_guard.as_ref().cloned()
        }.ok_or_else(|| "JAVA_VM not initialized".to_string())?;
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

#[cfg(test)]
mod zwik_zielona_gora_house_number_filter_tests {
    use super::*;

    fn alert() -> UnifiedAlert {
        UnifiedAlert {
            source: api_logic::AlertSource::ZwikZielonaGora,
            message: Some(
                "Przerwa przy ul. Chmielna 1-23; ul. Zacisze 80 również objęta pracami."
                    .to_string(),
            ),
            location: Some("Zielona Góra; ul. Chmielna 1-23".to_string()),
            address_index: Some(0),
            is_local: Some(true),
            ..Default::default()
        }
    }

    fn settings(filter_by_house_no: bool) -> Option<Settings> {
        Some(Settings {
            addresses: vec![AddressEntry {
                name: "Dom".to_string(),
                city_name: "Zielona Góra".to_string(),
                voivodeship: "Lubuskie".to_string(),
                commune: "Zielona Góra".to_string(),
                street_name: "Chmielna".to_string(),
                street_name_1: "Chmielna".to_string(),
                house_no: "80".to_string(),
                city_id: Some(988313),
                is_active: true,
                ..Default::default()
            }],
            filter_by_house_no,
            ..Settings::default()
        })
    }

    #[test]
    fn zwik_house_number_filter_uses_the_street_scoped_location() {
        let mut alerts = vec![alert()];
        apply_alert_filtering(&mut alerts, &settings(true));
        assert_eq!(alerts[0].is_local, Some(false));
    }

    #[test]
    fn zwik_house_number_filter_remains_setting_controlled() {
        let mut alerts = vec![alert()];
        apply_alert_filtering(&mut alerts, &settings(false));
        assert_eq!(alerts[0].is_local, Some(true));
    }
}

#[cfg(test)]
mod lpwik_legnica_house_number_filter_tests {
    use super::*;

    #[test]
    fn lpwik_legnica_provider_is_registered() {
        assert!(get_providers()
            .iter()
            .any(|provider| provider.source() == api_logic::AlertSource::LpwikLegnica));
    }

    fn alert() -> UnifiedAlert {
        UnifiedAlert {
            source: api_logic::AlertSource::LpwikLegnica,
            message: Some(
                "Prace przy ul. Sokolskiej. Przerwy w dostawie wody: ul. Sokolska 1-37 oraz ul. Jaskółcza 1-3."
                    .to_string(),
            ),
            location: Some("Legnica; ul. Sokolska 1-37; ul. Jaskółcza 1-3".to_string()),
            address_index: Some(0),
            is_local: Some(true),
            ..Default::default()
        }
    }

    fn settings(filter_by_house_no: bool) -> Option<Settings> {
        Some(Settings {
            addresses: vec![AddressEntry {
                name: "Dom".to_string(),
                city_name: "Legnica".to_string(),
                voivodeship: "Dolnośląskie".to_string(),
                street_name: "Sokolska".to_string(),
                street_name_1: "Sokolska".to_string(),
                house_no: "40".to_string(),
                city_id: Some(954047),
                is_active: true,
                ..Default::default()
            }],
            filter_by_house_no,
            ..Settings::default()
        })
    }

    #[test]
    fn lpwik_house_number_matching_is_controlled_by_the_central_setting() {
        let mut disabled_alerts = vec![alert()];
        apply_alert_filtering(&mut disabled_alerts, &settings(false));
        assert_eq!(disabled_alerts[0].is_local, Some(true));

        let mut enabled_alerts = vec![alert()];
        apply_alert_filtering(&mut enabled_alerts, &settings(true));
        assert_eq!(enabled_alerts[0].is_local, Some(false));
        assert_eq!(enabled_alerts[0].address_index, None);
    }
}

#[cfg(test)]
mod aqua_bielsko_house_number_filter_tests {
    use super::*;

    fn alert() -> UnifiedAlert {
        UnifiedAlert {
            source: api_logic::AlertSource::AquaBielsko,
            message: Some("Awaria w Bielsko-Biała przy ul. Testowa nr 12-16".to_string()),
            address_index: Some(0), is_local: Some(true), ..Default::default()
        }
    }

    fn settings(filter_by_house_no: bool, house_no: &str) -> Option<Settings> {
        Some(Settings {
            addresses: vec![AddressEntry {
                name: "Dom".to_string(), city_name: "Bielsko-Biała".to_string(),
                voivodeship: "Śląskie".to_string(), commune: "Bielsko-Biała".to_string(),
                street_name: "Testowa".to_string(), street_name_1: "Testowa".to_string(),
                house_no: house_no.to_string(), city_id: Some(923584), is_active: true, ..Default::default()
            }],
            filter_by_house_no, ..Settings::default()
        })
    }

    #[test]
    fn aqua_bielsko_provider_is_registered_and_number_filter_is_setting_controlled() {
        assert!(get_providers().iter().any(|provider| provider.source() == api_logic::AlertSource::AquaBielsko));
        let mut disabled = vec![alert()];
        apply_alert_filtering(&mut disabled, &settings(false, "25"));
        assert_eq!(disabled[0].is_local, Some(true));
        let mut included = vec![alert()];
        apply_alert_filtering(&mut included, &settings(true, "14"));
        assert_eq!(included[0].is_local, Some(true));
        let mut excluded = vec![alert()];
        apply_alert_filtering(&mut excluded, &settings(true, "25"));
        assert_eq!(excluded[0].is_local, Some(false));
    }
}

#[cfg(test)]
mod bytomskie_wodociagi_house_number_filter_tests {
    use super::*;

    #[test]
    fn bytomskie_wodociagi_provider_is_registered() {
        assert!(get_providers().iter().any(|provider| {
            provider.source() == api_logic::AlertSource::BytomskieWodociagi
        }));
    }

    fn alert() -> UnifiedAlert {
        UnifiedAlert {
            source: api_logic::AlertSource::BytomskieWodociagi,
            message: Some("Przerwa w dostawie wody".to_string()),
            location: Some("Miejscowość: Bytom; ul. Testowa 12-16".to_string()),
            address_index: Some(0),
            is_local: Some(true),
            ..Default::default()
        }
    }

    fn settings(filter_by_house_no: bool, house_no: &str) -> Option<Settings> {
        Some(Settings {
            addresses: vec![AddressEntry {
                name: "Dom".to_string(),
                city_name: "Bytom".to_string(),
                voivodeship: "Śląskie".to_string(),
                street_name: "Testowa".to_string(),
                street_name_1: "Testowa".to_string(),
                house_no: house_no.to_string(),
                city_id: Some(938670),
                is_active: true,
                ..Default::default()
            }],
            filter_by_house_no,
            ..Settings::default()
        })
    }

    #[test]
    fn bytomskie_wodociagi_house_number_filter_is_setting_controlled() {
        let mut disabled_alerts = vec![alert()];
        apply_alert_filtering(&mut disabled_alerts, &settings(false, "25"));
        assert_eq!(disabled_alerts[0].is_local, Some(true));

        let mut matching_alerts = vec![alert()];
        apply_alert_filtering(&mut matching_alerts, &settings(true, "14"));
        assert_eq!(matching_alerts[0].is_local, Some(true));
        assert_eq!(matching_alerts[0].address_index, Some(0));

        let mut non_matching_alerts = vec![alert()];
        apply_alert_filtering(&mut non_matching_alerts, &settings(true, "25"));
        assert_eq!(non_matching_alerts[0].is_local, Some(false));
        assert_eq!(non_matching_alerts[0].address_index, None);
    }
}

#[cfg(target_os = "android")]
pub async fn get_gpec_html_android() -> Result<String, String> {
    let context_ref = ANDROID_CONTEXT.lock().map_err(|_| "Android Context lock poisoned".to_string())?.clone().ok_or("Android Context not initialized")?;
    let class_ref = GPEC_FETCHER_CLASS.lock().map_err(|_| "GpecWebViewFetcher lock poisoned".to_string())?.clone().ok_or("GpecWebViewFetcher class not cached")?;
    
    #[allow(deprecated)]
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let vm = {
            let vm_guard = JAVA_VM.lock().unwrap();
            vm_guard.as_ref().cloned()
        }.ok_or_else(|| "JAVA_VM not initialized".to_string())?;
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
                    Err("Native GPEC fetch returned null".to_string())
                } else {
                    Ok(html)
                }
            },
            Err(e) => Err(e.to_string())
        }
    }).await.map_err(|e: tokio::task::JoinError| e.to_string())?
}

/// Fetches a URL on Android using Kotlin's HttpURLConnection (Android BoringSSL/Conscrypt TLS),
/// bypassing rustls for sites with incompatible TLS configurations (e.g. wodociagi-kalisz.pl).
#[cfg(target_os = "android")]
pub async fn fetch_url_via_android(url: &str) -> Result<String, String> {
    let class_ref = PWIK_KALISZ_FETCHER_CLASS.lock()
        .map_err(|_| "PwikKaliszFetcher lock poisoned".to_string())?
        .clone()
        .ok_or("PwikKaliszFetcher class not cached")?;

    let url_owned = url.to_string();
    #[allow(deprecated)]
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let vm = {
            let vm_guard = JAVA_VM.lock().unwrap();
            vm_guard.as_ref().cloned()
        }.ok_or_else(|| "JAVA_VM not initialized".to_string())?;
        let res: Result<String, jni::errors::Error> = vm.attach_current_thread(|env| {
            let jurl = env.new_string(&url_owned)?;
            let result = env.call_static_method(
                &*class_ref,
                jni::jni_str!("fetchUrl"),
                jni::jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
                &[jni::objects::JValue::Object(&jurl)]
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
                    Err("PwikKaliszFetcher.fetchUrl returned null".to_string())
                } else {
                    Ok(html)
                }
            },
            Err(e) => Err(e.to_string())
        }
    }).await.map_err(|e: tokio::task::JoinError| e.to_string())?
}

#[cfg(test)]
mod pwik_gorzow_house_number_filter_tests {
    use super::*;

    fn alert() -> UnifiedAlert {
        let notice = "Gorzowie Wlkp. - ul. Osiedlowa nr od 14 do 25 oraz od 31 do 35";
        UnifiedAlert {
            source: api_logic::AlertSource::PwikGorzow,
            message: Some(format!("Awaria — W trakcie: {notice}")),
            location: Some(notice.to_string()),
            address_index: Some(0),
            is_local: Some(true),
            ..Default::default()
        }
    }

    fn settings(filter_by_house_no: bool, house_no: &str) -> Option<Settings> {
        Some(Settings {
            addresses: vec![AddressEntry {
                name: "Dom".to_string(),
                city_name: "Gorzów Wielkopolski".to_string(),
                voivodeship: "Lubuskie".to_string(),
                commune: "Gorzów Wielkopolski".to_string(),
                street_name: "Osiedlowa".to_string(),
                street_name_1: "Osiedlowa".to_string(),
                house_no: house_no.to_string(),
                city_id: Some(935_140),
                is_active: true,
                ..Default::default()
            }],
            filter_by_house_no,
            ..Settings::default()
        })
    }

    #[test]
    fn pwik_gorzow_provider_is_registered_and_house_filter_respects_setting() {
        assert!(get_providers()
            .iter()
            .any(|provider| provider.source() == api_logic::AlertSource::PwikGorzow));

        let mut disabled = vec![alert()];
        apply_alert_filtering(&mut disabled, &settings(false, "26"));
        assert_eq!(disabled[0].is_local, Some(true));

        let mut included = vec![alert()];
        apply_alert_filtering(&mut included, &settings(true, "14"));
        assert_eq!(included[0].is_local, Some(true));

        let mut excluded = vec![alert()];
        apply_alert_filtering(&mut excluded, &settings(true, "26"));
        assert!(excluded.is_empty());
    }
}

#[cfg(test)]
mod gliwice_provider_house_number_filter_tests {
    use super::*;

    fn settings(source: api_logic::AlertSource, filter_by_house_no: bool, house_no: &str) -> Option<Settings> {
        let (city_name, commune, city_id) = match source {
            api_logic::AlertSource::PecGliwice => ("Gliwice", "Gliwice", 940_000),
            api_logic::AlertSource::PwikGliwice => ("Gliwice", "Gliwice", 940_000),
            _ => unreachable!(),
        };
        Some(Settings {
            addresses: vec![AddressEntry {
                name: "Dom".to_string(),
                city_name: city_name.to_string(),
                voivodeship: "Śląskie".to_string(),
                commune: commune.to_string(),
                street_name: "Testowa".to_string(),
                street_name_1: "Testowa".to_string(),
                house_no: house_no.to_string(),
                city_id: Some(city_id),
                is_active: true,
                ..Default::default()
            }],
            filter_by_house_no,
            ..Settings::default()
        })
    }

    fn pec_alert() -> UnifiedAlert {
        UnifiedAlert {
            source: api_logic::AlertSource::PecGliwice,
            message: Some("Awaryjna przerwa w dostawie ciepła w Gliwicach przy ul. Testowej 12-16.".into()),
            location: Some("Gliwice; ul. Testowa 12-16".into()),
            address_index: Some(0),
            is_local: Some(true),
            ..Default::default()
        }
    }

    fn pwik_alert() -> UnifiedAlert {
        UnifiedAlert {
            source: api_logic::AlertSource::PwikGliwice,
            message: Some("PLANOWANE. Przerwa w dostawie wody w Gliwicach przy ul. Testowej nr 12-16.".into()),
            location: Some("Miejscowość: Gliwice; ul. Testowa 12-16".into()),
            address_index: Some(0),
            is_local: Some(true),
            ..Default::default()
        }
    }

    #[test]
    fn pec_gliwice_provider_is_registered_and_house_filter_respects_setting() {
        assert!(get_providers()
            .iter()
            .any(|provider| provider.source() == api_logic::AlertSource::PecGliwice));

        let mut disabled = vec![pec_alert()];
        apply_alert_filtering(&mut disabled, &settings(api_logic::AlertSource::PecGliwice, false, "25"));
        assert_eq!(disabled[0].is_local, Some(true));

        let mut included = vec![pec_alert()];
        apply_alert_filtering(&mut included, &settings(api_logic::AlertSource::PecGliwice, true, "14"));
        assert_eq!(included[0].is_local, Some(true));

        let mut excluded = vec![pec_alert()];
        apply_alert_filtering(&mut excluded, &settings(api_logic::AlertSource::PecGliwice, true, "25"));
        assert_eq!(excluded[0].is_local, Some(false));
    }

    #[test]
    fn pwik_gliwice_provider_is_registered_and_house_filter_respects_setting() {
        assert!(get_providers()
            .iter()
            .any(|provider| provider.source() == api_logic::AlertSource::PwikGliwice));

        let mut disabled = vec![pwik_alert()];
        apply_alert_filtering(&mut disabled, &settings(api_logic::AlertSource::PwikGliwice, false, "25"));
        assert_eq!(disabled[0].is_local, Some(true));

        let mut included = vec![pwik_alert()];
        apply_alert_filtering(&mut included, &settings(api_logic::AlertSource::PwikGliwice, true, "14"));
        assert_eq!(included[0].is_local, Some(true));

        let mut excluded = vec![pwik_alert()];
        apply_alert_filtering(&mut excluded, &settings(api_logic::AlertSource::PwikGliwice, true, "25"));
        assert_eq!(excluded[0].is_local, Some(false));
    }
}

#[cfg(test)]
mod regional_water_provider_registry_tests {
    use super::{api_logic::AlertSource, get_providers};

    #[test]
    fn all_approved_regional_water_providers_are_registered() {
        let providers = get_providers();
        for source in [
            AlertSource::MwikKoszalin,
            AlertSource::SosnowieckieWodociagi,
            AlertSource::TarnowskieWodociagi,
            AlertSource::ZabrzanskieWodociagi,
            AlertSource::PwikRudaSlaska,
            AlertSource::PwikRybnik,
        ] {
            assert!(providers.iter().any(|provider| provider.source() == source));
        }
    }

    #[test]
    fn shared_house_number_setting_controls_all_new_provider_alerts() {
        use super::{AddressEntry, Settings, UnifiedAlert, apply_alert_filtering};

        let providers = [
            (
                AlertSource::MwikKoszalin,
                "Koszalin",
                "Zachodniopomorskie",
                949_448,
            ),
            (
                AlertSource::SosnowieckieWodociagi,
                "Sosnowiec",
                "Śląskie",
                943_428,
            ),
            (
                AlertSource::TarnowskieWodociagi,
                "Tarnów",
                "Małopolskie",
                981_570,
            ),
            (
                AlertSource::ZabrzanskieWodociagi,
                "Zabrze",
                "Śląskie",
                945_380,
            ),
            (
                AlertSource::PwikRudaSlaska,
                "Ruda Śląska",
                "Śląskie",
                942_630,
            ),
            (AlertSource::PwikRybnik, "Rybnik", "Śląskie", 942_765),
        ];

        for (source, city, voivodeship, city_id) in providers {
            let alert = || UnifiedAlert {
                source,
                message: Some("Planowana przerwa w dostawie wody przy ul. Testowej 12-20".into()),
                location: Some(format!("{city}; ul. Testowa 12-20")),
                address_index: Some(0),
                is_local: Some(true),
                ..Default::default()
            };
            let settings = |house_no: &str, filter_by_house_no| {
                Some(Settings {
                    addresses: vec![AddressEntry {
                        name: "Home".into(),
                        city_name: city.into(),
                        commune: city.into(),
                        voivodeship: voivodeship.into(),
                        street_name: "Testowa".into(),
                        street_name_1: "Testowa".into(),
                        house_no: house_no.into(),
                        city_id: Some(city_id),
                        is_active: true,
                        ..Default::default()
                    }],
                    filter_by_house_no,
                    ..Settings::default()
                })
            };

            let mut included_without_filter = vec![alert()];
            apply_alert_filtering(&mut included_without_filter, &settings("25", false));
            assert_eq!(included_without_filter[0].is_local, Some(true), "{source}");

            let mut included_number = vec![alert()];
            apply_alert_filtering(&mut included_number, &settings("14", true));
            assert_eq!(included_number[0].is_local, Some(true), "{source}");

            let mut excluded_number = vec![alert()];
            apply_alert_filtering(&mut excluded_number, &settings("25", true));
            assert_eq!(excluded_number[0].is_local, Some(false), "{source}");
        }
    }
}
