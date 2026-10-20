use crate::api_logic::{self, AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::{Captures, Regex};
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const TORUNSKIE_WODOCIAGI_URL: &str =
    "https://wodociagi.torun.com.pl/aktualnosci/awarie-wodociagow/314-awarie-i-prace-planowe";

pub struct TorunskieWodociagiProvider;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum NoticeKind {
    WaterFailure,
    PlannedWaterWork,
}

impl NoticeKind {
    fn label(self) -> &'static str {
        match self {
            Self::WaterFailure => "Awaria wodociągowa",
            Self::PlannedWaterWork => "Prace na sieci wodociągowej",
        }
    }
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_polish(value: &str) -> String {
    crate::utils::clean_text(value)
}

fn tokens(value: &str) -> Vec<String> {
    normalize_polish(value)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

fn contains_word_sequence(text: &str, phrase: &str) -> bool {
    let text = tokens(text);
    let phrase = tokens(phrase);
    !phrase.is_empty()
        && text
            .windows(phrase.len())
            .any(|candidate| candidate == phrase.as_slice())
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap())
}

fn time_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(\d{1,2}):(\d{2})\s*(?:-|–|—|do)\s*(\d{1,2}):(\d{2})\b").unwrap()
    })
}

fn single_time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2}):(\d{2})\b").unwrap())
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    date_regex().captures(value).and_then(|captures| {
        NaiveDate::from_ymd_opt(
            captures.get(3)?.as_str().parse().ok()?,
            captures.get(2)?.as_str().parse().ok()?,
            captures.get(1)?.as_str().parse().ok()?,
        )
    })
}

fn parse_time(
    captures: &Captures<'_>,
    hour_group: usize,
    minute_group: usize,
) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(
        captures.get(hour_group)?.as_str().parse().ok()?,
        captures.get(minute_group)?.as_str().parse().ok()?,
        0,
    )
}

fn format_local_time(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M").to_string()
}

fn parse_schedule(
    date: NaiveDate,
    details: &str,
) -> Option<(
    String,
    Option<String>,
    Option<NaiveDateTime>,
    Option<NaiveDateTime>,
)> {
    if let Some(captures) = time_range_regex().captures(details) {
        let start_time = parse_time(&captures, 1, 2)?;
        let end_time = parse_time(&captures, 3, 4)?;
        let start = date.and_time(start_time);
        let mut end = date.and_time(end_time);
        if end <= start {
            end += Duration::days(1);
        }
        return Some((
            format_local_time(start),
            Some(format_local_time(end)),
            Some(start),
            Some(end),
        ));
    }

    if let Some(captures) = single_time_regex().captures(details) {
        let start = date.and_time(parse_time(&captures, 1, 2)?);
        return Some((format_local_time(start), None, Some(start), None));
    }

    // Preserve the published date without fabricating a start or end time.
    Some((date.format("%d-%m-%Y").to_string(), None, None, None))
}

fn element_text(element: ElementRef<'_>) -> String {
    normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn is_address_paragraph(value: &str) -> bool {
    let value = normalize_polish(value);
    [
        "ul.", "ul ", "ulica ", "al.", "al ", "aleja ", "pl.", "pl ", "plac ",
    ]
    .iter()
    .any(|prefix| value.trim_start().starts_with(prefix))
        || [
            ", ul.", ", ul ", ", ulica ", ", al.", ", al ", ", aleja ", ", pl.", ", pl ", ", plac ",
        ]
        .iter()
        .any(|prefix| value.contains(prefix))
}

fn extract_affected_locations(details_cell: ElementRef<'_>, fallback: &str) -> Vec<String> {
    let paragraph_selector = Selector::parse("p").unwrap();
    let paragraphs: Vec<String> = details_cell
        .select(&paragraph_selector)
        .map(element_text)
        .collect();
    let mut affected = Vec::new();
    let mut after_property_intro = false;

    for paragraph in paragraphs {
        let normalized = normalize_polish(&paragraph);
        if normalized.contains("posesji") && normalized.contains(':') {
            after_property_intro = true;
            if let Some((_, trailing)) = paragraph.split_once(':') {
                let trailing = normalize_whitespace(trailing);
                if is_address_paragraph(&trailing) {
                    affected.push(trailing);
                }
            }
            continue;
        }

        if after_property_intro {
            if is_address_paragraph(&paragraph) {
                affected.push(paragraph);
            } else if !paragraph.is_empty() {
                break;
            }
        }
    }

    if affected.is_empty() {
        let fallback = normalize_whitespace(fallback);
        if !fallback.is_empty() && normalize_polish(&fallback) != "brak" {
            affected.push(fallback);
        }
    }

    affected
}

fn summary_details(details_cell: ElementRef<'_>, fallback: &str) -> String {
    let paragraph_selector = Selector::parse("p").unwrap();
    let mut summary = Vec::new();

    for paragraph in details_cell.select(&paragraph_selector).map(element_text) {
        let normalized = normalize_polish(&paragraph);
        if normalized.contains("posesji") && normalized.contains(':') {
            if let Some(intro_index) = paragraph.to_lowercase().find("posesji") {
                let cause = normalize_whitespace(paragraph[..intro_index].trim_end_matches(':'));
                if !cause.is_empty() {
                    summary.push(cause);
                }
            }
            break;
        }
        if !paragraph.is_empty() {
            summary.push(paragraph);
        }
    }

    if summary.is_empty() {
        normalize_whitespace(fallback)
    } else {
        summary.join(" ")
    }
}

fn bulletin_localities(text: &str) -> Vec<(&'static str, &'static str)> {
    const PLACES: [(&str, &str); 6] = [
        ("Toruń", "torun"),
        ("Lubicz", "lubicz"),
        ("Łysomice", "lysomice"),
        ("Obrowo", "obrowo"),
        ("Wielka Nieszawka", "wielka nieszawka"),
        ("Zławieś Wielka", "zlawies wielka"),
    ];
    PLACES
        .iter()
        .copied()
        .filter(|(_, normalized)| contains_word_sequence(text, normalized))
        .collect()
}

fn address_matches_bulletin_locality(address: &AddressEntry, text: &str) -> bool {
    if contains_word_sequence(text, &address.city_name)
        || contains_word_sequence(text, &address.commune)
    {
        return true;
    }

    let places = bulletin_localities(text);
    if places.is_empty() {
        return address.city_id == Some(929448)
            || normalize_polish(&address.city_name).starts_with("torun");
    }
    places.iter().any(|(_, normalized)| {
        contains_word_sequence(&address.city_name, normalized)
            || contains_word_sequence(&address.commune, normalized)
    })
}

fn locality_label(text: &str, settings: &Settings) -> String {
    let mut labels: Vec<String> = bulletin_localities(text)
        .into_iter()
        .map(|(display, _)| display.to_string())
        .collect();
    for address in settings
        .addresses
        .iter()
        .filter(|address| address.is_active)
    {
        for candidate in [&address.city_name, &address.commune] {
            if !candidate.trim().is_empty()
                && contains_word_sequence(text, candidate)
                && !labels
                    .iter()
                    .any(|existing| normalize_polish(existing) == normalize_polish(candidate))
            {
                labels.push(candidate.clone());
            }
        }
    }
    if labels.is_empty() {
        "Toruń".to_string()
    } else {
        labels.join(", ")
    }
}

fn address_street_names(address: &AddressEntry) -> Vec<&str> {
    let mut streets = Vec::new();
    for candidate in [
        Some(address.street_name.as_str()),
        Some(address.street_name_1.as_str()),
        address.street_name_2.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        let candidate = crate::utils::strip_street_prefixes(candidate).trim();
        if !candidate.is_empty() && !streets.contains(&candidate) {
            streets.push(candidate);
        }
    }
    streets
}

fn address_matches_notice(
    address: &AddressEntry,
    locality_text: &str,
    affected_text: &str,
) -> bool {
    address_matches_bulletin_locality(address, locality_text)
        && address_street_names(address)
            .iter()
            .any(|street| contains_word_sequence(affected_text, street))
}

fn parse_notice(
    kind: NoticeKind,
    date_cell: &str,
    address_cell: &str,
    details_cell: ElementRef<'_>,
    settings: &Settings,
    now_local: NaiveDateTime,
) -> Option<UnifiedAlert> {
    let details = element_text(details_cell);
    let date = parse_date(date_cell).or_else(|| parse_date(&details))?;
    let (start_date, end_date, start, end) = parse_schedule(date, &details)?;

    if kind == NoticeKind::PlannedWaterWork {
        if let Some(end) = end {
            if end <= now_local {
                return None;
            }
        } else if date < now_local.date() {
            return None;
        }
    }

    let affected_locations = extract_affected_locations(details_cell, address_cell);
    let affected_text = affected_locations.join("; ");
    let locality_text = format!("{address_cell} {details}");
    let locality_label = locality_label(&locality_text, settings);

    let location = format!("Miejscowość: {locality_label}; {affected_text}");
    let matching_address = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && api_logic::is_address_applicable_for_provider(
                    &AlertSource::TorunskieWodociagi,
                    address,
                )
                && address_matches_notice(address, &locality_text, &affected_text)
        })
        .map(|(index, _)| index);

    let message = format!(
        "{}: {}",
        kind.label(),
        summary_details(details_cell, &details)
    );
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    date.hash(&mut hasher);
    normalize_whitespace(address_cell).hash(&mut hasher);
    normalize_whitespace(&details).hash(&mut hasher);
    start.hash(&mut hasher);
    end.hash(&mut hasher);

    Some(UnifiedAlert {
        source: AlertSource::TorunskieWodociagi,
        startDate: Some(start_date),
        endDate: end_date,
        message: Some(message),
        location: Some(location),
        address_index: matching_address,
        is_local: Some(matching_address.is_some()),
        hash: Some(format!("torunskie_wodociagi_{:x}", hasher.finish())),
    })
}

fn parse_bulletin(
    html: &str,
    settings: &Settings,
    now_local: NaiveDateTime,
) -> Result<Vec<UnifiedAlert>, String> {
    let document = Html::parse_document(html);
    let article_selector = Selector::parse("#article-314").map_err(|error| error.to_string())?;
    let content_selector =
        Selector::parse("[property='text']").map_err(|error| error.to_string())?;
    let scan_selector =
        Selector::parse("h4, p > strong, table").map_err(|error| error.to_string())?;
    let cell_selector = Selector::parse("td").map_err(|error| error.to_string())?;
    let row_selector = Selector::parse("tr").map_err(|error| error.to_string())?;

    let article = document
        .select(&article_selector)
        .next()
        .ok_or_else(|| "Toruńskie Wodociągi article-314 was not found".to_string())?;
    let content = article
        .select(&content_selector)
        .next()
        .ok_or_else(|| "Toruńskie Wodociągi article text was not found".to_string())?;

    let mut current_section = String::new();
    let mut found_water_section = false;
    let mut alerts = Vec::new();

    for element in content.select(&scan_selector) {
        match element.value().name() {
            "h4" | "strong" => {
                current_section = normalize_polish(&element_text(element));
            }
            "table" => {
                let kind = if current_section.contains("awaria wodociagowa") {
                    Some(NoticeKind::WaterFailure)
                } else if current_section.contains("prace na sieci wodociagowej") {
                    Some(NoticeKind::PlannedWaterWork)
                } else {
                    None
                };
                let Some(kind) = kind else {
                    continue;
                };
                found_water_section = true;

                for row in element.select(&row_selector) {
                    let cells: Vec<ElementRef<'_>> = row.select(&cell_selector).collect();
                    if cells.len() < 3 {
                        continue;
                    }
                    let date_cell = element_text(cells[0]);
                    let address_cell = element_text(cells[1]);
                    let details = element_text(cells[2]);
                    if [date_cell.as_str(), address_cell.as_str(), details.as_str()]
                        .iter()
                        .all(|value| value.is_empty() || normalize_polish(value) == "brak")
                    {
                        continue;
                    }
                    if normalize_polish(&details) == "brak" {
                        continue;
                    }

                    if let Some(alert) = parse_notice(
                        kind,
                        &date_cell,
                        &address_cell,
                        cells[2],
                        settings,
                        now_local,
                    ) {
                        alerts.push(alert);
                    }
                }
            }
            _ => {}
        }
    }

    if !found_water_section {
        return Err("Toruńskie Wodociągi water tables were not found".to_string());
    }

    Ok(alerts)
}

#[async_trait]
impl AlertProvider for TorunskieWodociagiProvider {
    fn id(&self) -> String {
        "torunskie_wodociagi".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::TorunskieWodociagi
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        if !api_logic::is_provider_applicable(self.source(), settings) {
            return (Vec::new(), Vec::new());
        }

        let result = crate::utils::retry(
            || async {
                let response = client
                    .get(TORUNSKIE_WODOCIAGI_URL)
                    .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
                    .send()
                    .await
                    .map_err(|error| error.to_string())?;
                if !response.status().is_success() {
                    return Err(format!("HTTP {}", response.status()));
                }
                response.text().await.map_err(|error| error.to_string())
            },
            3,
        )
        .await;

        let html = match result {
            Ok(html) => html,
            Err(error) => {
                let message = format!("Toruńskie Wodociągi fetch error: {error}");
                log::error!("{message}");
                return (Vec::new(), vec![message]);
            }
        };

        let now_local = Utc::now().with_timezone(&Warsaw).naive_local();
        match parse_bulletin(&html, settings, now_local) {
            Ok(alerts) => (alerts, Vec::new()),
            Err(error) => {
                let message = format!("Toruńskie Wodociągi parse error: {error}");
                log::error!("{message}");
                (Vec::new(), vec![message])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
        <article id="article-314">
          <div property="text">
            <h4>Awaria wodociągowa</h4>
            <table><tbody>
              <tr><td>Data wystąpienia</td><td>Adres</td><td>Szczegóły incydentu</td></tr>
              <tr><td>brak</td><td>brak</td><td>brak</td></tr>
            </tbody></table>
            <p><strong>Awaria kanalizacyjna</strong></p>
            <table><tbody>
              <tr><td>Data wystąpienia</td><td>Adres</td><td>Szczegóły incydentu</td></tr>
              <tr><td>22.10.2026</td><td>ul. Inna 1</td><td>Awaria kanalizacyjna</td></tr>
            </tbody></table>
            <h4>Prace na sieci wodociągowej</h4>
            <table><tbody>
              <tr><td>Data wystąpienia</td><td>Adres</td><td>Szczegóły incydentu</td></tr>
              <tr>
                <td>22.10.2026</td>
                <td><p>ul. Batorego 42-44</p></td>
                <td>
                  <p>W związku z przebudową sieci wodociągowej DN 100 w ul. Batorego w dniu 22.10.2026 w godzinach 7:30-10:00 zaplanowano przerwę w dostawie wody do posesji:</p>
                  <p>ul. Kościuszki 55, 57, 61-63</p>
                  <p>ul. Batorego 33, 32, 32A, 34, 37, 37B, 37A, 34A, 36, 38, 39, 40, 41</p>
                  <p>ul. Dworcowa 3c, 3B, 3D</p>
                  <p>ul. Świętopełka 24D, 24E, 28E</p>
                  <p>Za utrudnienia przepraszamy</p>
                </td>
              </tr>
            </tbody></table>
            <h4>Prace inwestycyjne</h4>
            <table><tbody>
              <tr><td>Termin</td><td>Adres</td><td>Szczegóły</td><td>Wykonawca</td></tr>
              <tr><td>Październik 2026</td><td>Toruń, ul. Hallera</td><td>Wymiana przyłączy wodociągowych</td><td>Wykonawca</td></tr>
            </tbody></table>
          </div>
        </article>
    "#;

    fn test_address(city: &str, commune: &str, street: &str, house: &str) -> AddressEntry {
        AddressEntry {
            name: "Home".to_string(),
            city_name: city.to_string(),
            commune: commune.to_string(),
            voivodeship: "Kujawsko-Pomorskie".to_string(),
            street_name: street.to_string(),
            street_name_1: street.to_string(),
            house_no: house.to_string(),
            city_id: if city == "Toruń" { Some(929448) } else { None },
            is_active: true,
            ..Default::default()
        }
    }

    fn test_now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 20)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    #[test]
    fn parses_planned_multi_street_water_interruption_and_uses_affected_properties() {
        let settings = Settings {
            addresses: vec![test_address("Toruń", "Toruń", "Batorego", "42")],
            ..Default::default()
        };
        let alerts = parse_bulletin(FIXTURE, &settings, test_now()).unwrap();

        assert_eq!(
            alerts.len(),
            1,
            "sewer and investment rows must be excluded"
        );
        let alert = &alerts[0];
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-22 07:30"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-22 10:00"));
        assert!(alert.message.as_deref().unwrap().contains("DN 100"));
        assert!(alert.message.as_deref().unwrap().contains("7:30-10:00"));
        assert!(!alert.message.as_deref().unwrap().contains("Kościuszki 55"));
        assert!(alert
            .location
            .as_deref()
            .unwrap()
            .contains("Kościuszki 55, 57, 61-63"));
        assert!(alert
            .location
            .as_deref()
            .unwrap()
            .contains("Świętopełka 24D, 24E, 28E"));
        assert!(!alert
            .location
            .as_deref()
            .unwrap()
            .contains("Batorego 42-44"));
        assert_eq!(alert.is_local, Some(true));
        assert_eq!(alert.address_index, Some(0));
    }

    #[test]
    fn skips_empty_water_tables() {
        let html = r#"
          <article id="article-314"><div property="text">
            <h4>Awaria wodociągowa</h4>
            <table><tbody>
              <tr><td>Data wystąpienia</td><td>Adres</td><td>Szczegóły incydentu</td></tr>
              <tr><td>brak</td><td>brak</td><td>brak</td></tr>
            </tbody></table>
            <h4>Prace na sieci wodociągowej</h4>
            <table><tbody>
              <tr><td>Data wystąpienia</td><td>Adres</td><td>Szczegóły incydentu</td></tr>
              <tr><td>brak</td><td>brak</td><td>brak</td></tr>
            </tbody></table>
          </div></article>
        "#;
        let alerts = parse_bulletin(html, &Settings::default(), test_now()).unwrap();
        assert!(alerts.is_empty());
    }

    #[test]
    fn keeps_date_only_water_failures_without_inventing_an_end() {
        let html = r#"
          <article id="article-314"><div property="text">
            <h4>Awaria wodociągowa</h4>
            <table><tbody>
              <tr><td>Data wystąpienia</td><td>Adres</td><td>Szczegóły incydentu</td></tr>
              <tr><td>21.10.2026</td><td>ul. Kościuszki</td><td>Brak wody na ulicy</td></tr>
            </tbody></table>
          </div></article>
        "#;
        let alerts = parse_bulletin(html, &Settings::default(), test_now()).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate.as_deref(), Some("21-10-2026"));
        assert_eq!(alerts[0].endDate, None);
    }

    #[test]
    fn matches_an_address_in_a_supported_commune_when_the_notice_names_it() {
        let settings = Settings {
            addresses: vec![test_address("Lubicz Dolny", "Lubicz", "Kościuszki", "55")],
            ..Default::default()
        };
        let html = FIXTURE.replace(
            "ul. Kościuszki 55, 57, 61-63",
            "Lubicz, ul. Kościuszki 55, 57, 61-63",
        );
        let alerts = parse_bulletin(&html, &settings, test_now()).unwrap();
        assert_eq!(alerts[0].is_local, Some(true));
        assert_eq!(alerts[0].address_index, Some(0));
    }

    #[test]
    fn matches_and_labels_a_village_when_the_notice_names_it() {
        let settings = Settings {
            addresses: vec![test_address(
                "Przysiek",
                "Zławieś Wielka",
                "Kościuszki",
                "55",
            )],
            ..Default::default()
        };
        let html = FIXTURE.replace(
            "ul. Kościuszki 55, 57, 61-63",
            "Przysiek, ul. Kościuszki 55, 57, 61-63",
        );
        let alerts = parse_bulletin(&html, &settings, test_now()).unwrap();
        assert_eq!(alerts[0].is_local, Some(true));
        assert!(alerts[0].location.as_deref().unwrap().contains("Przysiek"));
    }

    #[test]
    fn central_house_filter_checks_later_addresses_in_a_multi_street_notice() {
        let settings = Settings {
            addresses: vec![
                test_address("Toruń", "Toruń", "Batorego", "42"),
                test_address("Toruń", "Toruń", "Kościuszki", "55"),
            ],
            filter_by_house_no: true,
            ..Default::default()
        };
        let mut alerts = parse_bulletin(FIXTURE, &settings, test_now()).unwrap();

        crate::apply_alert_filtering(&mut alerts, &Some(settings));

        assert_eq!(alerts[0].is_local, Some(true));
        assert_eq!(alerts[0].address_index, Some(1));
    }

    #[test]
    fn central_house_filter_rejects_work_site_house_number() {
        let settings = Settings {
            addresses: vec![test_address("Toruń", "Toruń", "Batorego", "42")],
            filter_by_house_no: true,
            ..Default::default()
        };
        let mut alerts = parse_bulletin(FIXTURE, &settings, test_now()).unwrap();

        crate::apply_alert_filtering(&mut alerts, &Some(settings));

        assert_eq!(alerts[0].is_local, Some(false));
    }

    #[test]
    fn central_house_filter_checks_letter_suffixes_and_ranges() {
        for (street, house, expected) in [
            ("Batorego", "32A", true),
            ("Batorego", "32B", false),
            ("Kościuszki", "62", true),
            ("Kościuszki", "64", false),
        ] {
            let settings = Settings {
                addresses: vec![test_address("Toruń", "Toruń", street, house)],
                filter_by_house_no: true,
                ..Default::default()
            };
            let mut alerts = parse_bulletin(FIXTURE, &settings, test_now()).unwrap();

            crate::apply_alert_filtering(&mut alerts, &Some(settings));

            assert_eq!(
                alerts[0].is_local,
                Some(expected),
                "unexpected result for {street} {house}"
            );
        }
    }

    #[tokio::test]
    #[ignore = "live source connectivity check"]
    async fn live_bulletin_is_fetchable_with_the_app_rustls_client() {
        let client = crate::network_state::NetworkState::build_client().unwrap();
        let response = client
            .get(TORUNSKIE_WODOCIAGI_URL)
            .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "HTTP {}", response.status());
        let html = response.text().await.unwrap();
        let now_local = Utc::now().with_timezone(&Warsaw).naive_local();
        assert!(parse_bulletin(&html, &Settings::default(), now_local).is_ok());
    }
}
