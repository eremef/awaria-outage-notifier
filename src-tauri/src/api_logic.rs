use serde::{Deserialize, Serialize};

pub const BASE_URL: &str = "https://www.tauron-dystrybucja.pl/waapi";
pub const MPWIK_URL: &str = "https://www.mpwik.wroc.pl/wp-admin/admin-ajax.php";
pub const FORTUM_URL: &str = "https://formularz.fortum.pl/api/v1/switchoffs";
pub const FORTUM_CITY_GUID: &str = "d06e8606-f1d7-eb11-bacb-000d3aa9626e";
pub const FORTUM_REGION_ID: u32 = 3;

// ── Alert source abstraction ──────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertSource {
    Tauron,
    Water,
    Fortum,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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

    pub fn matches_street(&self, street_name: &str) -> bool {
        let Some(message) = &self.Message else {
            return false;
        };

        if message.contains(street_name) {
            return true;
        }

        let normalized = street_name
            .trim_start_matches("ul.")
            .trim_start_matches("al.")
            .trim_start_matches("pl.")
            .trim_start_matches("os.")
            .trim_start_matches("rondo ")
            .trim();

        let significant_words: Vec<&str> = normalized
            .split_whitespace()
            .filter(|w: &&str| w.len() >= 3)
            .collect();

        significant_words.iter().any(|word| {
            let regex = format!(r"(?i)\b{}\b", regex::escape(word));
            regex::Regex::new(&regex)
                .map(|r| r.is_match(message))
                .unwrap_or(false)
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[allow(non_snake_case)]
pub struct GeoItem {
    pub GAID: u64,
    pub Name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct OutageItem {
    pub GAID: Option<u64>,
    pub Message: Option<String>,
    pub StartDate: Option<String>,
    pub EndDate: Option<String>,
    pub Description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct OutageResponse {
    pub OutageItems: Option<Vec<OutageItem>>,
    pub debug_query: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AddressEntry {
    pub name: String,
    #[serde(rename = "cityName")]
    pub city_name: String,
    #[serde(rename = "streetName")]
    pub street_name: String,
    #[serde(rename = "houseNo")]
    pub house_no: String,
    #[serde(rename = "cityGAID")]
    pub city_gaid: u64,
    #[serde(rename = "streetGAID")]
    pub street_gaid: u64,
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
    #[serde(default, skip_deserializing, rename = "cityName")]
    pub city_name: String,
    #[serde(default, skip_deserializing, rename = "streetName")]
    pub street_name: String,
    #[serde(default, skip_deserializing, rename = "houseNo")]
    pub house_no: String,
    #[serde(default, skip_deserializing, rename = "cityGAID")]
    pub city_gaid: u64,
    #[serde(default, skip_deserializing, rename = "streetGAID")]
    pub street_gaid: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            addresses: Vec::new(),
            primary_address_index: None,
            theme: None,
            language: None,
            enabled_sources: Some(vec![
                "tauron".to_string(),
                "water".to_string(),
                "fortum".to_string(),
            ]),
            city_name: String::new(),
            street_name: String::new(),
            house_no: String::new(),
            city_gaid: 0,
            street_gaid: 0,
        }
    }
}

// impl Settings {
//     pub fn migrate_legacy(&mut self) {
//         if self.addresses.is_empty() && (self.city_gaid > 0 || !self.city_name.is_empty()) {
//             self.addresses.push(AddressEntry {
//                 name: "Address 1".to_string(),
//                 city_name: self.city_name.clone(),
//                 street_name: self.street_name.clone(),
//                 house_no: self.house_no.clone(),
//                 city_gaid: self.city_gaid,
//                 street_gaid: self.street_gaid,
//             });
//             self.primary_address_index = Some(0);
//         }
//         self.city_name = String::new();
//         self.street_name = String::new();
//         self.house_no = String::new();
//         self.city_gaid = 0;
//         self.street_gaid = 0;
//     }
// }

pub fn get_cities_query(city_name: &str, cache_bust: &str) -> Vec<(&'static str, String)> {
    vec![
        ("partName", city_name.to_string()),
        ("_", cache_bust.to_string()),
    ]
}

pub fn get_streets_query(
    street_name: &str,
    city_gaid: u64,
    cache_bust: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("partName", street_name.to_string()),
        ("ownerGAID", city_gaid.to_string()),
        ("_", cache_bust.to_string()),
    ]
}

pub fn get_outages_query(
    city_gaid: u64,
    street_gaid: u64,
    house_no: &str,
    from_date: &str,
    cache_bust: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("cityGAID", city_gaid.to_string()),
        ("streetGAID", street_gaid.to_string()),
        ("houseNo", house_no.to_string()),
        ("fromDate", from_date.to_string()),
        ("getLightingSupport", "false".to_string()),
        ("getServicedSwitchingoff", "true".to_string()),
        ("_", cache_bust.to_string()),
    ]
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
            city_name: "Wrocław".to_string(),
            street_name: "Kuźnicza".to_string(),
            house_no: "25".to_string(),
            city_gaid: 123,
            street_gaid: 456,
            theme: Some("dark".to_string()),
            language: Some("pl".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_cities_query() {
        let query = get_cities_query("Wro", "12345");
        assert_eq!(query.len(), 2);
        assert_eq!(query[0], ("partName", "Wro".to_string()));
        assert_eq!(query[1], ("_", "12345".to_string()));
    }

    #[test]
    fn test_streets_query() {
        let query = get_streets_query("Roz", 123, "12345");
        assert_eq!(query.len(), 3);
        assert_eq!(query[0], ("partName", "Roz".to_string()));
        assert_eq!(query[1], ("ownerGAID", "123".to_string()));
        assert_eq!(query[2], ("_", "12345".to_string()));
    }

    #[test]
    fn test_outages_query() {
        let query = get_outages_query(123, 456, "5", "2024-01-01", "12345");
        assert_eq!(query.len(), 7);
        assert_eq!(query[0], ("cityGAID", "123".to_string()));
        assert_eq!(query[1], ("streetGAID", "456".to_string()));
        assert_eq!(query[2], ("houseNo", "5".to_string()));
    }

    #[test]
    fn test_settings_persistence() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("test_settings.json");

        let settings = Settings {
            city_name: "TestCity".to_string(),
            street_name: "TestStreet".to_string(),
            house_no: "10".to_string(),
            city_gaid: 111,
            street_gaid: 222,
            theme: Some("light".to_string()),
            language: Some("en".to_string()),
            ..Default::default()
        };

        // Save
        save_settings_to_path(&test_path, &settings).expect("Failed to save settings");

        // Load
        let loaded = load_settings_from_path(&test_path).expect("Failed to load settings");
        assert_eq!(Some(settings), loaded);

        // Cleanup
        std::fs::remove_file(test_path).ok();
    }

    #[test]
    fn test_load_non_existent_settings() {
        let test_path = std::path::Path::new("non_existent_settings.json");
        let loaded = load_settings_from_path(test_path).expect("Failed to load settings");
        assert_eq!(None, loaded);
    }

    #[test]
    fn test_load_corrupt_settings() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("corrupt_settings.json");
        std::fs::write(&test_path, "{ invalid json }").unwrap();

        let result = load_settings_from_path(&test_path);
        assert!(result.is_err());
        // Just verify it's an error, exact message can vary

        std::fs::remove_file(test_path).ok();
    }

    #[test]
    fn test_load_legacy_settings_missing_fields() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("legacy_settings.json");
        // Theme is optional, but let's see if we missing other fields how it behaves
        let legacy_json = r#"{
            "cityName": "Legacy",
            "streetName": "Old St",
            "houseNo": "1",
            "cityGAID": 1,
            "streetGAID": 2
        }"#;
        std::fs::write(&test_path, legacy_json).unwrap();

        let loaded =
            load_settings_from_path(&test_path).expect("Should handle missing optional fields");
        assert!(loaded.is_some());
        let s = loaded.unwrap();
        assert_eq!(s.city_name, "Legacy");
        assert_eq!(s.theme, None); // Should default to None

        std::fs::remove_file(test_path).ok();
    }

    #[test]
    fn test_parse_outage_response() {
        let json = r#"{
            "OutageItems": [
                {
                    "GAID": 100,
                    "Message": "Outage at St.",
                    "StartDate": "2024-01-01T10:00:00",
                    "EndDate": "2024-01-01T12:00:00",
                    "Description": "Testing"
                }
            ]
        }"#;
        let response: OutageResponse =
            serde_json::from_str(json).expect("Failed to parse OutageResponse");
        let items = response.OutageItems.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].GAID, Some(100));
        assert_eq!(items[0].Message.as_deref(), Some("Outage at St."));
    }

    #[test]
    fn test_parse_incomplete_outage_response() {
        // Test that we handle missing optional fields gracefully
        let json = r#"{
            "OutageItems": [
                {
                    "GAID": 101
                }
            ]
        }"#;
        let response: OutageResponse =
            serde_json::from_str(json).expect("Failed to parse OutageResponse");
        let items = response.OutageItems.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].GAID, Some(101));
        assert!(items[0].Message.is_none());
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
