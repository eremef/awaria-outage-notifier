use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;
use std::sync::OnceLock;

pub const MWIK_BYDGOSZCZ_PAGE_URL: &str = "https://mwik.bydgoszcz.pl/wp-json/wp/v2/pages/8760";

pub struct MwikBydgoszczProvider;

#[derive(Debug, Deserialize)]
struct WordPressPage {
    content: WordPressContent,
}

#[derive(Debug, Deserialize)]
struct WordPressContent {
    rendered: String,
}

#[derive(Clone, Copy)]
enum NoticeKind {
    Planned,
    Emergency,
}

impl NoticeKind {
    fn from_heading(heading: &str) -> Option<Self> {
        let normalized = normalize_polish(heading);
        if normalized.contains("planowane wylaczenie wody") {
            Some(Self::Planned)
        } else if normalized.contains("awaryjne wylaczenie wody") {
            Some(Self::Emergency)
        } else {
            None
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Planned => "Planowane wyłączenie wody",
            Self::Emergency => "Awaryjne wyłączenie wody",
        }
    }
}

fn time_range_regex() -> &'static Regex {
    static TIME_RANGE_RE: OnceLock<Regex> = OnceLock::new();
    TIME_RANGE_RE
        .get_or_init(|| Regex::new(r"^\s*(\d{1,2}:\d{2})\s*[-–]\s*(\d{1,2}:\d{2})\s*$").unwrap())
}

fn normalize_polish(text: &str) -> String {
    text.to_lowercase()
        .replace('ą', "a")
        .replace('ć', "c")
        .replace('ę', "e")
        .replace('ł', "l")
        .replace('ń', "n")
        .replace('ó', "o")
        .replace('ś', "s")
        .replace(['ź', 'ż'], "z")
}

fn element_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_clock(value: &str) -> Option<NaiveTime> {
    let (hour, minute) = value.split_once(':')?;
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
}

fn parse_time_range(value: &str) -> Option<(NaiveTime, NaiveTime)> {
    let captures = time_range_regex().captures(value)?;
    let start = parse_clock(captures.get(1)?.as_str())?;
    let end = parse_clock(captures.get(2)?.as_str())?;
    (end > start).then_some((start, end))
}

fn format_datetime(datetime: NaiveDateTime) -> String {
    datetime.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn street_matches_address(text: &str, address: &AddressEntry) -> bool {
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );

    let notice_tokens: Vec<_> = normalize_polish(text)
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();

    candidates.into_iter().any(|candidate| {
        let candidate = crate::utils::strip_street_prefixes(candidate.trim());
        let tokens: Vec<_> = normalize_polish(candidate)
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .map(str::to_string)
            .collect();
        !tokens.is_empty()
            && tokens.iter().all(|token| {
                let stem = if token.ends_with('a') && token.chars().count() > 5 {
                    &token[..token.len() - 1]
                } else {
                    token.as_str()
                };
                notice_tokens
                    .iter()
                    .any(|notice_token| notice_token.starts_with(stem))
            })
    })
}

fn address_matches_notice(street_text: &str, address: &AddressEntry) -> bool {
    let has_street = !address.street_name.trim().is_empty()
        || !address.street_name_1.trim().is_empty()
        || address
            .street_name_2
            .as_deref()
            .is_some_and(|street| !street.trim().is_empty());
    !has_street || street_matches_address(street_text, address)
}

fn parse_notice_table(
    table: ElementRef<'_>,
    kind: NoticeKind,
    active_addresses: &[(usize, &AddressEntry)],
) -> Vec<UnifiedAlert> {
    let row_selector = Selector::parse("tr").unwrap();
    let cell_selector = Selector::parse("th, td").unwrap();
    let mut alerts = Vec::new();

    for row in table.select(&row_selector) {
        let cells: Vec<_> = row.select(&cell_selector).map(element_text).collect();
        if cells.len() < 3 {
            continue;
        }

        let date_text = cells[0].trim();
        let time_text = cells[1].trim();
        let street_text = cells[2].trim();
        if normalize_polish(date_text) == "data"
            || normalize_polish(date_text) == "brak"
            || (date_text.is_empty() && time_text.is_empty() && street_text.is_empty())
        {
            continue;
        }

        let Some(date) = NaiveDate::parse_from_str(date_text, "%d.%m.%Y").ok() else {
            log::warn!("MWiK Bydgoszcz skipped row with invalid date: {date_text}");
            continue;
        };
        let Some((start_time, end_time)) = parse_time_range(time_text) else {
            log::warn!("MWiK Bydgoszcz skipped row with invalid time range: {time_text}");
            continue;
        };
        if street_text.is_empty() {
            log::warn!("MWiK Bydgoszcz skipped row without a street");
            continue;
        }

        let start = date.and_time(start_time);
        let end = date.and_time(end_time);
        let address_index = active_addresses
            .iter()
            .find(|(_, address)| address_matches_notice(street_text, address))
            .map(|(index, _)| *index);

        alerts.push(UnifiedAlert {
            source: AlertSource::MwikBydgoszcz,
            startDate: Some(format_datetime(start)),
            endDate: Some(format_datetime(end)),
            message: Some(format!("{}: {}", kind.label(), street_text)),
            location: Some("Miejscowość: Bydgoszcz".to_string()),
            address_index,
            is_local: Some(address_index.is_some()),
            hash: None,
        });
    }

    alerts
}

fn parse_rendered_content(html: &str, settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    let document = Html::parse_fragment(html);
    let heading_selector = Selector::parse("h2").map_err(|error| error.to_string())?;
    let section_table_selector =
        Selector::parse("h2 + figure table").map_err(|error| error.to_string())?;

    let sections: Vec<_> = document
        .select(&heading_selector)
        .filter_map(|heading| {
            NoticeKind::from_heading(&element_text(heading))
                .map(|kind| (kind, element_text(heading)))
        })
        .collect();
    let tables: Vec<_> = document.select(&section_table_selector).collect();
    if sections.is_empty() {
        return Err("MWiK Bydgoszcz page contains no outage sections".to_string());
    }
    if sections.len() != tables.len() {
        return Err(format!(
            "MWiK Bydgoszcz page section/table count mismatch: {} sections, {} tables",
            sections.len(),
            tables.len()
        ));
    }

    let active_addresses: Vec<_> = settings
        .addresses
        .iter()
        .enumerate()
        .filter(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::MwikBydgoszcz,
                    address,
                )
        })
        .collect();

    Ok(sections
        .into_iter()
        .zip(tables)
        .flat_map(|((kind, _heading), table)| parse_notice_table(table, kind, &active_addresses))
        .collect())
}

pub fn parse_wordpress_page_json(
    json: &str,
    settings: &Settings,
) -> Result<Vec<UnifiedAlert>, String> {
    let page: WordPressPage = serde_json::from_str(json)
        .map_err(|error| format!("MWiK Bydgoszcz JSON parse error: {error}"))?;
    parse_rendered_content(&page.content.rendered, settings)
}

#[async_trait]
impl AlertProvider for MwikBydgoszczProvider {
    fn id(&self) -> String {
        "mwik_bydgoszcz".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::MwikBydgoszcz
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

        let response = match client
            .get(MWIK_BYDGOSZCZ_PAGE_URL)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("MWiK Bydgoszcz request error: {error}")],
                );
            }
        };
        if !response.status().is_success() {
            return (
                Vec::new(),
                vec![format!("MWiK Bydgoszcz HTTP error: {}", response.status())],
            );
        }

        let json = match response.text().await {
            Ok(json) => json,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("MWiK Bydgoszcz response read error: {error}")],
                );
            }
        };

        match parse_wordpress_page_json(&json, settings) {
            Ok(alerts) => (alerts, Vec::new()),
            Err(error) => (Vec::new(), vec![error]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENT: &str = r#"
        <h2>Planowane wyłączenie wody</h2>
        <figure class="wp-block-table"><table><tbody>
          <tr><td><strong>DATA</strong></td><td><strong>GODZINA</strong></td><td><strong>ULICA</strong></td></tr>
          <tr><td>19.10.2026</td><td>7:00-15:00</td><td>Focha 4</td></tr>
          <tr><td>19.10.2026</td><td>10:00 – 12:00</td><td>Modrzewiowa</td></tr>
          <tr><td></td><td></td><td></td></tr>
        </tbody></table></figure>
        <h2>Awaryjne wyłączenie wody</h2>
        <figure class="wp-block-table"><table><tbody>
          <tr><td><strong>DATA</strong></td><td><strong>GODZINA</strong></td><td><strong>ULICA</strong></td></tr>
          <tr><td>21.09.2026</td><td>10:10 – 17:00</td><td>Skowronków, Drozdów, Cietrzewia</td></tr>
          <tr><td>BRAK</td><td></td><td></td></tr>
          <tr><td></td><td></td><td></td></tr>
        </tbody></table></figure>
    "#;

    fn settings(city: &str, street: &str, house_no: &str, filter_by_house_no: bool) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Home".to_string(),
                city_name: city.to_string(),
                voivodeship: "Kujawsko-Pomorskie".to_string(),
                district: "Bydgoszcz".to_string(),
                commune: "Bydgoszcz".to_string(),
                street_name: street.to_string(),
                street_name_1: street.to_string(),
                street_name_2: None,
                house_no: house_no.to_string(),
                city_id: Some(if city == "Bydgoszcz" { 928363 } else { 929448 }),
                street_id: None,
                is_active: true,
            }],
            primary_address_index: Some(0),
            language: Some("pl".to_string()),
            filter_by_house_no,
            ..Settings::default()
        }
    }

    fn page_json(content: &str) -> String {
        serde_json::json!({"content": {"rendered": content}}).to_string()
    }

    #[test]
    fn parses_planned_and_emergency_rows_from_wordpress_json() {
        let alerts = parse_wordpress_page_json(
            &page_json(CONTENT),
            &settings("Bydgoszcz", "Focha", "99", false),
        )
        .unwrap();

        assert_eq!(alerts.len(), 3);
        assert_eq!(alerts[0].source, AlertSource::MwikBydgoszcz);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-19T07:00:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-10-19T15:00:00"));
        assert_eq!(alerts[0].is_local, Some(true));
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .contains("Planowane wyłączenie wody")
        );
        assert!(alerts[0].message.as_deref().unwrap().contains("Focha 4"));
        assert_eq!(
            alerts[0].location.as_deref(),
            Some("Miejscowość: Bydgoszcz")
        );
        assert_eq!(alerts[1].startDate.as_deref(), Some("2026-10-19T10:00:00"));
        assert_eq!(alerts[1].endDate.as_deref(), Some("2026-10-19T12:00:00"));
        assert_eq!(alerts[1].is_local, Some(false));
        assert_eq!(alerts[2].startDate.as_deref(), Some("2026-09-21T10:10:00"));
        assert_eq!(alerts[2].endDate.as_deref(), Some("2026-09-21T17:00:00"));
        assert_eq!(alerts[2].is_local, Some(false));
        assert!(
            alerts[2]
                .message
                .as_deref()
                .unwrap()
                .contains("Awaryjne wyłączenie wody")
        );
    }

    #[test]
    fn matches_each_street_and_leaves_house_number_filtering_to_settings() {
        for filter_by_house_no in [false, true] {
            let alerts = parse_wordpress_page_json(
                &page_json(CONTENT),
                &settings("Bydgoszcz", "Cietrzewia", "999", filter_by_house_no),
            )
            .unwrap();

            assert_eq!(alerts[2].is_local, Some(true));
            assert_eq!(alerts[2].address_index, Some(0));
            assert!(
                alerts[2]
                    .message
                    .as_deref()
                    .unwrap()
                    .contains("Skowronków, Drozdów, Cietrzewia")
            );
        }
    }

    #[test]
    fn skips_empty_states_and_malformed_rows() {
        let content = r#"
            <h2>Planowane wyłączenie wody</h2>
            <figure><table><tbody>
              <tr><td>DATA</td><td>GODZINA</td><td>ULICA</td></tr>
              <tr><td>BRAK</td><td></td><td></td></tr>
              <tr><td></td><td></td><td></td></tr>
              <tr><td>99.99.2026</td><td>25:00-26:00</td><td>Focha</td></tr>
              <tr><td>19.10.2026</td><td>7:00–15:00</td><td></td></tr>
            </tbody></table></figure>
            <h2>Awaryjne wyłączenie wody</h2>
            <figure><table><tbody><tr><td>BRAK</td><td></td><td></td></tr></tbody></table></figure>
        "#;
        let alerts = parse_wordpress_page_json(
            &page_json(content),
            &settings("Bydgoszcz", "Focha", "1", false),
        )
        .unwrap();

        assert!(alerts.is_empty());
    }

    #[test]
    fn rejects_malformed_page_json_and_unpaired_sections() {
        assert!(parse_wordpress_page_json("not json", &Settings::default()).is_err());
        let html = "<h2>Planowane wyłączenie wody</h2>";
        assert!(parse_wordpress_page_json(&page_json(html), &Settings::default()).is_err());
    }

    #[test]
    fn parses_supported_time_ranges_and_rejects_invalid_or_overnight_ranges() {
        assert_eq!(
            parse_time_range("7:00-15:00"),
            Some((
                NaiveTime::from_hms_opt(7, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(15, 0, 0).unwrap()
            ))
        );
        assert_eq!(
            parse_time_range("10:00 – 12:00"),
            Some((
                NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(12, 0, 0).unwrap()
            ))
        );
        assert_eq!(parse_time_range("25:00-26:00"), None);
        assert_eq!(parse_time_range("22:00-02:00"), None);
    }

    #[test]
    fn bydgoszcz_scope_is_applied_when_matching_addresses() {
        let torun = settings("Toruń", "Focha", "1", false);
        assert!(!crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::MwikBydgoszcz,
            &torun.addresses[0]
        ));
        assert!(crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::MwikBydgoszcz,
            &settings("Bydgoszcz", "Focha", "1", false).addresses[0]
        ));
    }

    #[tokio::test]
    #[ignore = "requires a live request to the MWiK Bydgoszcz endpoint"]
    async fn live_page_uses_awaria_network_client() {
        let client = crate::network_state::NetworkState::build_client()
            .expect("Failed to build HTTP client");
        let client_http1 = crate::network_state::NetworkState::build_client_http1()
            .expect("Failed to build HTTP/1.1 client");
        let settings = settings("Bydgoszcz", "Focha", "4", false);

        let (alerts, errors) = MwikBydgoszczProvider
            .fetch(&client, &client_http1, &settings, None)
            .await;

        assert!(
            errors.is_empty(),
            "MWiK Bydgoszcz live fetch failed: {errors:?}"
        );
        assert!(alerts.iter().all(|alert| {
            alert.source == AlertSource::MwikBydgoszcz
                && alert.location.as_deref() == Some("Miejscowość: Bydgoszcz")
        }));
        println!(
            "MWiK Bydgoszcz returned {} parsed alerts: {alerts:#?}",
            alerts.len()
        );
    }
}
