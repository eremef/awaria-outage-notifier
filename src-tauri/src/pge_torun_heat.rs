use crate::api_logic::{
    AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert,
    is_address_applicable_for_provider,
};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime};
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

pub const PGE_TORUN_HEAT_URL: &str = "https://pgetorun.pl/strefa-klienta/modernizacje";

pub struct PgeTorunHeatProvider;

fn schedule_year_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?is)\bharmonogram\b[\s\S]{0,300}?\b(20\d{2})\s*r?\.?").unwrap()
    })
}

fn date_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?P<start_day>\d{1,2})\s*\.\s*(?P<start_month>\d{1,2})(?:\s*\.\s*(?P<start_year>\d{4}))?\s*[-–—]\s*(?P<end_day>\d{1,2})\s*\.\s*(?P<end_month>\d{1,2})(?:\s*\.\s*(?P<end_year>\d{4}))?",
        )
        .unwrap()
    })
}

fn normalize_whitespace(value: &str) -> String {
    value
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn element_text(element: ElementRef<'_>) -> String {
    normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn schedule_year(text: &str) -> Option<i32> {
    schedule_year_regex()
        .captures(text)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

fn parse_date_range(text: &str, schedule_year: i32) -> Option<(NaiveDate, NaiveDate)> {
    let captures = date_range_regex().captures(text)?;
    let number = |name: &str| captures.name(name)?.as_str().parse::<u32>().ok();
    let start_day = number("start_day")?;
    let start_month = number("start_month")?;
    let start_year = captures
        .name("start_year")
        .and_then(|value| value.as_str().parse::<i32>().ok())
        .unwrap_or(schedule_year);
    let end_day = number("end_day")?;
    let end_month = number("end_month")?;
    let end_year = captures
        .name("end_year")
        .and_then(|value| value.as_str().parse::<i32>().ok())
        .unwrap_or_else(|| {
            if end_month < start_month {
                start_year + 1
            } else {
                start_year
            }
        });

    let start = NaiveDate::from_ymd_opt(start_year, start_month, start_day)?;
    let end = NaiveDate::from_ymd_opt(end_year, end_month, end_day)?;
    (end >= start).then_some((start, end))
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
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

fn street_matches_address(notice_text: &str, address: &AddressEntry) -> bool {
    let notice_tokens = normalize_polish(notice_text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    let configured_streets = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );

    configured_streets.into_iter().any(|street| {
        let street = crate::utils::strip_street_prefixes(street.trim());
        let tokens = normalize_polish(street)
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| {
                token.chars().count() > 2
                    && !matches!(*token, "swietego" | "swietej" | "swiety" | "swieta")
            })
            .map(str::to_string)
            .collect::<Vec<_>>();

        !tokens.is_empty()
            && tokens.iter().all(|token| {
                let stem_length = token.chars().count().saturating_sub(2).max(3);
                let stem = token.chars().take(stem_length).collect::<String>();
                notice_tokens
                    .iter()
                    .any(|notice_token| notice_token.starts_with(&stem))
            })
    })
}

fn matching_address_index(location_rows: &[String], settings: &Settings) -> Option<usize> {
    settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            if !address.is_active
                || !is_address_applicable_for_provider(&AlertSource::PgeTorunHeat, address)
            {
                return false;
            }

            let has_street = !address.street_name.trim().is_empty()
                || !address.street_name_1.trim().is_empty()
                || address
                    .street_name_2
                    .as_deref()
                    .is_some_and(|street| !street.trim().is_empty());
            !has_street
                || location_rows
                    .iter()
                    .any(|row| street_matches_address(row, address))
        })
        .map(|(index, _)| index)
}

pub fn parse_pge_torun_heat_html(
    html: &str,
    settings: &Settings,
) -> Result<Vec<UnifiedAlert>, String> {
    if normalize_polish(html).contains("to dzialanie zostalo odrzucone") {
        return Err("PGE Toruń anti-bot access denied".to_string());
    }

    let document = Html::parse_document(html);
    let content_selector = Selector::parse(".ezrichtext-field").expect("valid selector");
    let content = document
        .select(&content_selector)
        .next()
        .ok_or_else(|| "PGE Toruń schedule content not found".to_string())?;
    let content_text = element_text(content);
    let year = schedule_year(&content_text)
        .ok_or_else(|| "PGE Toruń schedule year not found".to_string())?;

    let table_selector = Selector::parse("table").expect("valid selector");
    let heading_selector = Selector::parse("thead th").expect("valid selector");
    let row_selector = Selector::parse("tbody tr").expect("valid selector");
    let cell_selector = Selector::parse("td").expect("valid selector");
    let mut alerts = Vec::new();

    for table in content.select(&table_selector) {
        let title = table
            .select(&heading_selector)
            .next()
            .map(element_text)
            .unwrap_or_default();
        let rows = table.select(&row_selector).collect::<Vec<_>>();
        let schedule = rows.iter().enumerate().find_map(|(index, row)| {
            let text = element_text(*row);
            let normalized = normalize_polish(&text);
            if !normalized.contains("przerw") || !normalized.contains("ciepl") {
                return None;
            }
            parse_date_range(&text, year).map(|dates| (index, text, dates))
        });
        let Some((schedule_index, schedule_text, (start_date, end_date))) = schedule else {
            continue;
        };

        let start = start_date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| format!("PGE Toruń invalid start date in schedule: {schedule_text}"))?;
        let end = end_date
            .and_hms_opt(23, 59, 0)
            .ok_or_else(|| format!("PGE Toruń invalid end date in schedule: {schedule_text}"))?;

        let location_rows = rows
            .iter()
            .skip(schedule_index + 1)
            .filter_map(|row| {
                let cells = row
                    .select(&cell_selector)
                    .map(element_text)
                    .filter(|cell| !cell.is_empty())
                    .collect::<Vec<_>>();
                (!cells.is_empty()).then(|| cells.join(" "))
            })
            .collect::<Vec<_>>();
        if title.is_empty() || location_rows.is_empty() {
            continue;
        }

        let address_index = matching_address_index(&location_rows, settings);
        alerts.push(UnifiedAlert {
            source: AlertSource::PgeTorunHeat,
            startDate: Some(format_datetime(start)),
            endDate: Some(format_datetime(end)),
            message: Some(format!(
                "Planowane przerwy w dostawie ciepłej wody — {title}. {schedule_text}. Ulice i budynki: {}.",
                location_rows.join("; ")
            )),
            location: Some("Miejscowość: Toruń".to_string()),
            address_index,
            is_local: Some(address_index.is_some()),
            hash: None,
        });
    }

    Ok(crate::api_logic::deduplicate_alerts(alerts))
}

#[async_trait]
impl AlertProvider for PgeTorunHeatProvider {
    fn id(&self) -> String {
        "pge_torun_heat".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PgeTorunHeat
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

        let response = match client.get(PGE_TORUN_HEAT_URL).send().await {
            Ok(response) => response,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("PGE Toruń request error: {error}")],
                );
            }
        };
        if !response.status().is_success() {
            return (
                Vec::new(),
                vec![format!("PGE Toruń HTTP error: {}", response.status())],
            );
        }

        match response.text().await {
            Ok(html) => match parse_pge_torun_heat_html(&html, settings) {
                Ok(alerts) => (alerts, Vec::new()),
                Err(error) => (Vec::new(), vec![format!("PGE Toruń parse error: {error}")]),
            },
            Err(error) => (
                Vec::new(),
                vec![format!("PGE Toruń response read error: {error}")],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
      <html><body><div class="ezrichtext-field">
        <h3>HARMONOGRAM planowanych przerw w dostawach ciepłej wody w związku z pracami w 2026 r.</h3>
        <table>
          <thead><tr><th colspan="2">2. Wymiana armatury w studniach zaworowych</th></tr></thead>
          <tbody>
            <tr><td colspan="2"><u><strong>1.07- 2.07</strong></u> przerwa w dostawie ciepłej wody dla budynków przy ulicy:</td></tr>
            <tr><td><strong>Dąbrowskiego</strong></td><td>1, 4</td></tr>
            <tr><td><strong>Jagiellończyka 2/Piastowska 5</strong></td><td>&nbsp;</td></tr>
          </tbody>
        </table>
        <table>
          <thead><tr><th colspan="2">6. Budowa przyłączy i rozgrupowanie węzła cieplnego</th></tr></thead>
          <tbody>
            <tr><td colspan="2"><strong>28.09-1.10</strong> przerwa w dostawie ciepłej wody dla budynków przy ulicy:</td></tr>
            <tr><td><strong>Kołłątaja</strong></td><td>12-16</td></tr>
            <tr><td><strong>Piekary 14 / Kopernika 37</strong></td><td>&nbsp;</td></tr>
          </tbody>
        </table>
        <p>W przypadku zmian harmonogram zostanie uaktualniony.</p>
        <p>15.09.2026 r.</p>
      </div></body></html>
    "#;

    fn settings(street: &str, house_no: &str, filter_by_house_no: bool) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Home".to_string(),
                city_name: "Toruń".to_string(),
                voivodeship: "Kujawsko-Pomorskie".to_string(),
                street_name: street.to_string(),
                street_name_1: street.to_string(),
                house_no: house_no.to_string(),
                city_id: Some(929448),
                is_active: true,
                ..Default::default()
            }],
            filter_by_house_no,
            ..Settings::default()
        }
    }

    fn alert_containing<'a>(alerts: &'a [UnifiedAlert], text: &str) -> &'a UnifiedAlert {
        alerts
            .iter()
            .find(|alert| alert.message.as_deref().unwrap_or_default().contains(text))
            .unwrap_or_else(|| panic!("missing alert containing {text:?}"))
    }

    #[test]
    fn parses_all_day_planned_heat_interruptions_and_keeps_update_date_separate() {
        let alerts =
            parse_pge_torun_heat_html(FIXTURE, &settings("Dąbrowskiego", "999", false)).unwrap();
        assert_eq!(alerts.len(), 2);

        let july = alert_containing(&alerts, "Wymiana armatury");
        assert_eq!(july.source, AlertSource::PgeTorunHeat);
        assert_eq!(july.startDate.as_deref(), Some("2026-07-01T00:00:00"));
        assert_eq!(july.endDate.as_deref(), Some("2026-07-02T23:59:00"));
        assert_eq!(july.location.as_deref(), Some("Miejscowość: Toruń"));
        assert_eq!(july.is_local, Some(true));
        assert!(
            july.message
                .as_deref()
                .unwrap()
                .contains("Planowane przerwy")
        );
        assert!(
            july.message
                .as_deref()
                .unwrap()
                .contains("Dąbrowskiego 1, 4")
        );
        assert!(!july.message.as_deref().unwrap().contains("15.09.2026"));
    }

    #[test]
    fn parses_month_boundary_ranges_and_compound_street_rows() {
        let alerts =
            parse_pge_torun_heat_html(FIXTURE, &settings("Piastowska", "4", false)).unwrap();
        let october = alert_containing(&alerts, "Budowa przyłączy");
        assert_eq!(october.startDate.as_deref(), Some("2026-09-28T00:00:00"));
        assert_eq!(october.endDate.as_deref(), Some("2026-10-01T23:59:00"));
        assert_eq!(october.is_local, Some(false));

        let compound = alert_containing(&alerts, "Wymiana armatury");
        assert!(
            compound
                .message
                .as_deref()
                .unwrap()
                .contains("Jagiellończyka 2/Piastowska 5")
        );
        assert_eq!(compound.is_local, Some(true));

        let second_street =
            parse_pge_torun_heat_html(FIXTURE, &settings("Kopernika", "37", false)).unwrap();
        assert_eq!(
            alert_containing(&second_street, "Budowa przyłączy").is_local,
            Some(true)
        );
    }

    #[test]
    fn matching_uses_street_and_leaves_house_number_preference_to_shared_filter() {
        let without_filter =
            parse_pge_torun_heat_html(FIXTURE, &settings("Dąbrowskiego", "99", false)).unwrap();
        let with_filter =
            parse_pge_torun_heat_html(FIXTURE, &settings("Dąbrowskiego", "99", true)).unwrap();

        assert_eq!(
            alert_containing(&without_filter, "Wymiana armatury").is_local,
            Some(true)
        );
        assert_eq!(
            alert_containing(&with_filter, "Wymiana armatury").is_local,
            Some(true)
        );
    }

    #[test]
    fn ignores_empty_or_malformed_schedules_and_rejects_non_schedule_pages() {
        let empty = r#"<div class="ezrichtext-field"><h3>HARMONOGRAM przerw w 2026 r.</h3></div>"#;
        assert!(
            parse_pge_torun_heat_html(empty, &settings("Dąbrowskiego", "1", false))
                .unwrap()
                .is_empty()
        );

        let malformed = r#"
          <div class="ezrichtext-field"><h3>HARMONOGRAM przerw w 2026 r.</h3>
            <table><thead><tr><th>Prace</th></tr></thead><tbody>
              <tr><td>Termin do uzgodnienia — przerwa w dostawie ciepłej wody</td></tr>
              <tr><td>Dąbrowskiego</td><td>1</td></tr>
            </tbody></table>
          </div>
        "#;
        assert!(
            parse_pge_torun_heat_html(malformed, &settings("Dąbrowskiego", "1", false))
                .unwrap()
                .is_empty()
        );
        assert!(
            parse_pge_torun_heat_html(
                "<html><body>Access denied</body></html>",
                &Settings::default()
            )
            .is_err()
        );
        let blocked = parse_pge_torun_heat_html(
            "<html><body>To dzialanie zostalo odrzucone</body></html>",
            &Settings::default(),
        )
        .unwrap_err();
        assert_eq!(blocked, "PGE Toruń anti-bot access denied");
    }

    #[tokio::test]
    #[ignore = "requires live access to PGE Toruń modernization schedule"]
    async fn live_page_has_a_parseable_schedule() {
        let client = crate::network_state::NetworkState::build_client().unwrap();
        let response = client.get(PGE_TORUN_HEAT_URL).send().await.unwrap();
        assert!(response.status().is_success());
        let html = response.text().await.unwrap();
        let alerts = parse_pge_torun_heat_html(&html, &settings("Dąbrowskiego", "1", false))
            .unwrap_or_else(|error| {
                let snippet = html.chars().take(500).collect::<String>();
                panic!("{error}; response begins with: {snippet}");
            });
        assert!(!alerts.is_empty());
    }
}
