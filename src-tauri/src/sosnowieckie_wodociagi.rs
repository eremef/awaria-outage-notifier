#[cfg(test)]
use crate::api_logic::AddressEntry;
use crate::api_logic::{AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

pub const SOSNOWIECKIE_WODOCIAGI_URL: &str =
    "https://rpwik.sosnowiec.pl/strefa-klienta/awarie-i-przerwy-w-dostawie-wody";
const OPEN_NOTICE_MAX_AGE: Duration = Duration::hours(72);

pub struct SosnowieckieWodociagiProvider;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoticeKind {
    Planned,
    Failure,
    General,
}

impl NoticeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Planned => "Planowane prace wodociągowe",
            Self::Failure => "Awaria wodociągowa",
            Self::General => "Komunikat wodociągowy",
        }
    }
}

fn normalize(text: &str) -> String {
    crate::utils::clean_text(text)
}

fn whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\s*r?\.?").unwrap())
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})[:.](\d{2})\b").unwrap())
}

fn time_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:od\s*)?(\d{1,2})[:.](\d{2})\s*(?:-|–|—|do)\s*(\d{1,2})[:.](\d{2})\b")
            .unwrap()
    })
}

fn street_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(?:ulica|ulicy|ulicach|ul\.)\s*").unwrap())
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    let captures = date_regex().captures(text)?;
    NaiveDate::from_ymd_opt(
        captures.get(3)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        captures.get(1)?.as_str().parse().ok()?,
    )
}

fn parse_clock(text: &str) -> Option<NaiveTime> {
    // Dotted dates such as 27.10.2026 must not be mistaken for a clock value.
    let without_dates = date_regex().replace_all(text, " ");
    let captures = time_regex().captures(&without_dates)?;
    NaiveTime::from_hms_opt(
        captures.get(1)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        0,
    )
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn endpoint(value: &str) -> Option<(NaiveDate, Option<NaiveTime>)> {
    Some((parse_date(value)?, parse_clock(value)))
}

fn parse_schedule(
    start_cell: &str,
    end_cell: &str,
    message: &str,
) -> (Option<String>, Option<String>) {
    let start_value = endpoint(start_cell).or_else(|| endpoint(message));
    let Some((start_date, cell_start_time)) = start_value else {
        return (None, None);
    };

    let body_range = time_range_regex().captures(message).and_then(|captures| {
        Some((
            NaiveTime::from_hms_opt(
                captures.get(1)?.as_str().parse().ok()?,
                captures.get(2)?.as_str().parse().ok()?,
                0,
            )?,
            NaiveTime::from_hms_opt(
                captures.get(3)?.as_str().parse().ok()?,
                captures.get(4)?.as_str().parse().ok()?,
                0,
            )?,
        ))
    });
    let start_time = cell_start_time.or_else(|| body_range.map(|range| range.0));
    let start = start_time
        .map(|time| format_datetime(start_date.and_time(time)))
        .unwrap_or_else(|| start_date.format("%Y-%m-%d").to_string());

    if end_cell.trim() == "-" || end_cell.trim().is_empty() {
        if let Some((_, end_time)) = body_range {
            let mut end = start_date.and_time(end_time);
            if let Some(start_time) = start_time
                && end <= start_date.and_time(start_time)
            {
                end += Duration::days(1);
            }
            return (Some(start), Some(format_datetime(end)));
        }
        return (Some(start), None);
    }

    let explicit_end = endpoint(end_cell);
    let end = if let Some((end_date, end_time)) = explicit_end {
        if let Some(end_time) = end_time {
            Some(format_datetime(end_date.and_time(end_time)))
        } else if let Some((_, body_end)) = body_range {
            Some(format_datetime(end_date.and_time(body_end)))
        } else {
            Some(end_date.format("%Y-%m-%d").to_string())
        }
    } else if let Some((_, end_time)) = body_range {
        let mut end = start_date.and_time(end_time);
        if let Some(start_time) = start_time
            && end <= start_date.and_time(start_time)
        {
            end += Duration::days(1);
        }
        Some(format_datetime(end))
    } else {
        None
    };
    (Some(start), end)
}

fn cell_text(element: ElementRef<'_>) -> String {
    whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn classify(text: &str) -> Option<NoticeKind> {
    let text = normalize(text);
    if [
        "dostawa wody zostala przywrocona",
        "przywrocono dostawe wody",
        "woda zostala przywrocona",
        "awaria zostala usunieta",
        "awaria usunieta",
        "prace zostaly zakonczone",
        "przerwa zostala odwolana",
        "zakonczono prace",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
    {
        return None;
    }
    // The supplied page is specifically for water-supply failures and interruptions.
    // Keep ambiguous rows as a general notice instead of inferring a failure type.
    if !text.contains("wod") && !text.contains("wodociag") {
        return None;
    }
    if text.contains("awari") || text.contains("uszkodzen") || text.contains("peknie") {
        Some(NoticeKind::Failure)
    } else if text.contains("planowan")
        || text.contains("prac")
        || text.contains("remont")
        || text.contains("wymian")
    {
        Some(NoticeKind::Planned)
    } else {
        Some(NoticeKind::General)
    }
}

fn affected_streets(text: &str) -> String {
    let mut streets = Vec::new();
    for marker in street_marker_regex().find_iter(text) {
        let line_end = text[marker.end()..]
            .find(['\n', '\r'])
            .map(|offset| marker.end() + offset)
            .unwrap_or(text.len());
        let mut value = whitespace(&text[marker.start()..line_end]);
        let normalized = normalize(&value);
        let terminators = [
            " z powodu",
            " z uwagi",
            " w zwiazku",
            " za powstale",
            " za zaistniale",
            " telefon kontaktowy",
            " prosimy",
        ];
        if let Some(end) = terminators
            .iter()
            .filter_map(|term| normalized.find(term))
            .min()
        {
            value.truncate(end);
        }
        value = whitespace(&value);
        if !value.is_empty() && !streets.iter().any(|existing| existing == &value) {
            streets.push(value);
        }
    }
    streets.join("; ")
}

fn parse_row(
    message: &str,
    start_cell: &str,
    end_cell: &str,
    settings: &Settings,
    now_local: NaiveDateTime,
) -> Option<UnifiedAlert> {
    let kind = classify(message)?;
    let affected = affected_streets(message);
    let (start_date, end_date) = parse_schedule(start_cell, end_cell, message);
    let source = AlertSource::SosnowieckieWodociagi;
    // Keep city context alongside the streets so inflected names such as
    // "Wileńskiej" match configured "Wileńska" without cross-city matches.
    let location = if affected.is_empty() {
        "Miejscowość: Sosnowiec".to_string()
    } else {
        format!("Miejscowość: Sosnowiec; {affected}")
    };
    let matching_address = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(&source, address)
                && crate::pwik_gliwice::location_matches_address(&location, address)
        })
        .map(|(index, _)| index);
    let alert = UnifiedAlert {
        source,
        startDate: start_date,
        endDate: end_date,
        message: Some(format!("{}: {}", kind.label(), whitespace(message))),
        location: Some(location),
        address_index: matching_address,
        is_local: Some(matching_address.is_some()),
        hash: None,
    };
    if alert.endDate.is_none() && is_open_notice_stale(&alert, now_local) {
        return None;
    }
    Some(alert)
}

pub fn parse_notices_html(html: &str, settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    parse_notices_html_at(
        html,
        settings,
        Utc::now().with_timezone(&Warsaw).naive_local(),
    )
}

fn is_open_notice_stale(alert: &UnifiedAlert, now_local: NaiveDateTime) -> bool {
    let Some(start) = alert.startDate.as_deref() else {
        return false;
    };
    let start = NaiveDateTime::parse_from_str(start, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(start, "%Y-%m-%dT%H:%M"))
        .or_else(|_| {
            NaiveDate::parse_from_str(start, "%Y-%m-%d")
                .map(|date| date.and_hms_opt(0, 0, 0).unwrap())
        });
    start.is_ok_and(|start| now_local > start + OPEN_NOTICE_MAX_AGE)
}

fn parse_notices_html_at(
    html: &str,
    settings: &Settings,
    now_local: NaiveDateTime,
) -> Result<Vec<UnifiedAlert>, String> {
    let document = Html::parse_document(html);
    let table_selector = Selector::parse("table").map_err(|error| error.to_string())?;
    let row_selector = Selector::parse("tr").map_err(|error| error.to_string())?;
    let cell_selector = Selector::parse("th, td").map_err(|error| error.to_string())?;
    let table = document
        .select(&table_selector)
        .find(|table| {
            let text = normalize(&cell_text(*table));
            text.contains("rodzaj zgloszenia")
                && text.contains("data rozpoczecia")
                && text.contains("przewidywana data zakonczenia")
        })
        .ok_or_else(|| "Sosnowieckie Wodociągi outage table was not found".to_string())?;

    let mut alerts = Vec::new();
    for row in table.select(&row_selector) {
        let cells: Vec<_> = row.select(&cell_selector).collect();
        if cells.len() < 3 || cells.iter().any(|cell| cell.value().name() == "th") {
            continue;
        }
        let message = cell_text(cells[0]);
        let start_cell = cell_text(cells[1]);
        let end_cell = cell_text(cells[2]);
        if message.is_empty() {
            continue;
        }
        if let Some(alert) = parse_row(&message, &start_cell, &end_cell, settings, now_local) {
            alerts.push(alert);
        }
    }
    Ok(alerts)
}

#[async_trait]
impl AlertProvider for SosnowieckieWodociagiProvider {
    fn id(&self) -> String {
        "sosnowieckie_wodociagi".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::SosnowieckieWodociagi
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
        let response = match client.get(SOSNOWIECKIE_WODOCIAGI_URL).send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                return (
                    Vec::new(),
                    vec![format!(
                        "Sosnowieckie Wodociągi HTTP error: {}",
                        response.status()
                    )],
                );
            }
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("Sosnowieckie Wodociągi request error: {error}")],
                );
            }
        };
        let html = match response.text().await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!(
                        "Sosnowieckie Wodociągi response read error: {error}"
                    )],
                );
            }
        };
        match parse_notices_html(&html, settings) {
            Ok(alerts) => (alerts, Vec::new()),
            Err(error) => (Vec::new(), vec![error]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
        <html><table>
        <thead><tr><th>Rodzaj zgłoszenia</th><th>Data rozpoczęcia</th><th>Przewidywana data zakończenia</th></tr></thead>
        <tbody>
          <tr><td>Sosnowieckie Wodociągi S.A. zawiadamiają, że w związku z pracami na sieci wodociągowej dnia 27.10.2026 r. nastąpi zmiana kierunku zasilania w wodę do ul. Upadowej, Wileńskiej w godzinach 8:00-15:00 w Sosnowcu. Mogą wystąpić zmiany parametrów dostarczanej wody.</td><td>27.10.2026 r. 08:00</td><td>27.10.2026 r. 15:00</td></tr>
          <tr><td>Sosnowieckie Wodociągi S.A. zawiadamiają mieszkańców budynków przy ul. Leśna 32, 41, 49a oraz 70a w Sosnowcu, że w dniu 26.10.2026 r. rozpoczną się prace związane z remontem sieci wodociągowej.</td><td>26.10.2026 r.</td><td>-</td></tr>
          <tr><td>Sosnowieckie Wodociągi S.A. informują o awarii wodociągowej przy ul. Kukułek 74e w Sosnowcu. Woda zostanie przywrócona po zakończeniu naprawy.</td><td>27.10.2026r. 08:00</td><td>-</td></tr>
        </tbody></table></html>
    "#;

    fn settings(city: &str, street: &str, house_no: &str, filter_by_house_no: bool) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Dom".into(),
                city_name: city.into(),
                street_name: street.into(),
                street_name_1: street.into(),
                house_no: house_no.into(),
                is_active: true,
                ..AddressEntry::default()
            }],
            filter_by_house_no,
            ..Settings::default()
        }
    }

    fn parse_at(html: &str, settings: &Settings) -> Vec<UnifiedAlert> {
        let now = NaiveDate::from_ymd_opt(2026, 10, 27)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        parse_notices_html_at(html, settings, now).unwrap()
    }

    #[test]
    fn parses_table_intervals_and_multiple_affected_streets() {
        let alerts = parse_at(PAGE, &settings("Sosnowiec", "Wileńska", "999", true));
        assert_eq!(alerts.len(), 3);
        assert_eq!(alerts[0].source, AlertSource::SosnowieckieWodociagi);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-27T08:00:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-10-27T15:00:00"));
        assert_eq!(alerts[0].is_local, Some(true));
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .contains("parametrów dostarczanej wody")
        );
        assert!(
            alerts[0]
                .location
                .as_deref()
                .unwrap()
                .contains("Upadowej, Wileńskiej")
        );
        assert_eq!(alerts[1].startDate.as_deref(), Some("2026-10-26"));
        assert_eq!(alerts[1].endDate, None);
        assert!(
            alerts[1]
                .message
                .as_deref()
                .unwrap()
                .contains("Leśna 32, 41, 49a oraz 70a")
        );
    }

    #[test]
    fn preserves_failure_class_but_drops_explicitly_restored_notice() {
        let alerts = parse_at(PAGE, &settings("Sosnowiec", "Kukułek", "1", false));
        assert_eq!(alerts.len(), 3);
        assert!(
            alerts[2]
                .message
                .as_deref()
                .unwrap()
                .starts_with("Awaria wodociągowa:")
        );

        let restored = PAGE.replace(
            "Sosnowieckie Wodociągi S.A. informują o awarii wodociągowej przy ul. Kukułek 74e w Sosnowcu. Woda zostanie przywrócona po zakończeniu naprawy.",
            "Dostawa wody została przywrócona po usunięciu awarii wodociągowej przy ul. Kukułek 74e w Sosnowcu.",
        );
        assert_eq!(parse_at(&restored, &Settings::default()).len(), 2);
    }

    #[test]
    fn street_match_is_not_filtered_by_house_number_setting() {
        let enabled = parse_at(PAGE, &settings("Sosnowiec", "Leśna", "999", true));
        let disabled = parse_at(PAGE, &settings("Sosnowiec", "Leśna", "999", false));
        assert_eq!(enabled[1].is_local, Some(true));
        assert_eq!(disabled[1].is_local, Some(true));
        assert_eq!(enabled[1].address_index, Some(0));
    }

    #[test]
    fn same_street_in_another_city_is_not_local() {
        let alerts = parse_at(PAGE, &settings("Ruda Śląska", "Wileńska", "1", false));
        assert_eq!(alerts[0].is_local, Some(false));
        assert_eq!(alerts[0].address_index, None);
    }

    #[test]
    fn valid_empty_table_is_empty_and_missing_table_is_an_error() {
        let empty = r#"<table><tr><th>Rodzaj zgłoszenia</th><th>Data rozpoczęcia</th><th>Przewidywana data zakończenia</th></tr></table>"#;
        assert!(parse_at(empty, &Settings::default()).is_empty());
        assert!(
            parse_notices_html_at(
                "<html><p>Strona chwilowo niedostępna</p></html>",
                &Settings::default(),
                NaiveDate::from_ymd_opt(2026, 10, 27)
                    .unwrap()
                    .and_hms_opt(9, 0, 0)
                    .unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn open_notices_expire_only_after_72_local_hours() {
        let header = "<table><tr><th>Rodzaj zgłoszenia</th><th>Data rozpoczęcia</th><th>Przewidywana data zakończenia</th></tr>";
        let row = |start: &str| {
            format!(
                "<tr><td>Awaria wodociągowa przy ul. Leśnej, rozpoczęta dnia {start}.</td><td>{start}</td><td>-</td></tr>"
            )
        };
        let html = format!(
            "{header}{}{}{}</table>",
            row("27.10.2026 08:00"), // Older than 72 hours at the fixed check time.
            row("27.10.2026 09:00"), // Exactly 72 hours old; retain at the boundary.
            format!("{}{}", row("28.10.2026 10:00"), row("31.10.2026 10:00"))
        );
        let now = NaiveDate::from_ymd_opt(2026, 10, 30)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        let alerts = parse_notices_html_at(&html, &Settings::default(), now).unwrap();

        assert_eq!(alerts.len(), 3);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-27T09:00:00"));
        assert_eq!(alerts[1].startDate.as_deref(), Some("2026-10-28T10:00:00"));
        assert_eq!(alerts[2].startDate.as_deref(), Some("2026-10-31T10:00:00"));
    }
}
