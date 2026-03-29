use crate::tauron::OutageItem;
use serde::{Deserialize, Serialize};

pub const MPWIK_URL: &str = "https://www.mpwik.wroc.pl/wp-admin/admin-ajax.php";
pub const FORTUM_URL: &str = "https://formularz.fortum.pl/api/v1/switchoffs";
pub const FORTUM_CITIES_URL: &str = "https://formularz.fortum.pl/api/v1/teryt/cities";

// ── Alert source abstraction ──────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertSource {
    Tauron,
    Water,
    Fortum,
    Energa,
    Enea,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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
}

// ── MPWiK (water) types ───────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MpwikFailureItem {
    pub content: Option<String>,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MpwikResponse {
    pub failures: Option<Vec<MpwikFailureItem>>,
}

// ── Fortum types ─────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FortumCity {
    pub city_guid: String,
    pub city_name: String,
    pub region_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct FortumResponse {
    #[serde(default)]
    pub points: Vec<FortumPoint>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FortumPoint {
    pub switch_off_id: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub message: Option<String>,
}

/// Parse MPWiK date format "DD-MM-YYYY HH:mm" into ISO "YYYY-MM-DDTHH:mm:00".
pub fn parse_mpwik_date(date_str: &str) -> Option<String> {
    let parts: Vec<&str> = date_str.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }
    Some(format!(
        "{}-{}-{}T{}:00",
        date_parts[2], date_parts[1], date_parts[0], parts[1]
    ))
}

impl MpwikFailureItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        UnifiedAlert {
            source: AlertSource::Water,
            startDate: self.date_start.as_deref().and_then(parse_mpwik_date),
            endDate: self.date_end.as_deref().and_then(parse_mpwik_date),
            message: self.content.clone(),
            description: None,
            address_index: None,
            is_local: None,
        }
    }
}

impl FortumPoint {
    pub fn to_unified(&self) -> UnifiedAlert {
        UnifiedAlert {
            source: AlertSource::Fortum,
            startDate: self.start_date.clone(),
            endDate: self.end_date.clone(),
            message: self.message.clone(),
            description: None,
            address_index: None,
            is_local: None,
        }
    }
}

impl OutageItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        UnifiedAlert {
            source: AlertSource::Tauron,
            startDate: self.StartDate.clone(),
            endDate: self.EndDate.clone(),
            message: self.Message.clone(),
            description: self.Description.clone(),
            address_index: None,
            is_local: None,
        }
    }
}

pub fn matches_address(
    message: &Option<String>,
    city_name: &str,
    street_name_1: &str,
    street_name_2: &Option<String>,
) -> bool {
    let Some(message) = message else {
        return false;
    };

    fn word_match(text: &str, word: &str) -> bool {
        let pattern = format!(r"(?i)\b{}\b", regex::escape(word));
        regex::Regex::new(&pattern)
            .map(|r| r.is_match(text))
            .unwrap_or(false)
    }

    // City must match
    if !word_match(message, city_name) {
        return false;
    }

    if street_name_1.is_empty() {
        return true;
    }

    // Build priority list: compound name first (if nazwa_2 exists), then individual words
    let mut candidates: Vec<String> = Vec::new();
    if let Some(n2) = street_name_2 {
        let compound = format!("{} {}", n2.trim(), street_name_1.trim());
        candidates.push(compound);
    }

    // Add significant individual words (>= 3 chars)
    for word in street_name_1.split_whitespace() {
        if word.len() >= 3 {
            candidates.push(word.to_string());
        }
    }
    if let Some(n2) = street_name_2 {
        for word in n2.split_whitespace() {
            if word.len() >= 3 {
                candidates.push(word.to_string());
            }
        }
    }

    candidates.iter().any(|c| word_match(message, c))
}

pub fn matches_street_only(
    message: &Option<String>,
    street_name_1: &str,
    street_name_2: &Option<String>,
) -> bool {
    let Some(message) = message else {
        return false;
    };

    fn word_match(text: &str, word: &str) -> bool {
        let pattern = format!(r"(?i)\b{}\b", regex::escape(word));
        regex::Regex::new(&pattern)
            .map(|r| r.is_match(text))
            .unwrap_or(false)
    }

    if street_name_1.is_empty() {
        return true;
    }

    // Build priority list: compound name first (if nazwa_2 exists), then individual words
    let mut candidates: Vec<String> = Vec::new();
    if let Some(n2) = street_name_2 {
        let compound = format!("{} {}", n2.trim(), street_name_1.trim());
        candidates.push(compound);
    }

    // Add significant individual words (>= 3 chars)
    for word in street_name_1.split_whitespace() {
        if word.len() >= 3 {
            candidates.push(word.to_string());
        }
    }
    if let Some(n2) = street_name_2 {
        for word in n2.split_whitespace() {
            if word.len() >= 3 {
                candidates.push(word.to_string());
            }
        }
    }

    candidates.iter().any(|c| word_match(message, c))
}

// ── Address & Settings ────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AddressEntry {
    pub name: String,
    #[serde(rename = "cityName")]
    pub city_name: String,
    #[serde(default)]
    pub voivodeship: String,
    #[serde(default)]
    pub district: String,
    #[serde(default)]
    pub commune: String,
    #[serde(rename = "streetName")]
    pub street_name: String,
    #[serde(rename = "streetName1", default)]
    pub street_name_1: String,
    #[serde(rename = "streetName2", default)]
    pub street_name_2: Option<String>,
    #[serde(rename = "houseNo")]
    pub house_no: String,
    #[serde(rename = "cityId", default)]
    pub city_id: Option<u64>,
    #[serde(rename = "streetId", default)]
    pub street_id: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Settings {
    #[serde(default)]
    pub addresses: Vec<AddressEntry>,
    #[serde(default, rename = "primaryAddressIndex")]
    pub primary_address_index: Option<usize>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default, rename = "enabledSources")]
    pub enabled_sources: Option<Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            addresses: Vec::new(),
            primary_address_index: None,
            theme: None,
            language: None,
            enabled_sources: Some(Vec::new()),
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
    fn test_parse_mpwik_date() {
        let date = "12-03-2026 08:30";
        let parsed = parse_mpwik_date(date);
        assert_eq!(parsed, Some("2026-03-12T08:30:00".to_string()));

        let invalid = "invalid date";
        assert_eq!(parse_mpwik_date(invalid), None);
    }

    #[test]
    fn test_mpwik_to_unified() {
        let item = MpwikFailureItem {
            content: Some("Test water outage".to_string()),
            date_start: Some("12-03-2026 08:30".to_string()),
            date_end: Some("12-03-2026 16:00".to_string()),
        };
        let unified = item.to_unified();
        assert_eq!(unified.source, AlertSource::Water);
        assert_eq!(unified.message, Some("Test water outage".to_string()));
        assert_eq!(unified.startDate, Some("2026-03-12T08:30:00".to_string()));
        assert_eq!(unified.endDate, Some("2026-03-12T16:00:00".to_string()));
    }

    #[test]
    fn test_tauron_to_unified() {
        let item = OutageItem {
            GAID: Some(123),
            Message: Some("Test power outage".to_string()),
            StartDate: Some("2026-03-12T08:30:00".to_string()),
            EndDate: Some("2026-03-12T16:00:00".to_string()),
            Description: Some("Testing".to_string()),
        };
        let unified = item.to_unified();
        assert_eq!(unified.source, AlertSource::Tauron);
        assert_eq!(unified.message, Some("Test power outage".to_string()));
        assert_eq!(unified.description, Some("Testing".to_string()));
    }
}
