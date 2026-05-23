use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::collections::HashMap;
use sha2::{Sha256, Digest};
use async_trait::async_trait;
#[cfg(test)]
use mockall::{automock, predicate::*};

// ── Alert source abstraction ──────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum AlertSource {
    #[default]
    Tauron,
    #[serde(rename = "mpwik_wroclaw")]
    MpwikWroclaw,
    #[serde(rename = "mpwik_warszawa")]
    MpwikWarszawa,
    Fortum,
    Energa,
    Enea,
    Pge,
    Stoen,
    Psg,
    Wmk,
    #[serde(rename = "tauron_heat")]
    TauronHeat,
    Aquanet,
    #[serde(rename = "katowickie_wodociagi")]
    KatowickieWodociagi,
    #[serde(rename = "veolia_warszawa")]
    VeoliaWarszawa,
    #[serde(rename = "veolia_poznan")]
    VeoliaPoznan,
}

impl AlertSource {
    pub fn service_voivodeships(&self) -> Option<Vec<&'static str>> {
        match self {
            AlertSource::Tauron => Some(vec![
                "DOLNOŚLĄSKIE",
                "MAŁOPOLSKIE",
                "OPOLSKIE",
                "ŚLĄSKIE",
                "ŚWIĘTOKRZYSKIE",
                "PODKARPACKIE",
            ]),
            AlertSource::Energa => Some(vec![
                "POMORSKIE",
                "WARMIŃSKO-MAZURSKIE",
                "KUJAWSKO-POMORSKIE",
                "ZACHODNIOPOMORSKIE",
                "MAZOWIECKIE",
                "WIELKOPOLSKIE",
                "ŁÓDZKIE",
            ]),
            AlertSource::Enea => Some(vec![
                "WIELKOPOLSKIE",
                "LUBUSKIE",
                "ZACHODNIOPOMORSKIE",
                "KUJAWSKO-POMORSKIE",
            ]),
            AlertSource::Pge => Some(vec![
                "PODLASKIE",
                "LUBELSKIE",
                "PODKARPACKIE",
                "ŚWIĘTOKRZYSKIE",
                "ŁÓDZKIE",
                "MAZOWIECKIE",
                "MAŁOPOLSKIE",
                "WIELKOPOLSKIE",
            ]),
            AlertSource::Stoen => Some(vec!["MAZOWIECKIE"]),
            AlertSource::MpwikWroclaw => Some(vec!["DOLNOŚLĄSKIE"]),
            AlertSource::MpwikWarszawa => Some(vec!["MAZOWIECKIE"]),
            AlertSource::Wmk => Some(vec!["MAŁOPOLSKIE"]),
            AlertSource::Aquanet => Some(vec!["WIELKOPOLSKIE"]),
            AlertSource::KatowickieWodociagi => Some(vec!["ŚLĄSKIE"]),
            AlertSource::VeoliaWarszawa => Some(vec!["MAZOWIECKIE"]),
            AlertSource::VeoliaPoznan => Some(vec!["WIELKOPOLSKIE"]),
            AlertSource::Fortum => Some(vec![
                "DOLNOŚLĄSKIE",
                "ŚLĄSKIE",
                "MAZOWIECKIE",
                "WIELKOPOLSKIE",
                "ŁÓDZKIE",
            ]),
            AlertSource::TauronHeat => Some(vec![
                "ŚLĄSKIE",
                "MAŁOPOLSKIE",
                "DOLNOŚLĄSKIE",
            ]),
            AlertSource::Psg => None, // Nationwide
        }
    }
}

#[cfg_attr(test, automock)]
pub trait DatabaseInterface {
    fn is_alert_seen(&self, provider: &str, hash: &str) -> Result<bool, String>;
    fn mark_alert_as_seen(&self, provider: &str, hash: &str) -> Result<(), String>;
}

#[cfg_attr(test, automock)]
pub trait NotificationProvider {
    fn show_notification(&self, title: String, body: String, hash: String);
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct UnifiedAlert {
    pub source: AlertSource,
    pub startDate: Option<String>,
    pub endDate: Option<String>,
    pub message: Option<String>,
    pub description: Option<String>,
    #[serde(default, rename = "addressIndex")]
    pub address_index: Option<usize>,
    #[serde(default, rename = "isLocal")]
    pub is_local: Option<bool>,
    #[serde(default)]
    pub hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AlertsResponse {
    pub alerts: Vec<UnifiedAlert>,
    pub is_stale: bool,
}

impl UnifiedAlert {
    pub fn to_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.source.to_string());
        if let Some(msg) = &self.message {
            hasher.update(msg);
        }
        if let Some(start) = &self.startDate {
            hasher.update(start);
        }
        format!("{:x}", hasher.finalize())
    }
}

pub fn deduplicate_alerts(alerts: Vec<UnifiedAlert>) -> Vec<UnifiedAlert> {
    let mut grouped_alerts: HashMap<String, UnifiedAlert> = HashMap::new();
    for mut alert in alerts {
        let hash = alert.hash.clone().unwrap_or_else(|| alert.to_hash());
        alert.hash = Some(hash.clone());

        if let Some(existing) = grouped_alerts.get_mut(&hash) {
            // Merge logic: prioritize local alerts and preserve address_index
            if alert.is_local == Some(true) && existing.is_local != Some(true) {
                existing.is_local = Some(true);
                existing.address_index = alert.address_index;
                if alert.message.is_some() {
                    existing.message = alert.message.clone();
                }
                if alert.description.is_some() {
                    existing.description = alert.description.clone();
                }
            }
        } else {
            grouped_alerts.insert(hash, alert);
        }
    }
    grouped_alerts.into_values().collect()
}

pub struct MonitorEngine<'a> {
    pub db: &'a dyn DatabaseInterface,
    pub notifier: &'a dyn NotificationProvider,
    pub settings: &'a Settings,
}

impl<'a> MonitorEngine<'a> {
    pub fn new(db: &'a dyn DatabaseInterface, notifier: &'a dyn NotificationProvider, settings: &'a Settings) -> Self {
        Self { db, notifier, settings }
    }

    pub fn process_alerts(&self, alerts: Vec<UnifiedAlert>) {
        let enabled_sources: Vec<String> = self.settings.enabled_sources.clone().unwrap_or_default();
        
        log::info!("MonitorEngine: processing {} alerts", alerts.len());
        for alert in alerts {
            let hash = alert.hash.clone().unwrap_or_else(|| alert.to_hash());
            if alert.is_local != Some(true) {
                log::debug!("Alert {} skipped: not local (is_local={:?})", hash, alert.is_local);
                continue;
            }

            let source_key = alert.source.to_string();
            if !enabled_sources.contains(&source_key) {
                log::debug!("Alert {} skipped: source {} not enabled", hash, source_key);
                continue;
            }

            // Skip alerts that have already expired — this prevents historical outages from
            // triggering notifications when a provider is newly enabled.
            if let Some(end_str) = &alert.endDate {
                if let Some(end_dt) = crate::utils::parse_date(end_str) {
                    if end_dt < chrono::Utc::now() {
                        log::debug!("Alert {} skipped: already expired (end={})", hash, end_str);
                        continue;
                    }
                }
            }

            let notified_enabled = self.settings.notification_preferences.get(&source_key).copied().unwrap_or(false);

            if notified_enabled {
                let mut already_notified_as_upcoming = false;

                // --- UPCOMING NOTIFICATION ---
                if self.settings.upcoming_notification_enabled {
                    if let Some(start_str) = &alert.startDate {
                        if let Some(start_dt) = crate::utils::parse_date(start_str) {
                            let now_utc = chrono::Utc::now();
                            let diff_hours = (start_dt - now_utc).num_hours();
                            
                            if diff_hours >= 0 && diff_hours <= self.settings.upcoming_notification_hours as i64 {
                                let upcoming_hash = format!("upcoming_{}", hash);
                                match self.db.is_alert_seen(&source_key, &upcoming_hash) {
                                    Ok(false) => {
                                        log::info!("Triggering UPCOMING notification for alert {}", hash);
                                        let title = format_notification_title(&alert, self.settings, true);
                                        let body = format_notification_body(&alert, self.settings);
                                        self.notifier.show_notification(title, body, hash.clone());
                                        self.db.mark_alert_as_seen(&source_key, &upcoming_hash).ok();
                                        already_notified_as_upcoming = true;
                                    },
                                    Ok(true) => log::debug!("Upcoming alert {} already seen", hash),
                                    Err(e) => log::error!("Database error checking upcoming seen status for {}: {}", hash, e),
                                }
                            } else {
                                log::debug!("Upcoming alert {} skipped: diff_hours={} not in window", hash, diff_hours);
                            }
                        }
                    }
                }

                // --- NEW ALERT NOTIFICATION ---
                match self.db.is_alert_seen(&source_key, &hash) {
                    Ok(false) => {
                        if !already_notified_as_upcoming {
                            log::info!("Triggering NEW alert notification for hash {}", hash);
                            let title = format_notification_title(&alert, self.settings, false);
                            let body = format_notification_body(&alert, self.settings);
                            self.notifier.show_notification(title, body, hash.clone());
                        } else {
                            log::info!("Skipping NEW alert notification for hash {} (already notified as upcoming)", hash);
                        }
                        self.db.mark_alert_as_seen(&source_key, &hash).ok();
                    },
                    Ok(true) => log::debug!("Alert {} already seen, skipping notification.", hash),
                    Err(e) => log::error!("Database error checking seen status for {}: {}", hash, e),
                }
            } else {
                log::debug!("Alert {} skipped: notifications disabled for {}", hash, source_key);
            }
        }
    }
}

// These functions were extracted from lib.rs but kept identical in logic
pub fn format_notification_title(alert: &UnifiedAlert, settings: &Settings, is_upcoming: bool) -> String {
    let is_pl = match settings.language.as_deref() {
        Some("pl") => true,
        Some("en") => false,
        _ => {
            // For "system" or None, we default to Polish as it's the primary market
            // and the content (street names, provider messages) is in Polish.
            true
        }
    };
    
    let label = match alert.source {
        AlertSource::Tauron | AlertSource::Energa | AlertSource::Enea | AlertSource::Pge | AlertSource::Stoen => {
            if is_pl { "awaria prądu" } else { "power outage" }
        }
        AlertSource::MpwikWroclaw | AlertSource::MpwikWarszawa | AlertSource::Wmk | AlertSource::Aquanet | AlertSource::KatowickieWodociagi => {
            if is_pl { "awaria wody" } else { "water outage" }
        }
        AlertSource::Fortum | AlertSource::TauronHeat | AlertSource::VeoliaWarszawa | AlertSource::VeoliaPoznan => {
            if is_pl { "awaria ogrzewania" } else { "heat outage" }
        }
        AlertSource::Psg => {
            if is_pl { "awaria gazu" } else { "gas outage" }
        }
    };
    
    let prefix = if is_upcoming {
        if is_pl { "Nadchodząca" } else { "Upcoming" }
    } else if is_pl { "Nowa" } else { "New" };
    
    let title = format!("{} {}", prefix, label);
    
    if let Some(idx) = alert.address_index {
        if let Some(addr) = settings.addresses.get(idx) {
            return format!("{}: {}", addr.name, title);
        }
    }
    title
}

pub fn format_notification_body(alert: &UnifiedAlert, settings: &Settings) -> String {
    let mut body = alert.message.clone().unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    
    let mut time_info = Vec::new();
    if let Some(start) = &alert.startDate {
        if let Some(dt) = crate::utils::parse_date(start) {
            time_info.push(crate::utils::format_date(dt));
        } else {
            time_info.push(start.clone());
        }
    }
    if let Some(end) = &alert.endDate {
        if let Some(dt) = crate::utils::parse_date(end) {
            time_info.push(crate::utils::format_date(dt));
        } else {
            time_info.push(end.clone());
        }
    }
    
    if !time_info.is_empty() {
        let times = time_info.join(" - ");
        if !body.contains(&times) {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(&times);
        }
    }
    if body.is_empty() {
        let is_pl = match settings.language.as_deref() {
            Some("pl") => true,
            Some("en") => false,
            _ => true,
        };
        return if is_pl { "Nowe zdarzenie".to_string() } else { "New event".to_string() };
    }
    body
}

impl std::fmt::Display for AlertSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AlertSource::Tauron => "tauron",
            AlertSource::MpwikWroclaw => "mpwik_wroclaw",
            AlertSource::MpwikWarszawa => "mpwik_warszawa",
            AlertSource::Fortum => "fortum",
            AlertSource::Energa => "energa",
            AlertSource::Enea => "enea",
            AlertSource::Pge => "pge",
            AlertSource::Stoen => "stoen",
            AlertSource::Psg => "psg",
            AlertSource::Wmk => "wmk",
            AlertSource::TauronHeat => "tauron_heat",
            AlertSource::Aquanet => "aquanet",
            AlertSource::KatowickieWodociagi => "katowickie_wodociagi",
            AlertSource::VeoliaWarszawa => "veolia_warszawa",
            AlertSource::VeoliaPoznan => "veolia_poznan",
        };
        write!(f, "{}", s)
    }
}

#[async_trait]
pub trait AlertProvider: Send + Sync {
    fn id(&self) -> String;
    fn source(&self) -> AlertSource;
    async fn fetch(
        &self,
        client: &Client,
        client_http1: &Client,
        settings: &Settings,
        app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>);
}

pub fn is_provider_applicable(source: AlertSource, settings: &Settings) -> bool {
    let voivodeships = match source.service_voivodeships() {
        Some(v) => v,
        None => return true, // Nationwide
    };

    let active_addresses: Vec<_> = settings.addresses.iter().filter(|a| a.is_active).collect();
    if active_addresses.is_empty() {
        // If there are no active addresses, we don't have a basis for pre-filtration.
        // We return true to allow fetching (likely resulting in 0 alerts if addresses are empty,
        // or general city alerts if applicable).
        return true;
    }

    active_addresses.iter().any(|a| {
        if a.voivodeship.is_empty() {
            // Fallback for missing voivodeship data
            return true;
        }
        let v = a.voivodeship.trim().to_uppercase();
        voivodeships.iter().any(|&sv| {
            if sv == v { return true; }
            let sv_norm = sv.replace("Ł", "L").replace("Ś", "S");
            let v_norm = v.replace("Ł", "L").replace("Ś", "S");
            if sv_norm == v_norm { return true; }
            
            // Handle `?` corruption (e.g. MA?OPOLSKIE)
            if v.contains('?') && v.len() == sv.len() {
                let mut match_with_wildcard = true;
                for (c1, c2) in v.chars().zip(sv.chars()) {
                    if c1 != '?' && c1 != c2 {
                        match_with_wildcard = false;
                        break;
                    }
                }
                if match_with_wildcard { return true; }
            }
            false
        })
    })
}

pub fn is_wroclaw(addr: &AddressEntry) -> bool {
    let name = addr.city_name.trim().to_lowercase();
    name.starts_with("wrocław") || name.starts_with("wroclaw") || addr.city_id == Some(986283)
}

pub fn is_warszawa(addr: &AddressEntry) -> bool {
    let name = addr.city_name.trim().to_lowercase();
    name.starts_with("warszawa") || name.starts_with("warsaw") || addr.city_id == Some(918123)
}

pub fn is_krakow(addr: &AddressEntry) -> bool {
    let name = addr.city_name.trim().to_lowercase();
    name.starts_with("kraków") || name.starts_with("krakow") || addr.city_id == Some(950463)
}



// ── Address & Settings ────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AddressEntry {
    pub name: String,
    pub city_name: String,
    #[serde(default)]
    pub voivodeship: String,
    #[serde(default)]
    pub district: String,
    #[serde(default)]
    pub commune: String,
    pub street_name: String,
    #[serde(default)]
    pub street_name_1: String,
    #[serde(default)]
    pub street_name_2: Option<String>,
    pub house_no: String,
    #[serde(default)]
    pub city_id: Option<u64>,
    #[serde(default)]
    pub street_id: Option<u64>,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub addresses: Vec<AddressEntry>,
    pub primary_address_index: Option<usize>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub enabled_sources: Option<Vec<String>>,
    #[serde(default)]
    pub notification_preferences: HashMap<String, bool>,
    #[serde(default)]
    pub upcoming_notification_enabled: bool,
    #[serde(default = "default_upcoming_hours")]
    pub upcoming_notification_hours: u32,
    #[serde(default = "default_true")]
    pub show_other_outages: bool,
}

fn default_upcoming_hours() -> u32 {
    24
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            addresses: Vec::new(),
            primary_address_index: None,
            theme: None,
            language: None,
            enabled_sources: Some(Vec::new()),
            notification_preferences: HashMap::new(),
            upcoming_notification_enabled: false,
            upcoming_notification_hours: 24,
            show_other_outages: true,
        }
    }
}

pub fn save_settings_to_path(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_settings_from_path(path: &std::path::Path) -> Result<Option<Settings>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    if data.trim().is_empty() {
        return Ok(None);
    }
    let settings: Settings = serde_json::from_str(&data)
        .map_err(|e| format!("Settings parse error (might be empty/corrupt): {}", e))?;
    Ok(Some(settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate;

    #[test]
    fn test_settings_serialization() {
        let settings = Settings {
            theme: Some("dark".to_string()),
            language: Some("pl".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_address_entry_with_teryt_ids() {
        let addr = AddressEntry {
            name: "Home".to_string(),
            city_name: "Wrocław".to_string(),
            voivodeship: "".to_string(),
            district: "".to_string(),
            commune: "".to_string(),
            street_name: "ul. Kuźnicza".to_string(),
            street_name_1: "Kuźnicza".to_string(),
            street_name_2: None,
            house_no: "25".to_string(),
            city_id: Some(969400),
            street_id: Some(13900),
            is_active: true,
        };
        let json = serde_json::to_string(&addr).unwrap();
        let deserialized: AddressEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(addr, deserialized);
    }

    #[test]
    fn test_settings_persistence() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("test_settings.json");

        let settings = Settings {
            theme: Some("light".to_string()),
            language: Some("en".to_string()),
            ..Default::default()
        };

        save_settings_to_path(&test_path, &settings).expect("Failed to save settings");
        let loaded = load_settings_from_path(&test_path).expect("Failed to load settings");
        assert_eq!(Some(settings), loaded);

        std::fs::remove_file(test_path).ok();
    }

    #[test]
    fn test_load_non_existent_settings() {
        let test_path = std::path::Path::new("non_existent_settings.json");
        let loaded = load_settings_from_path(test_path).expect("Failed to load settings");
        assert_eq!(None, loaded);
    }

    #[test]
    fn test_unified_alert_hashing() {
        let alert1 = UnifiedAlert {
            source: AlertSource::Tauron,
            startDate: Some("2024-01-01 10:00".to_string()),
            endDate: None,
            message: Some("Brak prądu".to_string()),
            description: None,
            address_index: None,
            is_local: None,
            hash: None,
        };

        let alert2 = UnifiedAlert {
            source: AlertSource::Tauron,
            startDate: Some("2024-01-01 10:00".to_string()),
            endDate: Some("2024-01-01 14:00".to_string()),
            message: Some("Brak prądu".to_string()),
            description: Some("Different desc".to_string()),
            address_index: Some(1),
            is_local: Some(true),
            hash: None,
        };

        // Hashes should match if source, message, and startDate match (ignoring desc/endDate etc.)
        assert_eq!(alert1.to_hash(), alert2.to_hash());

        let alert3 = UnifiedAlert {
            source: AlertSource::Energa,
            ..alert1.clone()
        };
        assert_ne!(alert1.to_hash(), alert3.to_hash());

        let alert4 = UnifiedAlert {
            message: Some("Inny komunikat".to_string()),
            ..alert1.clone()
        };
        assert_ne!(alert1.to_hash(), alert4.to_hash());
    }

    #[test]
    fn test_unified_alert_sorting() {
        let mut alerts = [
            UnifiedAlert {
                source: AlertSource::Tauron,
                startDate: Some("2024-05-20 12:00".to_string()),
                endDate: None,
                message: None,
                description: None,
                address_index: None,
                is_local: None,
                hash: None,
            },
            UnifiedAlert {
                source: AlertSource::Energa,
                startDate: Some("2024-05-20 10:00".to_string()),
                endDate: None,
                message: None,
                description: None,
                address_index: None,
                is_local: None,
                hash: None,
            },
            UnifiedAlert {
                source: AlertSource::MpwikWroclaw,
                startDate: None,
                endDate: None,
                message: None,
                description: None,
                address_index: None,
                is_local: None,
                hash: None,
            },
        ];

        alerts.sort_by(|a, b| {
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

        assert_eq!(alerts[0].source, AlertSource::Energa); // 10:00
        assert_eq!(alerts[1].source, AlertSource::Tauron); // 12:00
        assert_eq!(alerts[2].source, AlertSource::MpwikWroclaw);  // None
    }

    #[test]
    fn test_deduplicate_alerts() {
        let alerts = vec![
            UnifiedAlert {
                source: AlertSource::Tauron,
                message: Some("Outage".to_string()),
                is_local: Some(false),
                ..Default::default()
            },
            UnifiedAlert {
                source: AlertSource::Tauron,
                message: Some("Outage".to_string()),
                is_local: Some(true),
                address_index: Some(5),
                ..Default::default()
            },
        ];

        let deduplicated = deduplicate_alerts(alerts);
        assert_eq!(deduplicated.len(), 1);
        assert_eq!(deduplicated[0].is_local, Some(true));
        assert_eq!(deduplicated[0].address_index, Some(5));
    }

    #[test]
    fn test_monitor_engine_notification_flow() {
        let mut mock_db = MockDatabaseInterface::new();
        let mut mock_notifier = MockNotificationProvider::new();

        let settings = Settings {
            notification_preferences: [("tauron".to_string(), true)].into(),
            enabled_sources: Some(vec!["tauron".to_string()]),
            ..Default::default()
        };

        let alerts = vec![
            UnifiedAlert {
                source: AlertSource::Tauron,
                message: Some("Brak prądu".to_string()),
                is_local: Some(true),
                ..Default::default()
            }
        ];

        let hash = alerts[0].to_hash();

        mock_db.expect_is_alert_seen()
            .with(predicate::eq("tauron"), predicate::eq(hash.clone()))
            .times(1)
            .returning(|_, _| Ok(false));

        mock_notifier.expect_show_notification()
            .with(predicate::always(), predicate::eq("Brak prądu".to_string()), predicate::eq(hash.clone()))
            .times(1)
            .returning(|_, _, _| ());

        mock_db.expect_mark_alert_as_seen()
            .with(predicate::eq("tauron"), predicate::eq(hash.clone()))
            .times(1)
            .returning(|_, _| Ok(()));

        let engine = MonitorEngine::new(&mock_db, &mock_notifier, &settings);
        engine.process_alerts(alerts);
    }

    #[test]
    fn test_monitor_engine_skip_seen() {
        let mut mock_db = MockDatabaseInterface::new();
        let mut mock_notifier = MockNotificationProvider::new();

        let settings = Settings {
            notification_preferences: [("tauron".to_string(), true)].into(),
            enabled_sources: Some(vec!["tauron".to_string()]),
            ..Default::default()
        };

        let alerts = vec![
            UnifiedAlert {
                source: AlertSource::Tauron,
                message: Some("Brak prądu".to_string()),
                is_local: Some(true),
                ..Default::default()
            }
        ];

        mock_db.expect_is_alert_seen()
            .returning(|_, _| Ok(true));

        mock_notifier.expect_show_notification().times(0);

        let engine = MonitorEngine::new(&mock_db, &mock_notifier, &settings);
        engine.process_alerts(alerts);
    }

    #[test]
    fn test_monitor_engine_upcoming_notification() {
        let mut mock_db = MockDatabaseInterface::new();
        let mut mock_notifier = MockNotificationProvider::new();

        // Outage starts in 2 hours
        let start_time = (chrono::Utc::now() + chrono::Duration::hours(2)).format("%Y-%m-%d %H:%M:%S").to_string();

        let settings = Settings {
            notification_preferences: [("tauron".to_string(), true)].into(),
            enabled_sources: Some(vec!["tauron".to_string()]),
            upcoming_notification_enabled: true,
            upcoming_notification_hours: 24,
            ..Default::default()
        };

        let alerts = vec![
            UnifiedAlert {
                source: AlertSource::Tauron,
                startDate: Some(start_time),
                message: Some("Planowana przerwa".to_string()),
                is_local: Some(true),
                ..Default::default()
            }
        ];

        let hash = alerts[0].to_hash();
        let upcoming_hash = format!("upcoming_{}", hash);

        mock_db.expect_is_alert_seen()
            .with(predicate::eq("tauron"), predicate::eq(upcoming_hash.clone()))
            .times(1)
            .returning(|_, _| Ok(false));

        mock_db.expect_is_alert_seen()
            .with(predicate::eq("tauron"), predicate::eq(hash.clone()))
            .times(1)
            .returning(|_, _| Ok(false));

        mock_notifier.expect_show_notification().times(1)
            .returning(|_, _, _| ());

        mock_db.expect_mark_alert_as_seen().times(2).returning(|_, _| Ok(()));

        let engine = MonitorEngine::new(&mock_db, &mock_notifier, &settings);
        engine.process_alerts(alerts);
    }

    #[test]
    fn test_format_notification_body_with_custom_dates() {
        let alert = UnifiedAlert {
            source: AlertSource::Psg,
            startDate: Some("2024-05-20 10:00".to_string()),
            endDate: Some("termin zostanie podany wkrótce".to_string()),
            message: Some("Prace serwisowe".to_string()),
            ..Default::default()
        };

        let settings = Settings::default();
        let body = format_notification_body(&alert, &settings);
        // "2024-05-20 10:00" parses to "20-05-2024 10:00"
        // "termin zostanie podany wkrótce" remains as is
        assert!(body.contains("20-05-2024 10:00"));
        assert!(body.contains("termin zostanie podany wkrótce"));
        assert!(body.contains("Prace serwisowe"));
    }

    #[test]
    fn test_is_provider_applicable() {
        let mut settings = Settings::default();
        settings.addresses.push(AddressEntry {
            city_name: "Wrocław".to_string(),
            voivodeship: "Dolnośląskie".to_string(),
            is_active: true,
            ..Default::default()
        });

        // Tauron is nationwide (basically) but serves Wrocław
        assert!(is_provider_applicable(AlertSource::Tauron, &settings));
        
        // MPWiK Wrocław is local to Wrocław
        assert!(is_provider_applicable(AlertSource::MpwikWroclaw, &settings));
        
        // Stoen is Warsaw only
        assert!(!is_provider_applicable(AlertSource::Stoen, &settings));
        
        // Energa does not serve Dolnośląskie
        assert!(!is_provider_applicable(AlertSource::Energa, &settings));

        // Now add a Warsaw address
        settings.addresses.push(AddressEntry {
            city_name: "Warszawa".to_string(),
            voivodeship: "Mazowieckie".to_string(),
            is_active: true,
            ..Default::default()
        });

        // Now Stoen and Energa should be applicable
        assert!(is_provider_applicable(AlertSource::Stoen, &settings));
        assert!(is_provider_applicable(AlertSource::Energa, &settings));
        
        // If address is inactive, it shouldn't count
        settings.addresses[1].is_active = false;
        assert!(!is_provider_applicable(AlertSource::Stoen, &settings));
    }

    #[test]
    fn test_monitor_engine_skip_expired() {
        let mock_db = MockDatabaseInterface::new();
        let mock_notifier = MockNotificationProvider::new();

        let settings = Settings {
            notification_preferences: [("tauron".to_string(), true)].into(),
            enabled_sources: Some(vec!["tauron".to_string()]),
            ..Default::default()
        };

        // Outage that ended 1 hour ago
        let past_end = (chrono::Utc::now() - chrono::Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();

        let alerts = vec![
            UnifiedAlert {
                source: AlertSource::Tauron,
                endDate: Some(past_end),
                message: Some("Stara awaria".to_string()),
                is_local: Some(true),
                ..Default::default()
            }
        ];

        // is_alert_seen and show_notification should NEVER be called for an expired alert
        // (mockall panics if unexpected calls are made — no .expect() calls needed)
        let engine = MonitorEngine::new(&mock_db, &mock_notifier, &settings);
        engine.process_alerts(alerts);
    }
}
