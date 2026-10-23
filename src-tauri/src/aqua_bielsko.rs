use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const AQUA_EVENTS_URL: &str =
    "https://komunikaty.aqua.com.pl/api/index.php?m=getActiveEventsData";

#[derive(Debug, Deserialize)]
struct EventsResponse {
    events: Vec<AquaEvent>,
}

#[derive(Debug, Deserialize)]
struct AquaEvent {
    #[serde(default)]
    event_type: Option<String>,
    #[serde(default)]
    data_start: Option<String>,
    #[serde(default)]
    time_start: Option<String>,
    #[serde(default)]
    data_end: Option<String>,
    #[serde(default)]
    time_end: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HouseNumber {
    number: u32,
    suffix: Option<char>,
}

impl Ord for HouseNumber {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.number
            .cmp(&other.number)
            .then_with(|| self.suffix.cmp(&other.suffix))
    }
}

impl PartialOrd for HouseNumber {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn street_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(?:przy\s+)?(?:ul\.|ulica)\s*").unwrap())
}

fn locality_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(?:,\s*przy\s+w\s+|,\s*w\s+|\bprzy\s+w\s+)").unwrap())
}

fn street_number_marker() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\s+(?:nr\.?|numery|numer)\s*").unwrap())
}

fn number_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{1,4}[a-z]?)[ \t]*(?:-|–|—|−|\bdo\b)[ \t]*(\d{1,4}[a-z]?)\b").unwrap()
    })
}

fn house_numbers_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(\d{1,4}[a-z]?)\b").unwrap())
}

fn normalize(value: &str) -> String {
    crate::utils::clean_text(value)
        .replace(['–', '—', '−'], "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_number(value: &str) -> Option<HouseNumber> {
    let value = value.trim().to_ascii_lowercase();
    let digits = value
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    let number = digits.parse().ok()?;
    let suffix = value[digits.len()..]
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic());
    Some(HouseNumber { number, suffix })
}

fn parse_time(value: Option<&str>) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(value?.trim(), "%H:%M").ok()
}

fn format_date_time(date: Option<&str>, time: Option<&str>) -> Option<String> {
    let raw_date = date?.trim();
    let date = NaiveDate::parse_from_str(raw_date, "%Y-%m-%d").ok()?;
    match parse_time(time) {
        Some(time) => Some(date.and_time(time).format("%Y-%m-%dT%H:%M:%S").to_string()),
        None => Some(date.format("%Y-%m-%d").to_string()),
    }
}

fn event_type_label(event_type: &str) -> &'static str {
    match event_type {
        "AWA" => "Awaria sieci wodociągowej",
        "WYL" => "Planowane wyłączenie wody",
        "PNS" => "Planowane prace remontowe",
        "ZMC" => "Obniżone ciśnienie wody",
        "PJW" => "Pogorszona jakość wody",
        "AWE" => "Przerwa w zasilaniu energetycznym",
        "INNE" => "Komunikat AQUA",
        _ => "Komunikat AQUA",
    }
}

fn event_hash(event: &AquaEvent) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    event
        .event_type
        .as_deref()
        .unwrap_or_default()
        .hash(&mut hasher);
    event
        .data_start
        .as_deref()
        .unwrap_or_default()
        .hash(&mut hasher);
    event
        .time_start
        .as_deref()
        .unwrap_or_default()
        .hash(&mut hasher);
    event
        .data_end
        .as_deref()
        .unwrap_or_default()
        .hash(&mut hasher);
    event
        .time_end
        .as_deref()
        .unwrap_or_default()
        .hash(&mut hasher);
    normalize(event.description.as_deref().unwrap_or_default()).hash(&mut hasher);
    format!("aqua_bielsko_{:016x}", hasher.finish())
}

fn notice_sections(notice: &str) -> Vec<&str> {
    let mut sections = Vec::new();
    let mut start = 0;
    for marker in locality_marker_regex().find_iter(notice) {
        if marker.start() > start {
            sections.push(
                notice[start..marker.start()].trim_matches(|c: char| c == ',' || c.is_whitespace()),
            );
        }
        start = marker.end();
    }
    if start < notice.len() {
        sections.push(notice[start..].trim_matches(|c: char| c == ',' || c.is_whitespace()));
    }
    if sections.is_empty() {
        sections.push(notice);
    }
    sections
}

fn section_matches_address(section: &str, address: &AddressEntry) -> bool {
    let text = normalize(section);
    [address.city_name.as_str(), address.commune.as_str()]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .map(normalize)
        .any(|name| {
            if name.is_empty() {
                return false;
            }
            let name_tokens = name
                .split(|character: char| !character.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .collect::<Vec<_>>();
            let text_tokens = text
                .split(|character: char| !character.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .collect::<Vec<_>>();
            text_tokens
                .windows(name_tokens.len())
                .any(|words| words == name_tokens)
        })
}

fn stem_token(token: &str) -> String {
    for suffix in [
        "skiego", "ckiego", "skich", "ckich", "skiej", "ckiej", "owej", "owym", "owych", "ami",
        "ach", "ego", "emu", "iej", "ich", "ych", "owa", "owe", "owej", "ska", "skie", "ski", "ej",
        "owi", "ow", "em", "ie", "a", "u",
    ] {
        if token.len() > suffix.len() + 2 && token.ends_with(suffix) {
            return token[..token.len() - suffix.len()].to_string();
        }
    }
    token.to_string()
}

fn street_matches(street_name: &str, candidate: &str) -> bool {
    let candidate = crate::utils::strip_street_prefixes(candidate.trim());
    let expected = normalize(candidate)
        .split_whitespace()
        .filter(|token| token.chars().any(char::is_alphabetic))
        .map(stem_token)
        .collect::<Vec<_>>();
    let actual = normalize(street_name)
        .split_whitespace()
        .filter(|token| token.chars().any(char::is_alphabetic))
        .map(stem_token)
        .collect::<Vec<_>>();
    !expected.is_empty()
        && expected.iter().all(|word| {
            actual
                .iter()
                .any(|found| found == word || found.starts_with(word) || word.starts_with(found))
        })
}

fn street_segments(section: &str) -> Vec<(String, String)> {
    let mut segments = Vec::new();
    for matched in street_marker_regex().find_iter(section) {
        let rest = &section[matched.end()..];
        let end = street_marker_regex()
            .find(rest)
            .map(|next| next.start())
            .unwrap_or(rest.len());
        let street_info = rest[..end].trim().trim_start_matches(':').trim();
        let (name, numbers) = street_number_marker()
            .find(street_info)
            .map(|marker| {
                (
                    &street_info[..marker.start()],
                    street_info[marker.end()..].trim(),
                )
            })
            .unwrap_or((street_info, street_info));
        if !name.trim().is_empty() {
            segments.push((
                name.trim().to_string(),
                if numbers.is_empty() {
                    street_info.to_string()
                } else {
                    numbers.to_string()
                },
            ));
        }
    }
    segments
}

fn matching_notice_segments(notice: &str, address: &AddressEntry) -> Vec<String> {
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    notice_sections(notice)
        .into_iter()
        .filter(|section| section_matches_address(section, address))
        .flat_map(street_segments)
        .filter(|(name, _)| {
            candidates
                .iter()
                .any(|candidate| street_matches(name, candidate))
        })
        .map(|(_, numbers)| numbers)
        .collect()
}

pub fn location_matches_address(notice: &str, address: &AddressEntry) -> bool {
    !matching_notice_segments(notice, address).is_empty()
}

fn house_number_matches_segment(segment: &str, requested: &str) -> Option<bool> {
    let requested = parse_number(requested)?;
    let normalized = normalize(segment);
    let odd_only = normalized.contains("nieparzyste") || normalized.contains("nieparz");
    let even_only = !odd_only && (normalized.contains("parzyste") || normalized.contains("parz"));
    if (odd_only && requested.number.is_multiple_of(2))
        || (even_only && !requested.number.is_multiple_of(2))
    {
        return Some(false);
    }
    let ranges = number_range_regex()
        .captures_iter(segment)
        .filter_map(|capture| {
            Some((
                parse_number(capture.get(1)?.as_str())?,
                parse_number(capture.get(2)?.as_str())?,
            ))
        })
        .collect::<Vec<_>>();
    if !ranges.is_empty() {
        return Some(ranges.into_iter().any(|(start, end)| {
            let (low, high) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            requested >= low && requested <= high
        }));
    }
    let listed = house_numbers_regex()
        .captures_iter(segment)
        .filter_map(|capture| {
            capture
                .get(1)
                .and_then(|value| parse_number(value.as_str()))
        })
        .collect::<Vec<_>>();
    (!listed.is_empty()).then(|| listed.contains(&requested))
}

/// House-number enforcement is called only when the central setting is enabled.
pub fn house_number_matches_notice(notice: &str, address: &AddressEntry) -> bool {
    if address.house_no.trim().is_empty() {
        return true;
    }
    let results = matching_notice_segments(notice, address)
        .into_iter()
        .filter_map(|segment| house_number_matches_segment(&segment, &address.house_no))
        .collect::<Vec<_>>();
    results.is_empty() || results.into_iter().any(|result| result)
}

fn parse_events(json: &str, settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    let response: EventsResponse = serde_json::from_str(json)
        .map_err(|error| format!("AQUA event response JSON error: {error}"))?;
    let mut alerts = Vec::new();
    for event in response.events {
        let Some(description) = event
            .description
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        else {
            continue;
        };
        let source_label = event_type_label(event.event_type.as_deref().unwrap_or_default());
        let address_index = settings
            .addresses
            .iter()
            .enumerate()
            .find(|(_, address)| {
                address.is_active
                    && crate::api_logic::is_address_applicable_for_provider(
                        &AlertSource::AquaBielsko,
                        address,
                    )
                    && location_matches_address(description, address)
            })
            .map(|(index, _)| index);
        alerts.push(UnifiedAlert {
            source: AlertSource::AquaBielsko,
            startDate: format_date_time(event.data_start.as_deref(), event.time_start.as_deref()),
            endDate: format_date_time(event.data_end.as_deref(), event.time_end.as_deref()),
            message: Some(format!("{source_label}: {description}")),
            location: Some(description.to_string()),
            address_index,
            is_local: Some(address_index.is_some()),
            hash: Some(event_hash(&event)),
        });
    }
    Ok(alerts)
}

async fn fetch_events(client: &Client) -> Result<String, String> {
    crate::utils::retry(
        || async {
            let response = client
                .get(AQUA_EVENTS_URL)
                .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if !response.status().is_success() {
                return Err(format!("HTTP {}", response.status()));
            }
            response.text().await.map_err(|error| error.to_string())
        },
        2,
    )
    .await
}

pub struct AquaBielskoProvider;

#[async_trait]
impl AlertProvider for AquaBielskoProvider {
    fn id(&self) -> String {
        "aqua_bielsko".to_string()
    }
    fn source(&self) -> AlertSource {
        AlertSource::AquaBielsko
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        if !crate::api_logic::is_provider_applicable(self.source(), settings) {
            return (Vec::new(), Vec::new());
        }
        match fetch_events(client).await {
            Ok(json) => match parse_events(&json, settings) {
                Ok(alerts) => (alerts, Vec::new()),
                Err(error) => (Vec::new(), vec![error]),
            },
            Err(error) => (Vec::new(), vec![format!("AQUA request error: {error}")]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(city: &str, commune: &str, street: &str, house: &str) -> AddressEntry {
        AddressEntry {
            name: "Home".into(),
            city_name: city.into(),
            voivodeship: "Śląskie".into(),
            district: "bielski".into(),
            commune: commune.into(),
            street_name: street.into(),
            street_name_1: street.into(),
            street_name_2: None,
            house_no: house.into(),
            city_id: None,
            street_id: None,
            is_active: true,
        }
    }

    fn fixture() -> &'static str {
        r#"{"events":[{"event_type":"AWA","data_start":"2026-10-23","time_start":"12:40","data_end":"2026-10-23","time_end":"18:30","description":"Awaria w dniu 2026-10-23 w Bielsko-Biała przy ul. Darniowa nr 4, 6, 26, 32, 34A, przy ul. Dziewanny nr 60, 68, przy w Jasienica przy ul. Astrów nr 2, 4, 5, przy ul. Cieszyńska nr 6, 8, przy w Jaworze przy ul. Bielska nr 21"}]}"#
    }

    #[test]
    fn parses_schedule_type_description_and_empty_feed() {
        let mut settings = Settings::default();
        settings
            .addresses
            .push(address("Jasienica", "Jasienica", "Astrów", "4"));
        let alerts = parse_events(fixture(), &settings).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-23T12:40:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-10-23T18:30:00"));
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .starts_with("Awaria sieci wodociągowej:")
        );
        assert_eq!(alerts[0].is_local, Some(true));
        assert!(
            parse_events(r#"{"events":[]}"#, &settings)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn matches_locality_and_own_street_not_another_municipality_or_street() {
        let notice = &serde_json::from_str::<EventsResponse>(fixture())
            .unwrap()
            .events[0]
            .description
            .as_deref()
            .unwrap()
            .to_string();
        assert!(location_matches_address(
            notice,
            &address("Jasienica", "Jasienica", "Astrów", "4")
        ));
        assert!(!location_matches_address(
            notice,
            &address("Jasienica", "Jasienica", "Darniowa", "4")
        ));
        assert!(location_matches_address(
            notice,
            &address("Bielsko-Biała", "Bielsko-Biała", "Darniowa", "4")
        ));
        assert!(!location_matches_address(
            notice,
            &address("Kozy", "Kozy", "Astrów", "4")
        ));
    }

    #[test]
    fn house_number_ranges_lists_suffixes_and_missing_bounds() {
        let event = serde_json::from_str::<EventsResponse>(fixture())
            .unwrap()
            .events
            .remove(0);
        let notice = event.description.unwrap();
        assert!(house_number_matches_notice(
            &notice,
            &address("Bielsko-Biała", "Bielsko-Biała", "Darniowa", "34a")
        ));
        assert!(!house_number_matches_notice(
            &notice,
            &address("Bielsko-Biała", "Bielsko-Biała", "Darniowa", "35")
        ));
        assert!(house_number_matches_notice(
            &notice,
            &address("Jasienica", "Jasienica", "Astrów", "4")
        ));
        let missing_time = r#"{"events":[{"event_type":"ZMC","data_start":"2026-10-23","description":"Ciśnienie w Bielsko-Biała przy ul. Darniowa nr 4"}]}"#;
        let alerts = parse_events(missing_time, &Settings::default()).unwrap();
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-23"));
        assert_eq!(alerts[0].endDate, None);
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .starts_with("Obniżone ciśnienie wody:")
        );
    }

    #[test]
    fn malformed_response_is_reported_and_unrecognized_event_type_preserved() {
        assert!(parse_events("not json", &Settings::default()).is_err());
        let json = r#"{"events":[{"event_type":"NEW","data_start":"2026-10-23","description":"Water notice"}]}"#;
        let alerts = parse_events(json, &Settings::default()).unwrap();
        assert_eq!(
            alerts[0].message.as_deref(),
            Some("Komunikat AQUA: Water notice")
        );
    }
}
