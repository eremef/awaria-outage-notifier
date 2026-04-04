use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sha2::{Sha256, Digest};

// ── Alert source abstraction ──────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertSource {
    Tauron,
    Water,
    Fortum,
    Energa,
    Enea,
    Pge,
    Stoen,
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

impl std::fmt::Display for AlertSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AlertSource::Tauron => "tauron",
            AlertSource::Water => "water",
            AlertSource::Fortum => "fortum",
            AlertSource::Energa => "energa",
            AlertSource::Enea => "enea",
            AlertSource::Pge => "pge",
            AlertSource::Stoen => "stoen",
        };
        write!(f, "{}", s)
    }
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
    #[serde(default = "default_true", rename = "isActive")]
    pub is_active: bool,
}

fn default_true() -> bool {
    true
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
    #[serde(default, rename = "notificationPreferences")]
    pub notification_preferences: HashMap<String, bool>,
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
    fn test_unified_alert_hashing() {
        let alert1 = UnifiedAlert {
            source: AlertSource::Tauron,
            startDate: Some("2024-01-01 10:00".to_string()),
            endDate: None,
            message: Some("Brak prądu".to_string()),
            description: None,
            address_index: None,
            is_local: None,
        };

        let alert2 = UnifiedAlert {
            source: AlertSource::Tauron,
            startDate: Some("2024-01-01 10:00".to_string()),
            endDate: Some("2024-01-01 14:00".to_string()),
            message: Some("Brak prądu".to_string()),
            description: Some("Different desc".to_string()),
            address_index: Some(1),
            is_local: Some(true),
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
}
