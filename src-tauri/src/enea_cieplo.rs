use crate::api_logic::{
    AlertProvider, AlertSource, Settings, UnifiedAlert, is_address_applicable_for_provider,
};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::OnceLock;

pub const ENEA_CIEPLO_URL: &str = "https://www.enea.pl/strona-korporacyjna/grupa-enea/spolki/enea-cieplo/przerwy-w-dostawie-ciepla";

pub struct EneaCieploProvider;

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d{1,2}\.\d{1,2}\.\d{4}").unwrap())
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(?:godz(?:ina|iny)?\.?\s*)?(\d{1,2}[:.]\d{2})").unwrap())
}

fn parse_time(value: &str) -> Option<(u32, u32)> {
    let (hour, minute) = value.split_once([':', '.'])?;
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    (hour < 24 && minute < 60).then_some((hour, minute))
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_schedule(text: &str) -> (Option<String>, Option<String>) {
    let dates: Vec<_> = date_regex()
        .find_iter(text)
        .filter_map(|matched| NaiveDate::parse_from_str(matched.as_str(), "%d.%m.%Y").ok())
        .collect();
    let Some(start_date) = dates.first().copied() else {
        return (None, None);
    };

    // Strip dates before parsing times so date punctuation (12.10.2026) cannot
    // be mistaken for a time (12.10).
    let without_dates = date_regex().replace_all(text, " ");
    let times: Vec<_> = time_regex()
        .captures_iter(&without_dates)
        .filter_map(|captures| parse_time(&captures[1]))
        .collect();

    let (start_hour, start_minute) = times.first().copied().unwrap_or((0, 0));
    let Some(start) = start_date.and_hms_opt(start_hour, start_minute, 0) else {
        return (None, None);
    };

    let end = times.get(1).and_then(|(hour, minute)| {
        let end_date = dates.get(1).copied().unwrap_or(start_date);
        let mut end = end_date.and_hms_opt(*hour, *minute, 0)?;
        if dates.len() < 2 && end <= start {
            end += Duration::days(1);
        }
        (end > start).then_some(end)
    });

    (Some(format_datetime(start)), end.map(format_datetime))
}

fn normalize_polish(value: &str) -> String {
    value
        .to_lowercase()
        .replace('ą', "a")
        .replace('ć', "c")
        .replace('ę', "e")
        .replace('ł', "l")
        .replace('ń', "n")
        .replace('ó', "o")
        .replace('ś', "s")
        .replace(['ź', 'ż'], "z")
}

fn street_matches_address(address_text: &str, address: &crate::api_logic::AddressEntry) -> bool {
    let mut configured_streets = vec![address.street_name.as_str(), address.street_name_1.as_str()];
    if let Some(street_name_2) = address.street_name_2.as_deref() {
        configured_streets.push(street_name_2);
    }

    let normalized_notice = normalize_polish(address_text);
    let notice_tokens: Vec<_> = normalized_notice
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();

    if configured_streets.into_iter().any(|street| {
        let street = crate::utils::strip_street_prefixes(street.trim());
        let normalized_street = normalize_polish(street);
        let tokens: Vec<_> = normalized_street
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .collect();
        !tokens.is_empty()
            && tokens.iter().all(|token| {
                let stem_length = token.chars().count().saturating_sub(2).max(3);
                let stem: String = token.chars().take(stem_length).collect();
                notice_tokens
                    .iter()
                    .any(|notice_token| notice_token.starts_with(&stem))
            })
    }) {
        return true;
    }

    // Some ENEA notices shorten names such as "M. Curie-Skłodowskiej" to
    // "Skłodowskiej". Match the final part of a hyphenated TERYT street name
    // when the full normalized street did not match.
    let primary_street = crate::utils::strip_street_prefixes(address.street_name_1.trim());
    if primary_street.contains('-') {
        let normalized_primary = normalize_polish(primary_street);
        let primary_parts: Vec<_> = normalized_primary
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| token.chars().count() > 3)
            .collect();
        if let Some(suffix) = primary_parts.last() {
            let stem_length = suffix.chars().count().saturating_sub(2).max(3);
            let stem: String = suffix.chars().take(stem_length).collect();
            return notice_tokens
                .iter()
                .any(|notice_token| notice_token.starts_with(&stem));
        }
    }

    false
}

fn address_text_from_panel(panel: scraper::ElementRef<'_>) -> String {
    let Ok(paragraph_selector) = Selector::parse("p") else {
        return String::new();
    };
    let mut in_address_list = false;
    let mut address_lines = Vec::new();

    for paragraph in panel.select(&paragraph_selector) {
        let line = paragraph
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if line.is_empty() {
            continue;
        }

        let normalized = normalize_polish(&line);
        if normalized.contains("przepraszamy")
            || normalized.contains("pogotowie cieplownicze")
            || normalized.contains("prosimy o poinformowanie")
        {
            in_address_list = false;
            continue;
        }

        if normalized.contains("dot. bud. przy ul.:") {
            let offset = line
                .to_lowercase()
                .find("dot. bud. przy ul.:")
                .map(|index| index + "dot. bud. przy ul.:".len())
                .unwrap_or(0);
            let addresses = line[offset..].trim();
            if !addresses.is_empty() {
                address_lines.push(addresses.to_string());
            }
            in_address_list = true;
            continue;
        }

        if in_address_list && line.chars().any(|character| character.is_ascii_digit()) {
            address_lines.push(line);
        } else {
            in_address_list = false;
        }
    }

    address_lines.join("; ")
}

fn panel_text(panel: scraper::ElementRef<'_>) -> String {
    panel
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn incident_type(text: &str) -> &'static str {
    let normalized = normalize_polish(text);
    if normalized.contains("awari") {
        "Awaria ogrzewania"
    } else if normalized.contains("prac") || normalized.contains("moderniz") {
        "Planowane wyłączenie ogrzewania"
    } else {
        "Zakłócenia w dostawie ciepła"
    }
}

pub fn parse_enea_cieplo_html(html: &str, settings: &Settings) -> Vec<UnifiedAlert> {
    let document = Html::parse_document(html);
    let Ok(panel_selector) = Selector::parse("main [tabs-target]") else {
        return Vec::new();
    };
    let active_addresses: Vec<_> = settings
        .addresses
        .iter()
        .enumerate()
        .filter(|(_, address)| {
            address.is_active
                && is_address_applicable_for_provider(&AlertSource::EneaCieplo, address)
        })
        .collect();

    document
        .select(&panel_selector)
        .filter_map(|panel| {
            let text = panel_text(panel);
            if text.is_empty() {
                return None;
            }

            let normalized = normalize_polish(&text);
            if !normalized.contains("wstrzymana dostawa ciepla")
                && !normalized.contains("przerwa w dostawie ciepla")
            {
                return None;
            }

            let address_text = address_text_from_panel(panel);
            let local_match = active_addresses.iter().find(|(_, address)| {
                if address.street_name.trim().is_empty()
                    && address.street_name_1.trim().is_empty()
                    && address
                        .street_name_2
                        .as_deref()
                        .unwrap_or_default()
                        .trim()
                        .is_empty()
                {
                    true
                } else {
                    street_matches_address(&address_text, address)
                }
            });
            let (address_index, is_local) = match local_match {
                Some((index, _)) => (Some(*index), Some(true)),
                None => (None, Some(false)),
            };

            let kind = incident_type(&text);
            let details = text.split("Przepraszamy").next().unwrap_or(&text).trim();
            let message = format!("{kind} — {details}");
            let (start_date, end_date) = parse_schedule(&text);

            Some(UnifiedAlert {
                source: AlertSource::EneaCieplo,
                startDate: start_date,
                endDate: end_date,
                message: Some(message),
                location: Some("Miejscowość: Białystok".to_string()),
                address_index,
                is_local,
                hash: None,
            })
        })
        .collect()
}

#[async_trait]
impl AlertProvider for EneaCieploProvider {
    fn id(&self) -> String {
        "enea_cieplo".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::EneaCieplo
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
            .get(ENEA_CIEPLO_URL)
            .header("User-Agent", "Mozilla/5.0 (compatible; Awaria/1.0)")
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("ENEA Ciepło request error: {error}")],
                );
            }
        };
        if !response.status().is_success() {
            return (
                Vec::new(),
                vec![format!("ENEA Ciepło HTTP error: {}", response.status())],
            );
        }
        let html = match response.text().await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("ENEA Ciepło response error: {error}")],
                );
            }
        };

        (parse_enea_cieplo_html(&html, settings), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_logic::AddressEntry;

    fn settings(filter_by_house_no: bool) -> Settings {
        Settings {
            filter_by_house_no,
            addresses: vec![AddressEntry {
                name: "Home".to_string(),
                city_name: "Białystok".to_string(),
                voivodeship: "Podlaskie".to_string(),
                district: "Białystok".to_string(),
                commune: "Białystok".to_string(),
                street_name: "ul. Marii Curie-Skłodowskiej".to_string(),
                street_name_1: "Curie-Skłodowskiej".to_string(),
                street_name_2: Some("M.".to_string()),
                house_no: "9A".to_string(),
                city_id: None,
                street_id: None,
                is_active: true,
            }],
            ..Settings::default()
        }
    }

    const ACTIVE_NOTICE: &str = r#"
        <main>
          <ul class="tabs"><li><button data-target="notice">Białystok 12.10.2026</button></li></ul>
          <div tabs-target="notice">
            <div class="heading-weight">
              <p>Enea Ciepło Sp. z o.o. w Białymstoku informuje, że w związku z awarią na sieci ciepłowniczej w dniu 12.10.2026 r. od godz. 08:00 do godz.18.00 zostanie wstrzymana dostawa ciepła do n/w budynków:</p>
              <ul>
                <li>LM NIERUCHOMOŚCI</li>
                <li><p>dot. bud. przy ul.: Skłodowskiej 9A, 11;</p></li>
                <li>ADM POŁUDNIE</li>
                <li><p>dot. bud. przy ul.: Skłodowskiej 13;</p></li>
                <p>Waszyngtona 9, 11, 9/11;</p>
              </ul>
              <p>Przepraszamy za wynikłe z tego powodu utrudnienia.</p>
              <p>Kontakt: Pogotowie Ciepłownicze tel. 993.</p>
            </div>
          </div>
        </main>
    "#;

    const PLANNED_NOTICE: &str = r#"
        <main><div tabs-target="planned"><div class="heading-weight">
          <p>Enea Ciepło Sp. z o.o. w Białymstoku informuje, że w związku z pracami na sieci ciepłowniczej w dniach od 14.09.2026 r. godz. 8.00 do 15.09.2026 r. godz. 15.00 zostanie wstrzymana dostawa ciepła:</p>
          <p>dot. bud. przy ul.: Waszyngtona 1, 6;</p>
          <p>Przepraszamy za utrudnienia.</p>
        </div></div></main>
    "#;

    #[test]
    fn parses_failure_notice_and_matches_street_and_house() {
        let alerts = parse_enea_cieplo_html(ACTIVE_NOTICE, &settings(false));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].source, AlertSource::EneaCieplo);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-12T08:00:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-10-12T18:00:00"));
        assert_eq!(alerts[0].address_index, Some(0));
        assert_eq!(alerts[0].is_local, Some(true));
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .contains("Skłodowskiej 9A, 11")
        );
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .contains("Waszyngtona 9, 11, 9/11")
        );
        assert!(
            !alerts[0]
                .message
                .as_deref()
                .unwrap()
                .contains("Pogotowie Ciepłownicze")
        );
    }

    #[test]
    fn matches_a_street_on_a_continuation_line() {
        let mut settings = settings(false);
        settings.addresses[0].street_name = "Waszyngtona".to_string();
        settings.addresses[0].street_name_1 = "Waszyngtona".to_string();

        let alerts = parse_enea_cieplo_html(ACTIVE_NOTICE, &settings);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].address_index, Some(0));
        assert_eq!(alerts[0].is_local, Some(true));
    }

    #[test]
    fn parses_planned_multi_day_notice_and_continuation_streets() {
        let (start, end) =
            parse_schedule("od 14.09.2026 r. godz. 8.00 do 15.09.2026 r. godz. 15.00");
        assert_eq!(start.as_deref(), Some("2026-09-14T08:00:00"));
        assert_eq!(end.as_deref(), Some("2026-09-15T15:00:00"));

        let alert = &parse_enea_cieplo_html(PLANNED_NOTICE, &settings(false))[0];
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .starts_with("Planowane wyłączenie ogrzewania")
        );
        assert_eq!(alert.is_local, Some(false));
    }

    #[test]
    fn house_number_filtering_remains_in_the_shared_settings_filter() {
        let without_filter = parse_enea_cieplo_html(ACTIVE_NOTICE, &settings(false));
        let with_filter = parse_enea_cieplo_html(ACTIVE_NOTICE, &settings(true));
        assert_eq!(without_filter.len(), 1);
        assert_eq!(with_filter.len(), 1);
        assert_eq!(
            without_filter[0].address_index,
            with_filter[0].address_index
        );
        assert_eq!(without_filter[0].is_local, with_filter[0].is_local);
    }

    #[test]
    fn no_tabs_or_non_outage_panel_returns_no_alerts() {
        assert!(
            parse_enea_cieplo_html("<main><p>Aktualności</p></main>", &settings(false)).is_empty()
        );
        assert!(
            parse_enea_cieplo_html(
                "<main><div tabs-target='x'><p>Brak przerw</p></div></main>",
                &settings(false)
            )
            .is_empty()
        );
    }

    #[test]
    fn parses_multiple_notice_panels() {
        let html = format!("{ACTIVE_NOTICE}{PLANNED_NOTICE}");
        let alerts = parse_enea_cieplo_html(&html, &settings(false));
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-12T08:00:00"));
        assert_eq!(alerts[1].startDate.as_deref(), Some("2026-09-14T08:00:00"));
    }

    #[test]
    fn malformed_or_incomplete_schedule_keeps_unknown_values() {
        assert_eq!(
            parse_schedule("prace zaplanowano na 99.99.2026"),
            (None, None)
        );
        let (start, end) = parse_schedule("w dniu 22.10.2026 r. od godz. 08:00");
        assert_eq!(start.as_deref(), Some("2026-10-22T08:00:00"));
        assert_eq!(end, None);
    }
}
