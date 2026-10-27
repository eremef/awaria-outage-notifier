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
    #[serde(rename = "pge_torun_heat")]
    PgeTorunHeat,
    Stoen,
    Psg,
    Wmk,
    #[serde(rename = "wm_radom")]
    WmRadom,
    #[serde(rename = "tauron_heat")]
    TauronHeat,
    Aquanet,
    #[serde(rename = "katowickie_wodociagi")]
    KatowickieWodociagi,
    #[serde(rename = "veolia_warszawa")]
    VeoliaWarszawa,
    #[serde(rename = "veolia_poznan")]
    VeoliaPoznan,
    #[serde(rename = "veolia_lodz")]
    VeoliaLodz,
    #[serde(rename = "zwik_lodz")]
    ZwikLodz,
    #[serde(rename = "zwik_zielona_gora")]
    ZwikZielonaGora,
    #[serde(rename = "wodociagi_plockie")]
    WodociagiPlockie,
    #[serde(rename = "wodociagi_kieleckie")]
    WodociagiKieleckie,
    #[serde(rename = "torunskie_wodociagi")]
    TorunskieWodociagi,
    #[serde(rename = "wodociagi_bialostockie")]
    WodociagiBialostockie,
    #[serde(rename = "pwik_kalisz")]
    PwikKalisz,
    #[serde(rename = "pwik_czestochowa")]
    PwikCzestochowa,
    #[serde(rename = "pwik_olsztyn")]
    PwikOlsztyn,
    #[serde(rename = "wik_opole")]
    WikOpole,
    #[serde(rename = "pewik_gdynia")]
    PewikGdynia,
    #[serde(rename = "gdanskie_wodociagi")]
    GdanskieWodociagi,
    #[serde(rename = "gpec")]
    Gpec,
    #[serde(rename = "opec_gdynia")]
    OpecGdynia,
    #[serde(rename = "kpec_bydgoszcz")]
    KpecBydgoszcz,
    #[serde(rename = "enea_cieplo")]
    EneaCieplo,
    #[serde(rename = "energa_cieplo_kaliskie")]
    EnergaCieploKaliskie,
    #[serde(rename = "eco_sa")]
    EcoSa,
    #[serde(rename = "puk_rokietnica")]
    PukRokietnica,
    Sec,
    Lpec,
    #[serde(rename = "mpwik_lublin")]
    MpwikLublin,
    #[serde(rename = "mpwik_rzeszow")]
    MpwikRzeszow,
    #[serde(rename = "mwik_bydgoszcz")]
    MwikBydgoszcz,
    #[serde(rename = "mpec_rzeszow")]
    MpecRzeszow,
    #[serde(rename = "mpec_kielce")]
    MpecKielce,
    #[serde(rename = "lpwik_legnica")]
    LpwikLegnica,
    #[serde(rename = "bytomskie_wodociagi")]
    BytomskieWodociagi,
    #[serde(rename = "pwik_gorzow")]
    PwikGorzow,
    #[serde(rename = "aqua_bielsko")]
    AquaBielsko,
    #[serde(rename = "epec_elblag")]
    EpecElblag,
    #[serde(rename = "pec_gliwice")]
    PecGliwice,
    #[serde(rename = "pwik_gliwice")]
    PwikGliwice,
    #[serde(rename = "dabrowskie_wodociagi")]
    DabrowskieWodociagi,
    #[serde(rename = "mwik_koszalin")]
    MwikKoszalin,
    #[serde(rename = "sosnowieckie_wodociagi")]
    SosnowieckieWodociagi,
    #[serde(rename = "tarnowskie_wodociagi")]
    TarnowskieWodociagi,
    #[serde(rename = "zabrzanskie_wodociagi")]
    ZabrzanskieWodociagi,
    #[serde(rename = "pwik_ruda_slaska")]
    PwikRudaSlaska,
    #[serde(rename = "pwik_rybnik")]
    PwikRybnik,
}

pub enum ServiceLocation {
    Nationwide,
    Voivodeships(Vec<&'static str>),
    Cities(Vec<fn(&AddressEntry) -> bool>),
}

impl AlertSource {
    pub fn service_locations(&self) -> ServiceLocation {
        match self {
            AlertSource::Tauron => ServiceLocation::Voivodeships(vec![
                "DOLNOŚLĄSKIE",
                "MAŁOPOLSKIE",
                "OPOLSKIE",
                "ŚLĄSKIE",
                "ŚWIĘTOKRZYSKIE",
                "PODKARPACKIE",
            ]),
            AlertSource::Energa => ServiceLocation::Voivodeships(vec![
                "POMORSKIE",
                "WARMIŃSKO-MAZURSKIE",
                "KUJAWSKO-POMORSKIE",
                "ZACHODNIOPOMORSKIE",
                "MAZOWIECKIE",
                "WIELKOPOLSKIE",
                "ŁÓDZKIE",
            ]),
            AlertSource::Enea => ServiceLocation::Voivodeships(vec![
                "WIELKOPOLSKIE",
                "LUBUSKIE",
                "ZACHODNIOPOMORSKIE",
                "KUJAWSKO-POMORSKIE",
            ]),
            AlertSource::Pge => ServiceLocation::Voivodeships(vec![
                "PODLASKIE",
                "LUBELSKIE",
                "PODKARPACKIE",
                "ŚWIĘTOKRZYSKIE",
                "ŁÓDZKIE",
                "MAZOWIECKIE",
                "MAŁOPOLSKIE",
                "WIELKOPOLSKIE",
            ]),
            AlertSource::PgeTorunHeat => ServiceLocation::Cities(vec![is_torun]),
            AlertSource::Fortum => ServiceLocation::Voivodeships(vec![
                "DOLNOŚLĄSKIE",
                "ŚLĄSKIE",
                "MAZOWIECKIE",
                "WIELKOPOLSKIE",
                "ŁÓDZKIE",
            ]),
            AlertSource::TauronHeat => ServiceLocation::Voivodeships(vec![
                "ŚLĄSKIE",
                "MAŁOPOLSKIE",
                "DOLNOŚLĄSKIE",
            ]),
            AlertSource::KatowickieWodociagi => ServiceLocation::Cities(vec![is_katowice]),
            AlertSource::WmRadom => ServiceLocation::Cities(vec![is_wm_radom_area]),
            AlertSource::WodociagiPlockie => ServiceLocation::Cities(vec![is_plock]),
            AlertSource::WodociagiKieleckie => {
                ServiceLocation::Cities(vec![is_wodociagi_kieleckie_area])
            }
            AlertSource::TorunskieWodociagi => {
                ServiceLocation::Cities(vec![is_torunskie_wodociagi_area])
            }
            AlertSource::WodociagiBialostockie => {
                ServiceLocation::Cities(vec![is_wodociagi_bialostockie_area])
            }
            AlertSource::PwikCzestochowa => ServiceLocation::Cities(vec![is_czestochowa]),
            AlertSource::PwikOlsztyn => ServiceLocation::Cities(vec![is_pwik_olsztyn_area]),
            AlertSource::WikOpole => ServiceLocation::Cities(vec![is_wik_opole_area]),
            AlertSource::ZwikZielonaGora => {
                ServiceLocation::Cities(vec![is_zwik_zielona_gora_area])
            }
            AlertSource::PewikGdynia => ServiceLocation::Cities(vec![is_pewik_gdynia_area]),
            AlertSource::Psg => ServiceLocation::Nationwide,
            
            AlertSource::Stoen => ServiceLocation::Cities(vec![is_warszawa]),
            AlertSource::MpwikWroclaw => ServiceLocation::Cities(vec![is_wroclaw]),
            AlertSource::MpwikWarszawa => ServiceLocation::Cities(vec![is_mpwik_warszawa_area]),
            AlertSource::Wmk => ServiceLocation::Cities(vec![is_krakow]),
            AlertSource::Aquanet => ServiceLocation::Cities(vec![is_poznan_area]),
            AlertSource::VeoliaWarszawa => ServiceLocation::Cities(vec![is_warszawa]),
            AlertSource::VeoliaPoznan => ServiceLocation::Cities(vec![is_poznan_area]),
            AlertSource::VeoliaLodz => ServiceLocation::Cities(vec![is_lodz]),
            AlertSource::ZwikLodz => ServiceLocation::Cities(vec![is_lodz]),
            AlertSource::PwikKalisz => ServiceLocation::Cities(vec![is_kalisz]),
            AlertSource::GdanskieWodociagi => ServiceLocation::Cities(vec![is_gdansk]),
            AlertSource::Gpec => ServiceLocation::Cities(vec![is_gdansk]),
            AlertSource::OpecGdynia => ServiceLocation::Cities(vec![is_opec_gdynia_area]),
            AlertSource::KpecBydgoszcz => ServiceLocation::Cities(vec![is_bydgoszcz]),
            AlertSource::MwikBydgoszcz => ServiceLocation::Cities(vec![is_bydgoszcz]),
            AlertSource::EneaCieplo => ServiceLocation::Cities(vec![is_bialystok]),
            AlertSource::EnergaCieploKaliskie => ServiceLocation::Cities(vec![is_kalisz]),
            // The bulletin spans several ECO companies; its complete heat-service
            // city roster is not published. Each notice must match its own city.
            AlertSource::EcoSa => ServiceLocation::Nationwide,
            AlertSource::PukRokietnica => ServiceLocation::Cities(vec![is_rokietnica]),
            AlertSource::Sec => ServiceLocation::Cities(vec![is_szczecin]),
            AlertSource::Lpec => ServiceLocation::Cities(vec![is_lublin]),
            AlertSource::MpwikLublin => ServiceLocation::Cities(vec![is_lublin]),
            AlertSource::MpwikRzeszow => ServiceLocation::Cities(vec![is_mpwik_rzeszow_area]),
            AlertSource::MpecRzeszow => ServiceLocation::Cities(vec![is_rzeszow]),
            AlertSource::MpecKielce => ServiceLocation::Cities(vec![is_kielce]),
            AlertSource::LpwikLegnica => ServiceLocation::Cities(vec![is_lpwik_legnica]),
            AlertSource::BytomskieWodociagi => ServiceLocation::Cities(vec![is_bytom]),
            AlertSource::PwikGorzow => ServiceLocation::Cities(vec![is_pwik_gorzow_area]),
            AlertSource::AquaBielsko => ServiceLocation::Cities(vec![is_aqua_bielsko_area]),
            AlertSource::EpecElblag => ServiceLocation::Cities(vec![is_elblag]),
            AlertSource::PecGliwice => ServiceLocation::Cities(vec![is_gliwice]),
            AlertSource::PwikGliwice => ServiceLocation::Cities(vec![is_pwik_gliwice_area]),
            AlertSource::DabrowskieWodociagi => {
                ServiceLocation::Cities(vec![is_dabrowskie_wodociagi_area])
            }
            AlertSource::MwikKoszalin => ServiceLocation::Cities(vec![is_koszalin]),
            AlertSource::SosnowieckieWodociagi => ServiceLocation::Cities(vec![is_sosnowiec]),
            AlertSource::TarnowskieWodociagi => {
                ServiceLocation::Cities(vec![is_tarnowskie_wodociagi_area])
            }
            AlertSource::ZabrzanskieWodociagi => ServiceLocation::Cities(vec![is_zabrze]),
            AlertSource::PwikRudaSlaska => ServiceLocation::Cities(vec![is_ruda_slaska]),
            AlertSource::PwikRybnik => ServiceLocation::Cities(vec![is_pwik_rybnik_area]),
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AlertSource::Tauron => "Tauron",
            AlertSource::MpwikWroclaw => "MPWiK Wrocław",
            AlertSource::MpwikWarszawa => "MPWiK Warszawa",
            AlertSource::Fortum => "Fortum",
            AlertSource::Energa => "Energa",
            AlertSource::Enea => "Enea",
            AlertSource::Pge => "PGE",
            AlertSource::PgeTorunHeat => "PGE Toruń",
            AlertSource::Stoen => "Stoen",
            AlertSource::Psg => "PSG",
            AlertSource::Wmk => "WMK Kraków",
            AlertSource::WmRadom => "WM Radom",
            AlertSource::TauronHeat => "Tauron Ciepło",
            AlertSource::Aquanet => "Aquanet",
            AlertSource::KatowickieWodociagi => "Katowickie Wodociągi",
            AlertSource::VeoliaWarszawa => "Veolia Warszawa",
            AlertSource::VeoliaPoznan => "Veolia Poznań",
            AlertSource::VeoliaLodz => "Veolia Łódź",
            AlertSource::ZwikLodz => "ZWiK Łódź",
            AlertSource::ZwikZielonaGora => "ZWiK Zielona Góra",
            AlertSource::WodociagiPlockie => "Wodociągi Płockie",
            AlertSource::WodociagiKieleckie => "Wodociągi Kieleckie",
            AlertSource::TorunskieWodociagi => "Toruńskie Wodociągi",
            AlertSource::WodociagiBialostockie => "Wodociągi Białostockie",
            AlertSource::PwikKalisz => "PWiK Kalisz",
            AlertSource::PwikCzestochowa => "PWiK Częstochowa",
            AlertSource::PwikOlsztyn => "PWiK Olsztyn",
            AlertSource::WikOpole => "WiK Opole",
            AlertSource::PewikGdynia => "PEWIK Gdynia",
            AlertSource::GdanskieWodociagi => "Gdańskie Wodociągi",
            AlertSource::Gpec => "GPEC Gdańsk",
            AlertSource::OpecGdynia => "OPEC Gdynia",
            AlertSource::KpecBydgoszcz => "KPEC Bydgoszcz",
            AlertSource::EneaCieplo => "ENEA Ciepło Białystok",
            AlertSource::EnergaCieploKaliskie => "Energa Ciepło Kaliskie",
            AlertSource::EcoSa => "ECO SA",
            AlertSource::Sec => "SEC Szczecin",
            AlertSource::Lpec => "LPEC Lublin",
            AlertSource::PukRokietnica => "PUK Rokietnica",
            AlertSource::MpwikLublin => "MPWiK Lublin",
            AlertSource::MpwikRzeszow => "MPWiK Rzeszów",
            AlertSource::MwikBydgoszcz => "MWiK Bydgoszcz",
            AlertSource::MpecRzeszow => "MPEC Rzeszów",
            AlertSource::MpecKielce => "MPEC Kielce",
            AlertSource::LpwikLegnica => "LPWiK Legnica",
            AlertSource::BytomskieWodociagi => "Bytomskie Wodociągi",
            AlertSource::PwikGorzow => "PWiK Gorzów Wielkopolski",
            AlertSource::AquaBielsko => "AQUA S.A.",
            AlertSource::EpecElblag => "EPEC Elbląg",
            AlertSource::PecGliwice => "PEC Gliwice",
            AlertSource::PwikGliwice => "PWiK Gliwice",
            AlertSource::DabrowskieWodociagi => "Dąbrowskie Wodociągi",
            AlertSource::MwikKoszalin => "MWiK Koszalin",
            AlertSource::SosnowieckieWodociagi => "Sosnowieckie Wodociągi",
            AlertSource::TarnowskieWodociagi => "Tarnowskie Wodociągi",
            AlertSource::ZabrzanskieWodociagi => "ZPWiK Zabrze",
            AlertSource::PwikRudaSlaska => "PWiK Ruda Śląska",
            AlertSource::PwikRybnik => "PWiK Rybnik",
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
    pub location: Option<String>,
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
    #[serde(default)]
    pub is_offline: bool,
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
        let digest = hasher.finalize();
        digest
            .iter()
            .map(|byte| format!("{:02x}", byte))
            .collect()
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
                if alert.location.is_some() {
                    existing.location = alert.location.clone();
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
            if let Some(end_str) = &alert.endDate
                && let Some(end_dt) = crate::utils::parse_date(end_str)
                    && end_dt < chrono::Utc::now() {
                        log::debug!("Alert {} skipped: already expired (end={})", hash, end_str);
                        continue;
                    }

            let notified_enabled = self.settings.notification_preferences.get(&source_key).copied().unwrap_or(false);

            if notified_enabled {
                let mut already_notified_as_upcoming = false;

                // --- UPCOMING NOTIFICATION ---
                if self.settings.upcoming_notification_enabled
                    && let Some(start_str) = &alert.startDate
                    // Date-only start values have no precise upcoming window.
                    && chrono::NaiveDate::parse_from_str(start_str, "%Y-%m-%d").is_err()
                        && let Some(start_dt) = crate::utils::parse_date(start_str) {
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
            if is_pl { "wyłączenie prądu" } else { "power outage" }
        }
        AlertSource::MpwikWroclaw
        | AlertSource::MpwikWarszawa
        | AlertSource::Wmk
        | AlertSource::WmRadom
        | AlertSource::Aquanet
        | AlertSource::KatowickieWodociagi
        | AlertSource::ZwikLodz
        | AlertSource::ZwikZielonaGora
        | AlertSource::WodociagiPlockie
        | AlertSource::WodociagiKieleckie
        | AlertSource::TorunskieWodociagi
        | AlertSource::WodociagiBialostockie
        | AlertSource::PwikKalisz
        | AlertSource::PwikCzestochowa
        | AlertSource::PwikGliwice
        | AlertSource::PwikOlsztyn
        | AlertSource::WikOpole
        | AlertSource::PewikGdynia
        | AlertSource::GdanskieWodociagi
        | AlertSource::PukRokietnica
        | AlertSource::MpwikLublin
        | AlertSource::MpwikRzeszow
        | AlertSource::MwikBydgoszcz
        | AlertSource::LpwikLegnica
        | AlertSource::BytomskieWodociagi
        | AlertSource::PwikGorzow
        | AlertSource::MwikKoszalin
        | AlertSource::SosnowieckieWodociagi
        | AlertSource::TarnowskieWodociagi
        | AlertSource::ZabrzanskieWodociagi
        | AlertSource::PwikRudaSlaska
        | AlertSource::PwikRybnik => {
            if is_pl {
                "wyłączenie wody"
            } else {
                "water outage"
            }
        }
        AlertSource::AquaBielsko => {
            if is_pl { "komunikat AQUA" } else { "AQUA notice" }
        }
        AlertSource::DabrowskieWodociagi => {
            if is_pl {
                "komunikat o usługach wodociągowych"
            } else {
                "water service notice"
            }
        }
        AlertSource::Fortum | AlertSource::TauronHeat | AlertSource::VeoliaWarszawa | AlertSource::VeoliaPoznan | AlertSource::VeoliaLodz | AlertSource::Gpec | AlertSource::OpecGdynia | AlertSource::KpecBydgoszcz | AlertSource::EneaCieplo | AlertSource::EnergaCieploKaliskie | AlertSource::EcoSa | AlertSource::Sec | AlertSource::Lpec | AlertSource::MpecRzeszow | AlertSource::MpecKielce | AlertSource::PgeTorunHeat | AlertSource::EpecElblag | AlertSource::PecGliwice => {
            if is_pl { "wyłączenie ogrzewania" } else { "heat outage" }
        }
        AlertSource::Psg => {
            if is_pl { "wyłączenie gazu" } else { "gas outage" }
        }
    };
    
    let prefix = if is_upcoming {
        if is_pl { "Nadchodzące" } else { "Upcoming" }
    } else if is_pl { "Nowe" } else { "New" };
    
    let outage_type = format!("{} {}", prefix, label);
    let provider_name = alert.source.display_name();
    
    if let Some(idx) = alert.address_index
        && let Some(addr) = settings.addresses.get(idx) {
            return format!("{}: {} - {}", addr.name, outage_type, provider_name);
        }
    format!("{} - {}", outage_type, provider_name)
}

pub fn format_notification_body(alert: &UnifiedAlert, settings: &Settings) -> String {
    let is_pl = match settings.language.as_deref() {
        Some("pl") => true,
        Some("en") => false,
        _ => true,
    };

    let format_alert_date = |value: &str| {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(|date| date.format("%d-%m-%Y").to_string())
            .unwrap_or_else(|_| {
                crate::utils::parse_date(value)
                    .map(crate::utils::format_date)
                    .unwrap_or_else(|| value.to_string())
            })
    };
    let mut time_info = Vec::new();
    if let Some(start) = &alert.startDate {
        time_info.push(format_alert_date(start));
    }
    if let Some(end) = &alert.endDate {
        time_info.push(format_alert_date(end));
    }
    let times = if time_info.is_empty() {
        if is_pl { "Brak danych".to_string() } else { "No data".to_string() }
    } else {
        time_info.join(" - ")
    };

    if let Some(idx) = alert.address_index
        && let Some(addr) = settings.addresses.get(idx) {
            let kiedy_lbl = if is_pl { "Kiedy" } else { "When" };
            let gdzie_lbl = if is_pl { "Gdzie" } else { "Where" };
            
            let mut saved_addr_str = String::new();
            if !addr.street_name.is_empty() {
                saved_addr_str.push_str(&addr.street_name);
                if settings.filter_by_house_no && !addr.house_no.is_empty() {
                    saved_addr_str.push(' ');
                    saved_addr_str.push_str(&addr.house_no);
                }
                if !addr.city_name.is_empty() {
                    saved_addr_str.push_str(", ");
                    saved_addr_str.push_str(&addr.city_name);
                }
            } else if !addr.city_name.is_empty() {
                saved_addr_str.push_str(&addr.city_name);
            }

            return format!("{}: {}\n{}: {}", kiedy_lbl, times, gdzie_lbl, saved_addr_str);
        }

    let mut body = alert.message.clone().unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    
    if !time_info.is_empty() && !body.contains(&times) {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&times);
    }
    
    if body.is_empty() {
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
            AlertSource::PgeTorunHeat => "pge_torun_heat",
            AlertSource::Stoen => "stoen",
            AlertSource::Psg => "psg",
            AlertSource::Wmk => "wmk",
            AlertSource::WmRadom => "wm_radom",
            AlertSource::TauronHeat => "tauron_heat",
            AlertSource::Aquanet => "aquanet",
            AlertSource::KatowickieWodociagi => "katowickie_wodociagi",
            AlertSource::VeoliaWarszawa => "veolia_warszawa",
            AlertSource::VeoliaPoznan => "veolia_poznan",
            AlertSource::VeoliaLodz => "veolia_lodz",
            AlertSource::ZwikLodz => "zwik_lodz",
            AlertSource::ZwikZielonaGora => "zwik_zielona_gora",
            AlertSource::WodociagiPlockie => "wodociagi_plockie",
            AlertSource::WodociagiKieleckie => "wodociagi_kieleckie",
            AlertSource::TorunskieWodociagi => "torunskie_wodociagi",
            AlertSource::WodociagiBialostockie => "wodociagi_bialostockie",
            AlertSource::PwikKalisz => "pwik_kalisz",
            AlertSource::PwikCzestochowa => "pwik_czestochowa",
            AlertSource::PwikOlsztyn => "pwik_olsztyn",
            AlertSource::WikOpole => "wik_opole",
            AlertSource::PewikGdynia => "pewik_gdynia",
            AlertSource::GdanskieWodociagi => "gdanskie_wodociagi",
            AlertSource::Gpec => "gpec",
            AlertSource::OpecGdynia => "opec_gdynia",
            AlertSource::KpecBydgoszcz => "kpec_bydgoszcz",
            AlertSource::EneaCieplo => "enea_cieplo",
            AlertSource::EnergaCieploKaliskie => "energa_cieplo_kaliskie",
            AlertSource::EcoSa => "eco_sa",
            AlertSource::PukRokietnica => "puk_rokietnica",
            AlertSource::Sec => "sec",
            AlertSource::Lpec => "lpec",
            AlertSource::MpwikLublin => "mpwik_lublin",
            AlertSource::MpwikRzeszow => "mpwik_rzeszow",
            AlertSource::MwikBydgoszcz => "mwik_bydgoszcz",
            AlertSource::MpecRzeszow => "mpec_rzeszow",
            AlertSource::MpecKielce => "mpec_kielce",
            AlertSource::LpwikLegnica => "lpwik_legnica",
            AlertSource::BytomskieWodociagi => "bytomskie_wodociagi",
            AlertSource::PwikGorzow => "pwik_gorzow",
            AlertSource::AquaBielsko => "aqua_bielsko",
            AlertSource::EpecElblag => "epec_elblag",
            AlertSource::PecGliwice => "pec_gliwice",
            AlertSource::PwikGliwice => "pwik_gliwice",
            AlertSource::DabrowskieWodociagi => "dabrowskie_wodociagi",
            AlertSource::MwikKoszalin => "mwik_koszalin",
            AlertSource::SosnowieckieWodociagi => "sosnowieckie_wodociagi",
            AlertSource::TarnowskieWodociagi => "tarnowskie_wodociagi",
            AlertSource::ZabrzanskieWodociagi => "zabrzanskie_wodociagi",
            AlertSource::PwikRudaSlaska => "pwik_ruda_slaska",
            AlertSource::PwikRybnik => "pwik_rybnik",
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

pub fn is_address_applicable_for_provider(source: &AlertSource, a: &AddressEntry) -> bool {
    match source.service_locations() {
        ServiceLocation::Nationwide => true,
        ServiceLocation::Cities(city_checkers) => {
            city_checkers.iter().any(|check| check(a))
        },
        ServiceLocation::Voivodeships(voivodeships) => {
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
        }
    }
}

pub fn is_provider_applicable(source: AlertSource, settings: &Settings) -> bool {
    let active_addresses: Vec<_> = settings.addresses.iter().filter(|a| a.is_active).collect();
    if active_addresses.is_empty() {
        // If there are no active addresses, we don't have a basis for pre-filtration.
        // We return true to allow fetching (likely resulting in 0 alerts if addresses are empty,
        // or general city alerts if applicable).
        return true;
    }

    let res = active_addresses.iter().any(|a| is_address_applicable_for_provider(&source, a));
    if matches!(source, AlertSource::Gpec) {
        log::info!("is_provider_applicable for Gpec returned {}, active={:?}", res, active_addresses.iter().map(|a| a.voivodeship.clone()).collect::<Vec<_>>());
    }
    res
}

macro_rules! city_checker {
    ($func_name:ident, $city_id:expr, $($name:expr),+) => {
        #[allow(dead_code)]
        pub fn $func_name(addr: &AddressEntry) -> bool {
            let name = addr.city_name.trim().to_lowercase();
            
            $(
                if name.starts_with($name) { return true; }
            )+

            if let Some(id) = $city_id {
                if addr.city_id == Some(id) { return true; }
            }

            let name_norm = name.replace("ó", "o").replace("ż", "z").replace("ł", "l").replace("ś", "s").replace("ć", "c").replace("ą", "a").replace("ę", "e").replace("ń", "n").replace("ź", "z");

            $(
                if name_norm.starts_with($name) { return true; }
            )+

            false
        }
    };
}

city_checker!(is_wroclaw, Some(986283_u64), "wrocław", "wroclaw");
city_checker!(is_warszawa, Some(918123_u64), "warszawa", "warsaw");
city_checker!(is_mpwik_warszawa_area, Some(918123_u64), "aleksandrow", "aleksandrów", "anusinek", "beniaminow", "beniaminów", "bialobrzegi", "białobrzegi", "boleslawowo", "bolesławowo", "borowa gora", "borowa góra", "cupel", "czarna struga", "dawidy", "dawidy bankowe", "debe", "debinki", "debkowizna", "derlacz", "dosin", "dębe", "dębinki", "dębkowizna", "falenty", "falenty duze", "falenty duże", "falenty nowe", "gasiorowo", "gora", "guty", "góra", "gąsiorowo", "izabelin", "izbica", "jachranka", "jadwisin", "janki", "janowek pierwszy", "janówek pierwszy", "jaworowa", "jozefow", "jozefow drugi", "józefów", "józefów drugi", "kaluszyn", "kania nowa", "kania polska", "karolino", "katy wegierskie", "kałuszyn", "kepa kikolska", "komornica", "komorow", "komorów", "kontrowers", "krubin", "kwietniowka", "kwietniówka", "kąty węgierskie", "kępa kikolska", "lacha", "lady", "lajski", "laszczki", "ludwinowo debskie", "ludwinowo dębskie", "ludwinowo zegrzynskie", "ludwinowo zegrzyńskie", "lysa gora", "marynino", "michalow-grabina", "michalow-reginow", "michalowice", "michalowice-wies", "michałowice", "michałowice-wieś", "michałów-grabina", "michałów-reginów", "nieporet", "nieporęt", "nowa wies", "nowa wieś", "nowe grocholice", "olszewnica", "olszewnica nowa", "olszewnica stara", "opacz mala", "opacz mała", "opacz-kolonia", "pecice", "pecice male", "piastow", "piastów", "poddebie", "poddębie", "podolszyn nowy", "poniatow", "poniatów", "pruszkow", "pruszków", "puchaly", "puchały", "pęcice", "pęcice małe", "raszyn", "reguly", "reguły", "rembelszczyzna", "rybaki", "rybie", "rynia", "sekocin nowy", "sekocin stary", "sekocin-las", "serock", "sikory", "skrzeszew", "skubianka", "skubianka-kolonia", "slomin", "sokolow", "sokołów", "stanislawow drugi", "stanislawow pierwszy", "stanislawowo", "stanisławowo", "stanisławów drugi", "stanisławów pierwszy", "stasi las", "suchy las", "swiecienica", "szadki", "sękocin nowy", "sękocin stary", "sękocin-las", "słomin", "topolina", "warsaw", "warszawa", "wieliszew", "wieliszew-kolonia", "wierzbica", "wola aleksandra", "wola kielpinska", "wola kiełpińska", "wola smolana", "wolka radzyminska", "wypedy", "wypędy", "wólka radzymińska", "zablocie", "zabłocie", "zagroby", "zalesie borowe", "zegrze", "zegrze poludniowe", "zegrze południowe", "zegrzynek", "łacha", "łady", "łajski", "łysa góra", "święcienica");
city_checker!(is_krakow, Some(950463_u64), "kraków", "krakow");
city_checker!(is_lodz, Some(958153_u64), "łódź", "lodz");
city_checker!(is_kalisz, Some(936579_u64), "kalisz");
city_checker!(is_szczecin, Some(977976_u64), "szczecin");
city_checker!(is_lublin, Some(959423_u64), "lublin");
city_checker!(is_kielce, Some(945930_u64), "kielce");
fn is_lpwik_legnica(addr: &AddressEntry) -> bool {
    if let Some(city_id) = addr.city_id {
        return city_id == 954047;
    }

    let city = crate::utils::clean_text(&addr.city_name);
    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    city.trim().starts_with("legnica")
        && (voivodeship.trim().is_empty() || voivodeship.trim().starts_with("dolnoslaskie"))
}

fn is_pwik_gorzow_area(addr: &AddressEntry) -> bool {
    const SERVICE_COMMUNES: [&str; 5] = [
        "bogdaniec",
        "deszczno",
        "klodawa",
        "lubiszyn",
        "santok",
    ];
    const SERVICE_LOCALITY_IDS: [u64; 6] = [
        935_140,  // Gorzów Wielkopolski
        178_672, // Bogdaniec
        179_520, // Deszczno
        181_817, // Kłodawa
        182_662, // Lubiszyn
        186_708, // Santok
    ];

    let city = crate::utils::clean_text(&addr.city_name);
    let mut commune = crate::utils::clean_text(&addr.commune);
    for prefix in ["gmina ", "gm "] {
        if let Some(stripped) = commune.strip_prefix(prefix) {
            commune = stripped.to_string();
            break;
        }
    }
    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    if !voivodeship.is_empty() && !voivodeship.starts_with("lubuskie") {
        return false;
    }

    if SERVICE_LOCALITY_IDS.contains(&addr.city_id.unwrap_or_default()) {
        return true;
    }

    if city == "gorzow wielkopolski" || city == "gorzow wlkp" {
        return true;
    }

    SERVICE_COMMUNES
        .iter()
        .any(|name| commune == *name || city == *name)
}

fn is_aqua_bielsko_area(addr: &AddressEntry) -> bool {
    const SERVICE_LOCALITIES: [&str; 12] = [
        "bielsko-biala", "bestwina", "buczkowice", "chybie", "czechowice-dziedzice",
        "jasienica", "jaworze", "kozy", "porabka", "szczyrk", "wilamowice", "wilkowice",
    ];
    const SERVICE_CITY_IDS: [u64; 13] = [
        923_584, 212_110, 48_395, 50_765, 939_094, 54_674, 55_290,
        924_365, 58_028, 64_773, 925_850, 926_996, 76_612,
    ];
    let city = crate::utils::clean_text(&addr.city_name).replace('–', "-");
    let commune = crate::utils::clean_text(&addr.commune).replace('–', "-");
    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    let is_kety = city.starts_with("kety") || commune.starts_with("kety");

    if let Some(city_id) = addr.city_id {
        if SERVICE_CITY_IDS.contains(&city_id) {
            let expected = if city_id == 924_365 { "malopolskie" } else { "slaskie" };
            return voivodeship.is_empty() || voivodeship.starts_with(expected);
        }
        // TERYT village IDs are not all in this short list. Their commune is
        // authoritative; don't accept a matching place name with a conflicting ID.
        if is_kety {
            return commune.starts_with("kety")
                && (voivodeship.is_empty() || voivodeship.starts_with("malopolskie"));
        }
        return (voivodeship.is_empty() || voivodeship.starts_with("slaskie"))
            && SERVICE_LOCALITIES.iter().any(|name| commune.starts_with(name));
    }

    if is_kety {
        return voivodeship.is_empty() || voivodeship.starts_with("malopolskie");
    }
    (voivodeship.is_empty() || voivodeship.starts_with("slaskie"))
        && SERVICE_LOCALITIES
            .iter()
            .any(|name| city.starts_with(name) || commune.starts_with(name))
}

fn is_bytom(addr: &AddressEntry) -> bool {
    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    if !voivodeship.trim().is_empty() && !voivodeship.trim().starts_with("slaskie") {
        return false;
    }

    if let Some(city_id) = addr.city_id {
        return city_id == 938670;
    }

    let city = crate::utils::clean_text(&addr.city_name);
    city.trim().starts_with("bytom")
}

fn is_gliwice(addr: &AddressEntry) -> bool {
    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    if !voivodeship.trim().is_empty() && !voivodeship.starts_with("slaskie") {
        return false;
    }

    if let Some(city_id) = addr.city_id {
        return city_id == 940_000;
    }

    crate::utils::clean_text(&addr.city_name)
        .trim()
        .starts_with("gliwice")
}

fn is_pwik_gliwice_area(addr: &AddressEntry) -> bool {
    const SERVICE_MUNICIPALITIES: [(&str, u64); 5] = [
        ("gliwice", 940_000),
        ("pyskowice", 942_417),
        ("rudziniec", 221_238),
        ("sosnicowice", 221_936),
        ("zbroslawice", 225_302),
    ];

    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    if !voivodeship.trim().is_empty() && !voivodeship.starts_with("slaskie") {
        return false;
    }

    let normalize_municipality = |value: &str| {
        let mut normalized = crate::utils::clean_text(value.trim());
        for prefix in [
            "gmina miejsko-wiejska ",
            "gmina miejska ",
            "gmina wiejska ",
            "gmina ",
            "gm ",
            "miasto ",
            "m. ",
        ] {
            if let Some(stripped) = normalized.strip_prefix(prefix) {
                normalized = stripped.trim().to_string();
                break;
            }
        }
        normalized
    };

    let city = normalize_municipality(&addr.city_name);
    let commune = normalize_municipality(&addr.commune);
    let city_municipality = SERVICE_MUNICIPALITIES
        .iter()
        .find(|(municipality, _)| city == *municipality);
    let commune_municipality = SERVICE_MUNICIPALITIES
        .iter()
        .find(|(municipality, _)| commune == *municipality);

    if let Some(city_id) = addr.city_id {
        if city_municipality.is_some_and(|(_, expected_id)| city_id != *expected_id) {
            return false;
        }
        if city_municipality.is_some_and(|(_, expected_id)| city_id == *expected_id) {
            return true;
        }
        // TERYT IDs for villages differ from the municipality seat. Their
        // commune name remains authoritative for service-area eligibility.
        return commune_municipality.is_some();
    }

    city_municipality.is_some() || commune_municipality.is_some()
}

fn is_dabrowskie_wodociagi_area(addr: &AddressEntry) -> bool {
    const DABROWA_GORNICZA_SIMC_ID: u64 = 939_473;

    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    if !voivodeship.trim().is_empty() && !voivodeship.starts_with("slaskie") {
        return false;
    }

    let normalize_municipality = |value: &str| {
        let mut normalized = crate::utils::clean_text(value.trim());
        for prefix in ["gmina miejska ", "gmina ", "gm ", "miasto ", "m. "] {
            if let Some(stripped) = normalized.strip_prefix(prefix) {
                normalized = stripped.trim().to_string();
                break;
            }
        }
        normalized
    };

    let city = normalize_municipality(&addr.city_name);
    let commune = normalize_municipality(&addr.commune);
    if addr.city_id == Some(DABROWA_GORNICZA_SIMC_ID) {
        return true;
    }
    if addr.city_id.is_some() {
        // Smaller TERYT localities in the city share the official commune name.
        return commune == "dabrowa gornicza";
    }

    city == "dabrowa gornicza" || commune == "dabrowa gornicza"
}

fn is_city_in_area(
    addr: &AddressEntry,
    city_id: u64,
    city_name: &str,
    voivodeship_name: &str,
) -> bool {
    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    if !voivodeship.is_empty() && !voivodeship.starts_with(voivodeship_name) {
        return false;
    }

    if let Some(address_city_id) = addr.city_id {
        return address_city_id == city_id;
    }

    crate::utils::clean_text(&addr.city_name).trim() == city_name
}

fn is_koszalin(addr: &AddressEntry) -> bool {
    is_city_in_area(addr, 949_448, "koszalin", "zachodniopomorskie")
}

fn is_sosnowiec(addr: &AddressEntry) -> bool {
    is_city_in_area(addr, 943_428, "sosnowiec", "slaskie")
}

fn is_zabrze(addr: &AddressEntry) -> bool {
    is_city_in_area(addr, 945_380, "zabrze", "slaskie")
}

fn is_ruda_slaska(addr: &AddressEntry) -> bool {
    is_city_in_area(addr, 942_630, "ruda slaska", "slaskie")
}

fn normalize_service_commune(value: &str) -> String {
    let mut commune = crate::utils::clean_text(value.trim());
    for prefix in [
        "gmina miejsko-wiejska ",
        "gmina miejska ",
        "gmina wiejska ",
        "gmina ",
        "gm ",
        "miasto ",
        "m ",
    ] {
        if let Some(stripped) = commune.strip_prefix(prefix) {
            commune = stripped.trim().to_string();
            break;
        }
    }
    commune
}

fn is_tarnowskie_wodociagi_area(addr: &AddressEntry) -> bool {
    // Main service area: Tarnów and the full communes of Tarnów and Skrzyszów.
    // The other named places are served only at the individual locality level.
    const SERVICE_LOCALITY_IDS: [u64; 8] = [
        981_570, // Tarnów
        835_443, // Kępa Bogumiłowicka
        827_277, // Szczepanowice (Gmina Pleśna)
        827_389, // Świebodzin (Gmina Pleśna)
        822_736, // Stare Żukowice
        826_220, // Machowa
        829_276, // Zalasowa
        834_580, // Zabłędza
    ];

    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    let city = crate::utils::clean_text(&addr.city_name);
    let commune = normalize_service_commune(&addr.commune);

    if let Some(city_id) = addr.city_id {
        if SERVICE_LOCALITY_IDS.contains(&city_id) {
            return true;
        }
        if !voivodeship.is_empty() && !voivodeship.starts_with("malopolskie") {
            return false;
        }
        return commune == "tarnow" || commune == "skrzyszow";
    }

    if city == "machowa" && commune == "pilzno" {
        return voivodeship.is_empty() || voivodeship.starts_with("podkarpackie");
    }
    if !voivodeship.is_empty() && !voivodeship.starts_with("malopolskie") {
        return false;
    }
    if city == "tarnow" || commune == "tarnow" || commune == "skrzyszow" {
        return true;
    }

    matches!(
        (city.as_str(), commune.as_str()),
        ("kepa bogumilowicka", "wierzchoslawice")
            | ("szczepanowice", "plesna")
            | ("swiebodzin", "plesna")
            | ("stare zukowice", "lisia gora")
            | ("zalasowa", "ryglice")
            | ("zabledza", "tuchow")
    )
}

fn is_pwik_rybnik_area(addr: &AddressEntry) -> bool {
    const SERVICE_LOCALITY_IDS: [u64; 3] = [942_765, 214_178, 214_155];

    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    if !voivodeship.is_empty() && !voivodeship.starts_with("slaskie") {
        return false;
    }

    if let Some(city_id) = addr.city_id {
        if SERVICE_LOCALITY_IDS.contains(&city_id) {
            return true;
        }
        let commune = normalize_service_commune(&addr.commune);
        return commune == "gaszowice" || commune == "jejkowice";
    }

    let city = crate::utils::clean_text(&addr.city_name);
    let commune = normalize_service_commune(&addr.commune);
    city == "rybnik"
        || city == "jejkowice"
        || city == "gaszowice"
        || commune == "jejkowice"
        || commune == "gaszowice"
}

fn is_elblag(addr: &AddressEntry) -> bool {
    if let Some(city_id) = addr.city_id {
        return city_id == 932_703;
    }

    crate::utils::clean_text(&addr.city_name).trim() == "elblag"
}

fn is_zwik_zielona_gora_area(addr: &AddressEntry) -> bool {
    const SERVICE_CITY_IDS: [u64; 28] = [
        917313, 917320, 988320, 917336, 917342, 917359, 917365, 988336, 917371, 917388, 917394,
        917425, 917431, 917448, 1_038_186, 917520, 917477, 917483, 1_038_513, 917490, 917508,
        917514, 1_038_192, 917537, 917543, 988313, 917419, 917402,
    ];
    const SERVICE_LOCALITIES: [&str; 28] = [
        "barcikowice",
        "barcikowiczki",
        "chynow",
        "drzonkow",
        "jany",
        "jarogniewice",
        "jeleniow",
        "jedrzychow",
        "kielpin",
        "krepa",
        "krepa mala",
        "lezyca",
        "lugowo",
        "marzecin",
        "nowy kisielin",
        "ochla",
        "piskorze",
        "przydroze",
        "przylep",
        "racula",
        "raculka",
        "stary kisielin",
        "stozne",
        "sucha",
        "wlosan",
        "zatonie",
        "zawada",
        "zielona gora",
    ];
    if let Some(city_id) = addr.city_id {
        return SERVICE_CITY_IDS.contains(&city_id);
    }

    let city = crate::utils::clean_text(&addr.city_name);
    let commune = crate::utils::clean_text(&addr.commune);
    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    if !voivodeship.starts_with("lubuskie") {
        return false;
    }

    SERVICE_LOCALITIES.contains(&city.as_str())
        || commune == "zielona gora"
        || commune == "gmina zielona gora"
}
fn is_wm_radom_area(addr: &AddressEntry) -> bool {
    if addr.city_id == Some(972750) || addr.city_id == Some(642253) {
        return true;
    }

    let normalize = |value: &str| {
        value
            .trim()
            .to_lowercase()
            .replace('ą', "a")
            .replace('ć', "c")
            .replace('ę', "e")
            .replace('ł', "l")
            .replace('ń', "n")
            .replace('ó', "o")
            .replace('ś', "s")
            .replace(['ź', 'ż'], "z")
    };
    let city = normalize(&addr.city_name);
    let commune = normalize(&addr.commune);
    let voivodeship = normalize(&addr.voivodeship);
    if !voivodeship.is_empty() && !voivodeship.starts_with("mazowieckie") {
        return false;
    }

    let service_areas = [
        "radom",
        "wolanow",
        "jastrzebia",
        "zakrzew",
        "jedlnia-letnisko",
        "jedlnia letnisko",
        "skaryszew",
        "jedlinsk",
        "kowala",
    ];
    service_areas
        .iter()
        .any(|area| city.starts_with(area) || commune.starts_with(area))
        || (city.starts_with("cerekiew")
            && (commune.starts_with("zakrzew") || voivodeship.starts_with("mazowieckie")))
}

city_checker!(is_rzeszow, Some(974133_u64), "rzeszów", "rzeszow");
city_checker!(is_torun, Some(929448_u64), "toruń", "torun");
fn is_wik_opole_area(addr: &AddressEntry) -> bool {
    if addr.city_id == Some(965016) {
        return true;
    }
    let city = crate::utils::clean_text(&addr.city_name);
    let commune = crate::utils::clean_text(&addr.commune);
    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    (voivodeship.is_empty() || voivodeship.starts_with("opolskie"))
        && (city == "opole"
            || ["komprachcice", "proszkow", "turawa"]
                .iter()
                .any(|area| city == *area || commune == *area))
}

fn is_pwik_olsztyn_area(addr: &AddressEntry) -> bool {
    const SERVICE_CITY_IDS: [u64; 9] = [
        964465, 470036, 477080, 486698, 473135, 473098, 473129, 473589, 488830,
    ];
    if let Some(city_id) = addr.city_id {
        return SERVICE_CITY_IDS.contains(&city_id);
    }

    let normalize = |value: &str| crate::utils::clean_text(value).replace(['-', ' '], "");
    let city = normalize(&addr.city_name);
    let commune = normalize(&addr.commune);
    let voivodeship = normalize(&addr.voivodeship);
    if !voivodeship.is_empty() && !voivodeship.starts_with("warminsko") {
        return false;
    }

    let localities = [
        "olsztyn",
        "nikielkowo",
        "jonkowo",
        "ostrzeszewo",
        "zalbki",
        "myki",
        "wadag",
        "gietrzwald",
        "bartag",
    ];
    localities.iter().any(|locality| {
        city == *locality
            || commune == *locality
            || (city.is_empty() && commune.starts_with(locality))
    })
}

fn is_mpwik_rzeszow_area(addr: &AddressEntry) -> bool {
    if let Some(city_id) = addr.city_id {
        return matches!(city_id, 974133 | 644750 | 652688 | 1043193 | 975078);
    }

    let normalize = |value: &str| {
        value
            .trim()
            .to_lowercase()
            .replace('ą', "a")
            .replace('ć', "c")
            .replace('ę', "e")
            .replace('ł', "l")
            .replace('ń', "n")
            .replace('ó', "o")
            .replace('ś', "s")
            .replace(['ź', 'ż'], "z")
    };
    let city = normalize(&addr.city_name);
    let voivodeship = normalize(&addr.voivodeship);
    (voivodeship.is_empty() || voivodeship.starts_with("podkarpackie"))
        && ["rzeszow", "boguchwala", "krasne", "tyczyn"]
            .iter()
            .any(|name| city.starts_with(name))
}
city_checker!(is_gdansk, Some(908123_u64), "gdańsk", "gdansk");
city_checker!(is_bydgoszcz, Some(928363_u64), "bydgoszcz");

fn is_opec_gdynia_area(addr: &AddressEntry) -> bool {
    let city = crate::utils::clean_text(&addr.city_name);
    let commune = crate::utils::clean_text(&addr.commune);
    let voivodeship = crate::utils::clean_text(&addr.voivodeship);
    if !voivodeship.is_empty() && !voivodeship.starts_with("pomorskie") {
        return false;
    }

    ["gdyn", "rumi", "red", "wejherow"]
        .iter()
        .any(|name| city.starts_with(name))
        || city.starts_with("kosakow")
        || commune.starts_with("kosakow")
}

fn is_pewik_gdynia_area(addr: &AddressEntry) -> bool {
    let normalize = |value: &str| crate::utils::clean_text(value).replace('-', " ");
    let city = normalize(&addr.city_name);
    let commune = normalize(&addr.commune);
    let district = normalize(&addr.district);
    let voivodeship = normalize(&addr.voivodeship);
    if !voivodeship.is_empty() && !voivodeship.starts_with("pomorskie") {
        return false;
    }
    if district.contains("port") || district.contains("wojsk") {
        return false;
    }

    if city.starts_with("gdyn") {
        let streets = crate::utils::configured_street_name_candidates(
            &addr.street_name,
            &addr.street_name_1,
            addr.street_name_2.as_deref(),
        );
        return !streets.iter().any(|street| {
            let street = normalize(street);
            ["losiowa", "sarnia", "jelenia"]
                .iter()
                .any(|excluded| street == *excluded)
        });
    }

    if ["rumi", "reda", "wejherow"]
        .iter()
        .any(|prefix| city.starts_with(prefix))
    {
        return true;
    }

    let in_kosakowo = commune.starts_with("kosakowo") || commune.starts_with("gmina kosakowo");
    let in_wejherowo_commune =
        commune.starts_with("wejherowo") || commune.starts_with("gmina wejherowo");
    let kosakowo_villages = [
        "debogorze wybudowanie",
        "kazimierz",
        "kosakowo",
        "pogorze",
    ];
    let wejherowo_villages = [
        "bolszewo",
        "goscicino",
        "gora",
        "kniewo",
        "orle",
        "warszkowo",
        "zamostne",
    ];

    (in_kosakowo && kosakowo_villages.iter().any(|village| city.starts_with(village)))
        || (in_wejherowo_commune
            && wejherowo_villages
                .iter()
                .any(|village| city.starts_with(village)))
}

fn is_torunskie_wodociagi_area(addr: &AddressEntry) -> bool {
    let city = crate::utils::clean_text(&addr.city_name).replace('-', " ");
    let commune = crate::utils::clean_text(&addr.commune).replace('-', " ");
    let voivodeship = crate::utils::clean_text(&addr.voivodeship).replace('-', " ");

    if !voivodeship.is_empty() && !voivodeship.starts_with("kujawsko pomorskie") {
        return false;
    }

    if addr.city_id == Some(929448) || city.starts_with("torun") {
        return true;
    }

    ["lubicz", "lysomice", "obrowo", "wielka nieszawka", "zlawies wielka"]
        .iter()
        .any(|area| city.starts_with(area) || commune.starts_with(area))
}

fn is_wodociagi_kieleckie_area(addr: &AddressEntry) -> bool {
    let normalize = |value: &str| {
        value
            .trim()
            .to_lowercase()
            .replace('ą', "a")
            .replace('ć', "c")
            .replace('ę', "e")
            .replace('ł', "l")
            .replace('ń', "n")
            .replace('ó', "o")
            .replace('ś', "s")
            .replace(['ź', 'ż'], "z")
            .replace('-', " ")
    };
    let city = normalize(&addr.city_name);
    let commune = normalize(&addr.commune);
    let voivodeship = normalize(&addr.voivodeship);
    if !voivodeship.is_empty() && !voivodeship.starts_with("swietokrzyskie") {
        return false;
    }

    addr.city_id == Some(945930)
        || city.starts_with("kielce")
        || city.starts_with("cedzyna")
        || city.starts_with("maslow")
        || city.starts_with("zagnansk")
        || ["maslow", "nowiny", "sitkowka nowiny", "zagnansk", "gorno"]
            .iter()
            .any(|name| commune.starts_with(name))
}

#[allow(dead_code)]
fn is_bialystok(addr: &AddressEntry) -> bool {
    if let Some(city_id) = addr.city_id {
        return city_id == 922410;
    }

    let city = addr.city_name.trim().to_lowercase();
    let is_bialystok = city.starts_with("białystok") || city.starts_with("bialystok");
    let voivodeship = addr.voivodeship.trim().to_lowercase();
    is_bialystok && (voivodeship.is_empty() || voivodeship.starts_with("podlaskie"))
}

fn is_wodociagi_bialostockie_area(addr: &AddressEntry) -> bool {
    let normalize = |value: &str| {
        value
            .trim()
            .to_lowercase()
            .replace('ą', "a")
            .replace('ć', "c")
            .replace('ę', "e")
            .replace('ł', "l")
            .replace('ń', "n")
            .replace('ó', "o")
            .replace('ś', "s")
            .replace(['ź', 'ż'], "z")
            .replace('-', " ")
    };
    let voivodeship = normalize(&addr.voivodeship);
    if !voivodeship.is_empty() && !voivodeship.starts_with("podlaskie") {
        return false;
    }

    let city = normalize(&addr.city_name);
    let commune = normalize(&addr.commune);
    addr.city_id == Some(922410)
        || addr.city_id == Some(923526)
        || city.starts_with("bialystok")
        || city.starts_with("wasilkow")
        || commune.starts_with("wasilkow")
        || [
            "jurowce",
            "sochonie",
            "woroszyl",
            "wolka poduchowna",
            "wolka przedmiescie",
            "dabrowki",
            "studzianki",
        ]
        .iter()
        .any(|locality| city.starts_with(locality))
}

city_checker!(is_katowice, None::<u64>, "katowice");
city_checker!(is_plock, None::<u64>, "płock", "plock");
city_checker!(is_czestochowa, None::<u64>, "częstochowa", "czestochowa");
city_checker!(is_poznan_area, None::<u64>, "poznań", "poznan", "czerwonak", "dopiewo", "kleszczewo", "komorniki", "kórnik", "kornik", "luboń", "lubon", "mosina", "murowana goślina", "murowana goslina", "puszczykowo", "suchy las", "swarzędz", "swarzedz", "tarnowo podgórne", "tarnowo podgorne", "brodnica");
city_checker!(is_rokietnica, None::<u64>, "rokietnica", "bytkowo", "cerekwica", "kiekrz", "krzyszkowo", "mrowino", "napachanie", "przybroda", "rostworowo", "rogierowko", "sobota", "starzyny", "zydowo", "dalekie");







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
    pub font_size: Option<String>,
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
    #[serde(default)]
    pub filter_by_house_no: bool,
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
            font_size: None,
            enabled_sources: Some(Vec::new()),
            notification_preferences: HashMap::new(),
            upcoming_notification_enabled: false,
            upcoming_notification_hours: 24,
            show_other_outages: true,
            filter_by_house_no: false,
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

    fn aqua_test_address(city: &str, commune: &str, voivodeship: &str, city_id: Option<u64>) -> AddressEntry {
        AddressEntry {
            name: "Home".into(), city_name: city.into(), voivodeship: voivodeship.into(),
            district: String::new(), commune: commune.into(), street_name: "Testowa".into(),
            street_name_1: "Testowa".into(), street_name_2: None, house_no: "1".into(),
            city_id, street_id: None, is_active: true,
        }
    }

    #[test]
    fn aqua_bielsko_source_id_and_service_area_are_explicit() {
        assert_eq!(AlertSource::AquaBielsko.to_string(), "aqua_bielsko");
        assert_eq!(AlertSource::AquaBielsko.display_name(), "AQUA S.A.");
        assert_eq!(serde_json::to_string(&AlertSource::AquaBielsko).unwrap(), "\"aqua_bielsko\"");
        for (city, commune, woj, id) in [
            ("Bielsko-Biała", "Bielsko-Biała", "Śląskie", Some(923_584)),
            ("Bestwina", "Bestwina", "Śląskie", Some(212_110)),
            ("Jasienica", "Jasienica", "Śląskie", Some(54_674)),
            ("Kęty", "Kęty", "Małopolskie", Some(924_365)),
            ("Kęty", "Kęty", "Małopolskie", None),
        ] {
            assert!(is_address_applicable_for_provider(&AlertSource::AquaBielsko, &aqua_test_address(city, commune, woj, id)), "{city}");
        }
        assert!(is_address_applicable_for_provider(&AlertSource::AquaBielsko, &aqua_test_address("Mazańcowice", "Jasienica", "Śląskie", Some(54_898))));
        assert!(!is_address_applicable_for_provider(&AlertSource::AquaBielsko, &aqua_test_address("Skoczów", "Skoczów", "Śląskie", None)));
        assert!(!is_address_applicable_for_provider(&AlertSource::AquaBielsko, &aqua_test_address("Andrychów", "Andrychów", "Małopolskie", None)));
        assert!(!is_address_applicable_for_provider(&AlertSource::AquaBielsko, &aqua_test_address("Kęty", "Kęty", "Śląskie", None)));
    }

    #[test]
    fn pwik_gorzow_source_id_and_service_area_are_explicit() {
        assert_eq!(AlertSource::PwikGorzow.to_string(), "pwik_gorzow");
        assert_eq!(AlertSource::PwikGorzow.display_name(), "PWiK Gorzów Wielkopolski");
        assert_eq!(serde_json::to_string(&AlertSource::PwikGorzow).unwrap(), "\"pwik_gorzow\"");

        for (city, commune, id) in [
            ("Gorzów Wielkopolski", "Gorzów Wielkopolski", Some(935_140)),
            ("Wojcieszyce", "Gmina Kłodawa", Some(93_000)),
            ("Bogdaniec", "Bogdaniec", Some(178_672)),
            ("Deszczno", "Deszczno", Some(179_520)),
            ("Lubiszyn", "Lubiszyn", Some(182_662)),
            ("Santok", "Santok", Some(186_708)),
        ] {
            assert!(is_address_applicable_for_provider(
                &AlertSource::PwikGorzow,
                &aqua_test_address(city, commune, "Lubuskie", id),
            ), "{city}, {commune}");
        }
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PwikGorzow,
            &aqua_test_address("Kłodawa", "Kłodawa", "Pomorskie", None),
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PwikGorzow,
            &aqua_test_address("Zielona Góra", "Zielona Góra", "Lubuskie", None),
        ));
    }

    #[test]
    fn pec_gliwice_source_id_and_service_area_are_explicit() {
        assert_eq!(AlertSource::PecGliwice.to_string(), "pec_gliwice");
        assert_eq!(AlertSource::PecGliwice.display_name(), "PEC Gliwice");
        assert_eq!(
            serde_json::to_string(&AlertSource::PecGliwice).unwrap(),
            "\"pec_gliwice\""
        );

        assert!(is_address_applicable_for_provider(
            &AlertSource::PecGliwice,
            &aqua_test_address("Gliwice", "Gliwice", "Śląskie", Some(940_000)),
        ));
        assert!(is_address_applicable_for_provider(
            &AlertSource::PecGliwice,
            &aqua_test_address("Gliwice", "", "", None),
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PecGliwice,
            &aqua_test_address("Bytom", "Bytom", "Śląskie", Some(938_670)),
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PecGliwice,
            &aqua_test_address("Gliwice", "Gliwice", "Małopolskie", Some(938_671)),
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PecGliwice,
            &aqua_test_address("Gliwice", "Gliwice", "Śląskie", Some(938_670)),
        ));
    }

    #[test]
    fn pwik_gliwice_source_id_and_service_area_are_explicit() {
        assert_eq!(AlertSource::PwikGliwice.to_string(), "pwik_gliwice");
        assert_eq!(AlertSource::PwikGliwice.display_name(), "PWiK Gliwice");
        assert_eq!(
            serde_json::to_string(&AlertSource::PwikGliwice).unwrap(),
            "\"pwik_gliwice\""
        );

        for (city, commune, id) in [
            ("Gliwice", "Gliwice", Some(940_000)),
            ("Pyskowice", "Gmina Pyskowice", Some(942_417)),
            ("Rudziniec", "Rudziniec", Some(221_238)),
            ("Sośnicowice", "Sośnicowice", Some(221_936)),
            ("Niewiesze", "Gmina Rudziniec", None),
            ("Zbrosławice", "Gmina Zbrosławice", Some(225_302)),
        ] {
            assert!(
                is_address_applicable_for_provider(
                    &AlertSource::PwikGliwice,
                    &aqua_test_address(city, commune, "Śląskie", id),
                ),
                "{city}, {commune}"
            );
        }
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PwikGliwice,
            &aqua_test_address("Bytom", "Bytom", "Śląskie", Some(938_670)),
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PwikGliwice,
            &aqua_test_address("Niewiesze", "Gmina Rudziniec", "Opolskie", None),
        ));
    }
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
            location: None,
            address_index: None,
            is_local: None,
            hash: None,
        };

        let alert2 = UnifiedAlert {
            source: AlertSource::Tauron,
            startDate: Some("2024-01-01 10:00".to_string()),
            endDate: Some("2024-01-01 14:00".to_string()),
            message: Some("Brak prądu".to_string()),
            location: Some("Different desc".to_string()),
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
                location: None,
                address_index: None,
                is_local: None,
                hash: None,
            },
            UnifiedAlert {
                source: AlertSource::Energa,
                startDate: Some("2024-05-20 10:00".to_string()),
                endDate: None,
                message: None,
                location: None,
                address_index: None,
                is_local: None,
                hash: None,
            },
            UnifiedAlert {
                source: AlertSource::MpwikWroclaw,
                startDate: None,
                endDate: None,
                message: None,
                location: None,
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
    fn date_only_start_does_not_trigger_a_false_upcoming_notification() {
        let mut mock_db = MockDatabaseInterface::new();
        let mut mock_notifier = MockNotificationProvider::new();
        let tomorrow = (chrono::Utc::now() + chrono::Duration::days(1))
            .with_timezone(&chrono_tz::Europe::Warsaw)
            .format("%Y-%m-%d")
            .to_string();
        let settings = Settings {
            notification_preferences: [("pewik_gdynia".to_string(), true)].into(),
            enabled_sources: Some(vec!["pewik_gdynia".to_string()]),
            upcoming_notification_enabled: true,
            upcoming_notification_hours: 48,
            ..Default::default()
        };
        let alerts = vec![UnifiedAlert {
            source: AlertSource::PewikGdynia,
            startDate: Some(tomorrow),
            message: Some("Awaria wodociągowa".to_string()),
            is_local: Some(true),
            ..Default::default()
        }];
        let hash = alerts[0].to_hash();
        mock_db
            .expect_is_alert_seen()
            .with(predicate::eq("pewik_gdynia"), predicate::eq(hash.clone()))
            .times(1)
            .returning(|_, _| Ok(false));
        mock_db
            .expect_mark_alert_as_seen()
            .with(predicate::eq("pewik_gdynia"), predicate::eq(hash))
            .times(1)
            .returning(|_, _| Ok(()));
        mock_notifier
            .expect_show_notification()
            .withf(|title, _, _| title.starts_with("Nowe") && !title.contains("Nadchodzące"))
            .times(1)
            .returning(|_, _, _| ());

        MonitorEngine::new(&mock_db, &mock_notifier, &settings).process_alerts(alerts);
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
    fn test_kpec_bydgoszcz_scope() {
        let bydgoszcz_address = AddressEntry {
            city_name: String::new(),
            city_id: Some(928363),
            is_active: true,
            ..Default::default()
        };
        let torun_address = AddressEntry {
            city_name: "Toruń".to_string(),
            is_active: true,
            ..Default::default()
        };
        let bydgoszcz_settings = Settings {
            addresses: vec![bydgoszcz_address],
            ..Default::default()
        };
        let torun_settings = Settings {
            addresses: vec![torun_address],
            ..Default::default()
        };

        assert!(is_provider_applicable(
            AlertSource::KpecBydgoszcz,
            &bydgoszcz_settings
        ));
        assert!(!is_provider_applicable(
            AlertSource::KpecBydgoszcz,
            &torun_settings
        ));
        assert_eq!(AlertSource::KpecBydgoszcz.to_string(), "kpec_bydgoszcz");
    }

    #[test]
    fn test_mwik_bydgoszcz_scope() {
        let bydgoszcz_address = AddressEntry {
            city_name: "Bydgoszcz".to_string(),
            city_id: Some(928363),
            is_active: true,
            ..Default::default()
        };
        let torun_address = AddressEntry {
            city_name: "Toruń".to_string(),
            city_id: Some(929448),
            is_active: true,
            ..Default::default()
        };

        assert!(is_address_applicable_for_provider(
            &AlertSource::MwikBydgoszcz,
            &bydgoszcz_address
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::MwikBydgoszcz,
            &torun_address
        ));
        assert_eq!(AlertSource::MwikBydgoszcz.to_string(), "mwik_bydgoszcz");
        assert_eq!(AlertSource::MwikBydgoszcz.display_name(), "MWiK Bydgoszcz");
    }

    #[test]
    fn test_wodociagi_kieleckie_scope() {
        let kielce = AddressEntry {
            city_name: "Kielce".to_string(),
            voivodeship: "Świętokrzyskie".to_string(),
            city_id: Some(945930),
            is_active: true,
            ..Default::default()
        };
        let maslow = AddressEntry {
            city_name: "Masłów Pierwszy".to_string(),
            commune: "Masłów".to_string(),
            voivodeship: "Świętokrzyskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let nowiny = AddressEntry {
            city_name: "Nowiny".to_string(),
            commune: "Nowiny".to_string(),
            voivodeship: "Świętokrzyskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let zagnansk = AddressEntry {
            city_name: "Zagnańsk".to_string(),
            commune: "Zagnańsk".to_string(),
            voivodeship: "Świętokrzyskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let cedzyna = AddressEntry {
            city_name: "Cedzyna".to_string(),
            commune: "Górno".to_string(),
            voivodeship: "Świętokrzyskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let lublin = AddressEntry {
            city_name: "Lublin".to_string(),
            commune: "Lublin".to_string(),
            voivodeship: "Lubelskie".to_string(),
            is_active: true,
            ..Default::default()
        };

        for address in [&kielce, &maslow, &nowiny, &zagnansk, &cedzyna] {
            assert!(is_address_applicable_for_provider(
                &AlertSource::WodociagiKieleckie,
                address
            ));
        }
        assert!(!is_address_applicable_for_provider(
            &AlertSource::WodociagiKieleckie,
            &lublin
        ));
        assert_eq!(
            AlertSource::WodociagiKieleckie.to_string(),
            "wodociagi_kieleckie"
        );
        assert_eq!(
            AlertSource::WodociagiKieleckie.display_name(),
            "Wodociągi Kieleckie"
        );
    }

    #[test]
    fn test_opec_gdynia_scope() {
        let gdynia = AddressEntry {
            city_name: "Gdynia".to_string(),
            voivodeship: "Pomorskie".to_string(),
            ..Default::default()
        };
        let rumia = AddressEntry {
            city_name: "Rumia".to_string(),
            voivodeship: "Pomorskie".to_string(),
            ..Default::default()
        };
        let reda = AddressEntry {
            city_name: "Reda".to_string(),
            voivodeship: "Pomorskie".to_string(),
            ..Default::default()
        };
        let kosakowo_commune = AddressEntry {
            city_name: "Pogórze".to_string(),
            commune: "Kosakowo".to_string(),
            voivodeship: "Pomorskie".to_string(),
            ..Default::default()
        };
        let wejherowo = AddressEntry {
            city_name: "Wejherowo".to_string(),
            voivodeship: "Pomorskie".to_string(),
            ..Default::default()
        };
        let sopot = AddressEntry {
            city_name: "Sopot".to_string(),
            voivodeship: "Pomorskie".to_string(),
            ..Default::default()
        };
        let wrong_voivodeship = AddressEntry {
            city_name: "Gdynia".to_string(),
            voivodeship: "Mazowieckie".to_string(),
            ..Default::default()
        };

        for address in [&gdynia, &rumia, &reda, &kosakowo_commune, &wejherowo] {
            assert!(is_address_applicable_for_provider(
                &AlertSource::OpecGdynia,
                address
            ));
        }
        for address in [&sopot, &wrong_voivodeship] {
            assert!(!is_address_applicable_for_provider(
                &AlertSource::OpecGdynia,
                address
            ));
        }
        assert_eq!(AlertSource::OpecGdynia.to_string(), "opec_gdynia");
        assert_eq!(AlertSource::OpecGdynia.display_name(), "OPEC Gdynia");
    }

    #[test]
    fn test_torunskie_wodociagi_scope() {
        let torun = AddressEntry {
            city_name: "Toruń".to_string(),
            commune: "Toruń".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            city_id: Some(929448),
            is_active: true,
            ..Default::default()
        };
        let lubicz = AddressEntry {
            city_name: "Lubicz Dolny".to_string(),
            commune: "Lubicz".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let lysomice = AddressEntry {
            city_name: "Łysomice".to_string(),
            commune: "Łysomice".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let obrowo = AddressEntry {
            city_name: "Obrowo".to_string(),
            commune: "Obrowo".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let wielka_nieszawka = AddressEntry {
            city_name: "Wielka Nieszawka".to_string(),
            commune: "Wielka Nieszawka".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let zlawies_wielka = AddressEntry {
            city_name: "Przysiek".to_string(),
            commune: "Zławieś Wielka".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let ciechocinek = AddressEntry {
            city_name: "Ciechocinek".to_string(),
            commune: "Ciechocinek".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            is_active: true,
            ..Default::default()
        };
        let lublin = AddressEntry {
            city_name: "Lublin".to_string(),
            commune: "Lublin".to_string(),
            voivodeship: "Lubelskie".to_string(),
            is_active: true,
            ..Default::default()
        };

        for address in [
            &torun,
            &lubicz,
            &lysomice,
            &obrowo,
            &wielka_nieszawka,
            &zlawies_wielka,
        ] {
            assert!(is_address_applicable_for_provider(
                &AlertSource::TorunskieWodociagi,
                address
            ));
        }
        assert!(!is_address_applicable_for_provider(
            &AlertSource::TorunskieWodociagi,
            &ciechocinek
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::TorunskieWodociagi,
            &lublin
        ));
        assert_eq!(
            AlertSource::TorunskieWodociagi.to_string(),
            "torunskie_wodociagi"
        );
        assert_eq!(
            AlertSource::TorunskieWodociagi.display_name(),
            "Toruńskie Wodociągi"
        );
    }

    #[test]
    fn test_wodociagi_bialostockie_scope() {
        let bialystok = AddressEntry {
            city_name: "Białystok".to_string(),
            voivodeship: "Podlaskie".to_string(),
            city_id: Some(922410),
            ..Default::default()
        };
        let wasilkow = AddressEntry {
            city_name: "Wasilków".to_string(),
            commune: "Wasilków".to_string(),
            voivodeship: "Podlaskie".to_string(),
            city_id: Some(923526),
            ..Default::default()
        };
        let village = AddressEntry {
            city_name: "Jurowce".to_string(),
            commune: "Wasilków".to_string(),
            voivodeship: "podlaskie".to_string(),
            ..Default::default()
        };
        let other_area = AddressEntry {
            city_name: "Łomża".to_string(),
            voivodeship: "Podlaskie".to_string(),
            ..Default::default()
        };
        let wrong_voivodeship = AddressEntry {
            city_name: "Wasilków".to_string(),
            voivodeship: "Mazowieckie".to_string(),
            ..Default::default()
        };

        for address in [&bialystok, &wasilkow, &village] {
            assert!(is_address_applicable_for_provider(
                &AlertSource::WodociagiBialostockie,
                address
            ));
        }
        assert!(!is_address_applicable_for_provider(
            &AlertSource::WodociagiBialostockie,
            &other_area
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::WodociagiBialostockie,
            &wrong_voivodeship
        ));
        assert_eq!(
            AlertSource::WodociagiBialostockie.to_string(),
            "wodociagi_bialostockie"
        );
        assert_eq!(
            AlertSource::WodociagiBialostockie.display_name(),
            "Wodociągi Białostockie"
        );
    }

    #[test]
    fn test_enea_cieplo_bialystok_scope() {
        let bialystok_address = AddressEntry {
            city_name: "Białystok".to_string(),
            voivodeship: "Podlaskie".to_string(),
            city_id: Some(922410),
            is_active: true,
            ..Default::default()
        };
        let other_bialystok_address = AddressEntry {
            city_name: "Białystok".to_string(),
            voivodeship: "Lubelskie".to_string(),
            city_id: Some(886759),
            is_active: true,
            ..Default::default()
        };

        assert!(is_address_applicable_for_provider(
            &AlertSource::EneaCieplo,
            &bialystok_address
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::EneaCieplo,
            &other_bialystok_address
        ));
        assert_eq!(AlertSource::EneaCieplo.to_string(), "enea_cieplo");
        assert_eq!(AlertSource::EneaCieplo.display_name(), "ENEA Ciepło Białystok");
    }

    #[test]
    fn test_energa_cieplo_kaliskie_scope() {
        let kalisz_address = AddressEntry {
            city_name: "Kalisz".to_string(),
            voivodeship: "Wielkopolskie".to_string(),
            city_id: Some(936579),
            is_active: true,
            ..Default::default()
        };
        let torun_address = AddressEntry {
            city_name: "Toruń".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            city_id: Some(929448),
            is_active: true,
            ..Default::default()
        };

        assert!(is_address_applicable_for_provider(
            &AlertSource::EnergaCieploKaliskie,
            &kalisz_address
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::EnergaCieploKaliskie,
            &torun_address
        ));
        assert_eq!(
            AlertSource::EnergaCieploKaliskie.to_string(),
            "energa_cieplo_kaliskie"
        );
        assert_eq!(
            AlertSource::EnergaCieploKaliskie.display_name(),
            "Energa Ciepło Kaliskie"
        );
    }

    #[test]
    fn test_pge_torun_heat_scope() {
        let torun_address = AddressEntry {
            city_name: "Toruń".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            city_id: Some(929448),
            is_active: true,
            ..Default::default()
        };
        let bydgoszcz_address = AddressEntry {
            city_name: "Bydgoszcz".to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            city_id: Some(928363),
            is_active: true,
            ..Default::default()
        };

        assert!(is_address_applicable_for_provider(
            &AlertSource::PgeTorunHeat,
            &torun_address
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PgeTorunHeat,
            &bydgoszcz_address
        ));
        assert_eq!(AlertSource::PgeTorunHeat.to_string(), "pge_torun_heat");
        assert_eq!(AlertSource::PgeTorunHeat.display_name(), "PGE Toruń");
    }

    #[test]
    fn test_mpec_kielce_scope() {
        let kielce_by_id = AddressEntry {
            city_name: "Kielce".to_string(),
            voivodeship: "Świętokrzyskie".to_string(),
            city_id: Some(945930),
            is_active: true,
            ..Default::default()
        };
        let kielce_by_name = AddressEntry {
            city_name: "Kielce".to_string(),
            voivodeship: "".to_string(),
            city_id: None,
            is_active: true,
            ..Default::default()
        };
        let lublin = AddressEntry {
            city_name: "Lublin".to_string(),
            voivodeship: "Lubelskie".to_string(),
            city_id: Some(959423),
            is_active: true,
            ..Default::default()
        };

        assert!(is_address_applicable_for_provider(
            &AlertSource::MpecKielce,
            &kielce_by_id
        ));
        assert!(is_address_applicable_for_provider(
            &AlertSource::MpecKielce,
            &kielce_by_name
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::MpecKielce,
            &lublin
        ));
        assert_eq!(AlertSource::MpecKielce.to_string(), "mpec_kielce");
        assert_eq!(AlertSource::MpecKielce.display_name(), "MPEC Kielce");
    }

    #[test]
    fn test_monitor_engine_skip_expired() {
        let mock_db = MockDatabaseInterface::new();
        let mock_notifier = MockNotificationProvider::new();

        let settings = Settings {
            notification_preferences: [("pge_torun_heat".to_string(), true)].into(),
            enabled_sources: Some(vec!["pge_torun_heat".to_string()]),
            ..Default::default()
        };

        // Outage that ended 1 hour ago
        let past_end = (chrono::Utc::now() - chrono::Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();

        let alerts = vec![
            UnifiedAlert {
                source: AlertSource::PgeTorunHeat,
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

    #[test]
    fn test_unified_notification_formatting() {
        let alert = UnifiedAlert {
            source: AlertSource::Tauron,
            startDate: Some("2024-05-20 10:00".to_string()),
            endDate: Some("2024-05-20 15:00".to_string()),
            message: Some("Brak prądu".to_string()),
            address_index: Some(0),
            is_local: Some(true),
            ..Default::default()
        };

        let mut settings = Settings {
            filter_by_house_no: true,
            ..Default::default()
        };
        settings.addresses.push(AddressEntry {
            name: "Dom".to_string(),
            city_name: "Szczecin".to_string(),
            street_name: "Stoisława".to_string(),
            house_no: "5".to_string(),
            is_active: true,
            ..Default::default()
        });

        let title = format_notification_title(&alert, &settings, false);
        assert_eq!(title, "Dom: Nowe wyłączenie prądu - Tauron");

        let upcoming_title = format_notification_title(&alert, &settings, true);
        assert_eq!(upcoming_title, "Dom: Nadchodzące wyłączenie prądu - Tauron");

        let body = format_notification_body(&alert, &settings);
        assert_eq!(body, "Kiedy: 20-05-2024 10:00 - 20-05-2024 15:00\nGdzie: Stoisława 5, Szczecin");
    }

    #[test]
    fn pewik_gdynia_service_area_uses_water_localities_and_excludes_other_territories() {
        let address = |city: &str, commune: &str, street: &str| AddressEntry {
            city_name: city.to_string(),
            commune: commune.to_string(),
            street_name: street.to_string(),
            street_name_1: street.to_string(),
            voivodeship: "Pomorskie".to_string(),
            is_active: true,
            ..Default::default()
        };

        assert!(is_address_applicable_for_provider(
            &AlertSource::PewikGdynia,
            &address("Gdynia", "Gdynia", "Owsiana")
        ));
        assert!(is_address_applicable_for_provider(
            &AlertSource::PewikGdynia,
            &address("Bolszewo", "Gmina Wejherowo", "Długa")
        ));
        assert!(is_address_applicable_for_provider(
            &AlertSource::PewikGdynia,
            &address("Pogórze", "Gmina Kosakowo", "Szkolna")
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PewikGdynia,
            &address("Gdynia", "Gdynia", "Sarnia")
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PewikGdynia,
            &AddressEntry {
                district: "Port Gdynia".to_string(),
                ..address("Gdynia", "Gdynia", "Morska")
            }
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PewikGdynia,
            &address("Sopot", "Sopot", "Morska")
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PewikGdynia,
            &address("Łężyce", "Gmina Wejherowo", "Leśna")
        ));
        assert_eq!(AlertSource::PewikGdynia.to_string(), "pewik_gdynia");
        assert_eq!(AlertSource::PewikGdynia.display_name(), "PEWIK Gdynia");
    }

    #[test]
    fn date_only_alert_start_is_not_formatted_as_2359() {
        let alert = UnifiedAlert {
            source: AlertSource::PewikGdynia,
            startDate: Some("2026-10-21".to_string()),
            endDate: Some("2026-10-21T19:00:00".to_string()),
            address_index: Some(0),
            ..Default::default()
        };
        let settings = Settings {
            language: Some("pl".to_string()),
            addresses: vec![AddressEntry {
                name: "Dom".to_string(),
                city_name: "Gdynia".to_string(),
                street_name: "Owsiana".to_string(),
                house_no: "12".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let body = format_notification_body(&alert, &settings);
        assert!(body.starts_with("Kiedy: 21-10-2026 - 21-10-2026 19:00"));
    }
}

#[cfg(test)]
mod zwik_zielona_gora_scope_tests {
    use super::{is_address_applicable_for_provider, AddressEntry, AlertSource};

    #[test]
    fn zwik_zielona_gora_scope_uses_current_municipal_simc_ids() {
        let ochla = AddressEntry {
            city_name: "Ochla".to_string(),
            commune: "Zielona Góra".to_string(),
            city_id: Some(917448),
            ..Default::default()
        };
        assert!(is_address_applicable_for_provider(
            &AlertSource::ZwikZielonaGora,
            &ochla
        ));
    }

    #[test]
    fn zwik_zielona_gora_text_fallback_requires_lubuskie() {
        let local = AddressEntry {
            city_name: "Racula".to_string(),
            commune: "Zielona Góra".to_string(),
            voivodeship: "Lubuskie".to_string(),
            ..Default::default()
        };
        let local_name_without_id = AddressEntry {
            city_name: "Ochla".to_string(),
            voivodeship: "Lubuskie".to_string(),
            ..Default::default()
        };
        let same_name_elsewhere = AddressEntry {
            city_name: "Zielona Góra".to_string(),
            voivodeship: "Dolnośląskie".to_string(),
            ..Default::default()
        };
        let unknown_teryt_city_id = AddressEntry {
            city_name: "Zielona Góra".to_string(),
            voivodeship: "Lubuskie".to_string(),
            city_id: Some(999001),
            ..Default::default()
        };
        let ambiguous_without_region = AddressEntry {
            city_name: "Zielona Góra".to_string(),
            ..Default::default()
        };

        assert!(is_address_applicable_for_provider(
            &AlertSource::ZwikZielonaGora,
            &local
        ));
        assert!(is_address_applicable_for_provider(
            &AlertSource::ZwikZielonaGora,
            &local_name_without_id
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::ZwikZielonaGora,
            &same_name_elsewhere
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::ZwikZielonaGora,
            &unknown_teryt_city_id
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::ZwikZielonaGora,
            &ambiguous_without_region
        ));
    }

    #[test]
    fn zwik_zielona_gora_has_stable_source_id_and_display_name() {
        assert_eq!(AlertSource::ZwikZielonaGora.to_string(), "zwik_zielona_gora");
        assert_eq!(
            AlertSource::ZwikZielonaGora.display_name(),
            "ZWiK Zielona Góra"
        );
        assert_eq!(
            serde_json::to_string(&AlertSource::ZwikZielonaGora).unwrap(),
            "\"zwik_zielona_gora\""
        );
    }
}

#[cfg(test)]
mod lpwik_legnica_registration_tests {
    use super::{
        format_notification_title, is_address_applicable_for_provider, AddressEntry, AlertSource,
        Settings, UnifiedAlert,
    };

    #[test]
    fn lpwik_legnica_uses_the_legnica_simc_and_city_name_fallback() {
        let by_simc = AddressEntry {
            city_name: "Legnica".to_string(),
            city_id: Some(954047),
            ..Default::default()
        };
        let by_name = AddressEntry {
            city_name: "Legnica".to_string(),
            ..Default::default()
        };
        let outside_service_area = AddressEntry {
            city_name: "Lubin".to_string(),
            city_id: Some(954071),
            ..Default::default()
        };
        let conflicting_identity = AddressEntry {
            city_name: "Legnica".to_string(),
            voivodeship: "Dolnośląskie".to_string(),
            city_id: Some(999001),
            ..Default::default()
        };
        assert!(is_address_applicable_for_provider(
            &AlertSource::LpwikLegnica,
            &by_simc
        ));
        assert!(is_address_applicable_for_provider(
            &AlertSource::LpwikLegnica,
            &by_name
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::LpwikLegnica,
            &outside_service_area
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::LpwikLegnica,
            &conflicting_identity
        ));
    }

    #[test]
    fn lpwik_legnica_has_a_stable_source_id_and_water_notification_classification() {
        assert_eq!(AlertSource::LpwikLegnica.to_string(), "lpwik_legnica");
        assert_eq!(
            AlertSource::LpwikLegnica.display_name(),
            "LPWiK Legnica"
        );
        assert_eq!(
            serde_json::to_string(&AlertSource::LpwikLegnica).unwrap(),
            "\"lpwik_legnica\""
        );

        let title = format_notification_title(
            &UnifiedAlert {
                source: AlertSource::LpwikLegnica,
                ..Default::default()
            },
            &Settings::default(),
            false,
        );
        assert_eq!(title, "Nowe wyłączenie wody - LPWiK Legnica");
    }

    #[test]
    fn bytomskie_wodociagi_is_limited_to_bytom() {
        let by_simc = AddressEntry {
            city_name: "Bytom".to_string(),
            voivodeship: "Śląskie".to_string(),
            city_id: Some(938670),
            ..Default::default()
        };
        let by_name = AddressEntry {
            city_name: "Bytom".to_string(),
            voivodeship: "Slaskie".to_string(),
            ..Default::default()
        };
        let unknown_voivodeship = AddressEntry {
            city_name: "BYTOM".to_string(),
            ..Default::default()
        };
        let wrong_city = AddressEntry {
            city_name: "Gliwice".to_string(),
            voivodeship: "Śląskie".to_string(),
            city_id: Some(937001),
            ..Default::default()
        };
        let wrong_voivodeship = AddressEntry {
            city_name: "Bytom".to_string(),
            voivodeship: "Mazowieckie".to_string(),
            ..Default::default()
        };
        let conflicting_identity = AddressEntry {
            city_name: "Bytom".to_string(),
            voivodeship: "Śląskie".to_string(),
            city_id: Some(999001),
            ..Default::default()
        };
        let wrong_known_voivodeship = AddressEntry {
            city_name: "Bytom".to_string(),
            voivodeship: "Mazowieckie".to_string(),
            city_id: Some(938670),
            ..Default::default()
        };

        for address in [&by_simc, &by_name, &unknown_voivodeship] {
            assert!(is_address_applicable_for_provider(
                &AlertSource::BytomskieWodociagi,
                address
            ));
        }
        for address in [
            &wrong_city,
            &wrong_voivodeship,
            &conflicting_identity,
            &wrong_known_voivodeship,
        ] {
            assert!(!is_address_applicable_for_provider(
                &AlertSource::BytomskieWodociagi,
                address
            ));
        }
    }

    #[test]
    fn bytomskie_wodociagi_has_a_stable_source_id_and_water_notification_classification() {
        assert_eq!(
            AlertSource::BytomskieWodociagi.to_string(),
            "bytomskie_wodociagi"
        );
        assert_eq!(
            AlertSource::BytomskieWodociagi.display_name(),
            "Bytomskie Wodociągi"
        );
        assert_eq!(
            serde_json::to_string(&AlertSource::BytomskieWodociagi).unwrap(),
            "\"bytomskie_wodociagi\""
        );

        let title = format_notification_title(
            &UnifiedAlert {
                source: AlertSource::BytomskieWodociagi,
                ..Default::default()
            },
            &Settings::default(),
            false,
        );
        assert_eq!(title, "Nowe wyłączenie wody - Bytomskie Wodociągi");
    }
}

#[cfg(test)]
mod regional_water_provider_tests {
    use super::{AddressEntry, AlertSource, is_address_applicable_for_provider};

    fn address(city: &str, commune: &str, voivodeship: &str, city_id: Option<u64>) -> AddressEntry {
        AddressEntry {
            city_name: city.to_string(),
            commune: commune.to_string(),
            voivodeship: voivodeship.to_string(),
            city_id,
            ..Default::default()
        }
    }

    #[test]
    fn new_sources_have_stable_ids_and_names() {
        let sources = [
            (AlertSource::MwikKoszalin, "mwik_koszalin", "MWiK Koszalin"),
            (
                AlertSource::SosnowieckieWodociagi,
                "sosnowieckie_wodociagi",
                "Sosnowieckie Wodociągi",
            ),
            (
                AlertSource::TarnowskieWodociagi,
                "tarnowskie_wodociagi",
                "Tarnowskie Wodociągi",
            ),
            (
                AlertSource::ZabrzanskieWodociagi,
                "zabrzanskie_wodociagi",
                "ZPWiK Zabrze",
            ),
            (
                AlertSource::PwikRudaSlaska,
                "pwik_ruda_slaska",
                "PWiK Ruda Śląska",
            ),
            (AlertSource::PwikRybnik, "pwik_rybnik", "PWiK Rybnik"),
        ];

        for (source, id, name) in sources {
            assert_eq!(source.to_string(), id);
            assert_eq!(source.display_name(), name);
            assert_eq!(serde_json::to_string(&source).unwrap(), format!("\"{id}\""));
        }
    }

    #[test]
    fn single_city_providers_reject_wrong_cities_and_voivodeships() {
        for (source, in_area, out_area) in [
            (
                AlertSource::MwikKoszalin,
                address("Koszalin", "Koszalin", "Zachodniopomorskie", Some(949_448)),
                address(
                    "Kołobrzeg",
                    "Kołobrzeg",
                    "Zachodniopomorskie",
                    Some(944_081),
                ),
            ),
            (
                AlertSource::SosnowieckieWodociagi,
                address("Sosnowiec", "Sosnowiec", "Śląskie", Some(943_428)),
                address("Sosnowiec", "Sędziszów", "Lubelskie", Some(13_592)),
            ),
            (
                AlertSource::ZabrzanskieWodociagi,
                address("Zabrze", "Zabrze", "Śląskie", Some(945_380)),
                address("Gliwice", "Gliwice", "Śląskie", Some(940_000)),
            ),
            (
                AlertSource::PwikRudaSlaska,
                address("Ruda Śląska", "Ruda Śląska", "Śląskie", Some(942_630)),
                address("Ruda Śląska", "Ruda Śląska", "Mazowieckie", Some(942_630)),
            ),
        ] {
            assert!(is_address_applicable_for_provider(&source, &in_area));
            assert!(!is_address_applicable_for_provider(&source, &out_area));
        }
    }

    #[test]
    fn tarnow_and_rybnik_providers_match_only_their_published_service_areas() {
        let tarnow = address("Tarnów", "Tarnów", "Małopolskie", Some(981_570));
        let skrzyszow_village = address("Łękawica", "Skrzyszów", "Małopolskie", Some(999_100));
        let zalasowa = address("Zalasowa", "Ryglice", "Małopolskie", Some(829_276));
        let machowa = address("Machowa", "Pilzno", "Podkarpackie", Some(826_220));
        let other_plesna_locality = address("Pleśna", "Pleśna", "Małopolskie", Some(827_060));
        let pilzno_town = address("Pilzno", "Pilzno", "Podkarpackie", Some(982_345));

        for included in [&tarnow, &skrzyszow_village, &zalasowa, &machowa] {
            assert!(is_address_applicable_for_provider(
                &AlertSource::TarnowskieWodociagi,
                included
            ));
        }
        assert!(!is_address_applicable_for_provider(
            &AlertSource::TarnowskieWodociagi,
            &other_plesna_locality
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::TarnowskieWodociagi,
            &pilzno_town
        ));

        let rybnik = address("Rybnik", "Rybnik", "Śląskie", Some(942_765));
        let gaszowice_village = address("Czernica", "Gaszowice", "Śląskie", Some(214_156));
        let katowice = address("Katowice", "Katowice", "Śląskie", Some(938_621));
        assert!(is_address_applicable_for_provider(
            &AlertSource::PwikRybnik,
            &rybnik
        ));
        assert!(is_address_applicable_for_provider(
            &AlertSource::PwikRybnik,
            &gaszowice_village
        ));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::PwikRybnik,
            &katowice
        ));
    }
}

