use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const WPWIK_WALBRZYCH_PAGE_URL: &str = "https://awarie.wodociagi.walbrzych.pl/";

pub struct WpwikWalbrzychProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Notice {
    title: String,
    body: String,
}

fn normalize(text: &str) -> String {
    crate::utils::clean_text(text)
        .replace(['–', '—', '−'], "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn element_text(element: ElementRef<'_>) -> String {
    collapse_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap())
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b([01]?\d|2[0-3])[:.]([0-5]\d)\b").unwrap())
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    let captures = date_regex().captures(value)?;
    let day = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let month = captures.get(2)?.as_str().parse::<u32>().ok()?;
    let year = captures.get(3)?.as_str().parse::<i32>().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

fn parse_time(value: &str) -> Option<NaiveTime> {
    let captures = time_regex().captures(value)?;
    let hour = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let minute = captures.get(2)?.as_str().parse::<u32>().ok()?;
    NaiveTime::from_hms_opt(hour, minute, 0)
}

fn schedule_dates(text: &str) -> Vec<NaiveDate> {
    date_regex()
        .captures_iter(text)
        .filter_map(|captures| parse_date(captures.get(0)?.as_str()))
        .collect()
}

fn schedule_times(text: &str) -> Vec<NaiveTime> {
    // Removing numeric dates first prevents fragments such as `10.20` inside
    // `27.10.2026` from being misread as a time.
    let without_dates = date_regex().replace_all(text, " ");
    time_regex()
        .captures_iter(&without_dates)
        .filter_map(|captures| parse_time(captures.get(0)?.as_str()))
        .collect()
}

fn parse_schedule(text: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let dates = schedule_dates(text);
    let times = schedule_times(text);
    let (Some(first_date), Some(last_date)) = (dates.first().copied(), dates.last().copied())
    else {
        return (None, None);
    };

    let normalized_text = normalize(text);
    match times.as_slice() {
        [] => (None, None),
        [end_time]
            if normalized_text.contains("termin usuniecia")
                || normalized_text.contains("powrotu zasilania")
                || normalized_text.contains("przywrocenia dostaw")
                || normalized_text.contains("do godz") =>
        {
            (None, Some(last_date.and_time(*end_time)))
        }
        [_] => (None, None),
        [start_time, end_time, ..]
            if normalized_text.contains(" od ") && normalized_text.contains(" do ") =>
        {
            let start = first_date.and_time(*start_time);
            let mut end = last_date.and_time(*end_time);
            if end <= start && dates.len() == 1 {
                end += Duration::days(1);
            }
            (Some(start), Some(end))
        }
        _ => (None, None),
    }
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn is_empty_notice(title: &str, body: &str) -> bool {
    let text = normalize(&format!("{title} {body}"));
    text.contains("brak aktualnych awarii")
        || text.contains("brak awarii i wylaczen")
        || text == "awarie brak awarii"
}

fn parse_notice(
    block: ElementRef<'_>,
    title_selector: &Selector,
    paragraph_selector: &Selector,
) -> Result<Option<Notice>, String> {
    let title = block
        .select(title_selector)
        .next()
        .map(element_text)
        .filter(|text| !text.is_empty());

    let Some(title) = title else {
        // The official board may keep its shell block when there are no current
        // notices. A block without a title is not itself an alert.
        return Ok(None);
    };

    let body = block
        .select(paragraph_selector)
        .map(element_text)
        .filter(|text| !text.is_empty())
        .filter(|text| normalize(text) != normalize(&title))
        .collect::<Vec<_>>()
        .join(" ");

    if is_empty_notice(&title, &body) {
        return Ok(None);
    }
    if body.is_empty() {
        return Err(format!("WPWiK Wałbrzych notice {title:?} has no body"));
    }

    Ok(Some(Notice { title, body }))
}

fn notice_hash(notice: &Notice) -> String {
    let mut hasher = DefaultHasher::new();
    normalize(&notice.title).hash(&mut hasher);
    normalize(&notice.body).hash(&mut hasher);
    format!("wpwik_walbrzych_{:x}", hasher.finish())
}

fn name_tokens(value: &str) -> Vec<String> {
    normalize(value)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn token_matches(candidate: &str, notice_token: &str) -> bool {
    candidate == notice_token
        || (candidate.chars().count() >= 8
            && notice_token.starts_with(&candidate.chars().take(7).collect::<String>()))
}

fn phrase_matches(text_tokens: &[String], phrase: &str) -> bool {
    let phrase_tokens = name_tokens(phrase);
    !phrase_tokens.is_empty()
        && text_tokens.windows(phrase_tokens.len()).any(|window| {
            window
                .iter()
                .zip(&phrase_tokens)
                .all(|(notice, candidate)| token_matches(candidate, notice))
        })
}

fn normalized_commune(address: &AddressEntry) -> String {
    let commune = normalize(&address.commune);
    commune
        .strip_prefix("gmina ")
        .or_else(|| commune.strip_prefix("gm "))
        .unwrap_or(&commune)
        .to_string()
}

fn locality_candidates(address: &AddressEntry) -> Vec<String> {
    let mut candidates = Vec::new();
    if !address.city_name.trim().is_empty() {
        candidates.push(address.city_name.clone());
    }
    let commune = normalized_commune(address);
    if !commune.is_empty() && !candidates.iter().any(|item| normalize(item) == commune) {
        candidates.push(commune);
    }
    candidates
}

/// Returns whether the notice names the configured locality and at least one
/// configured street. If no street is configured, locality alone is enough.
/// The shared source-area matcher should be checked first by callers.
pub fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    let tokens = name_tokens(location);
    if !locality_candidates(address)
        .iter()
        .any(|candidate| phrase_matches(&tokens, candidate))
    {
        return false;
    }

    let streets = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    streets.is_empty()
        || streets
            .iter()
            .any(|candidate| phrase_matches(&tokens, candidate))
}

/// Uses the shared house-number matcher. Call this only from central filtering
/// when `Settings.filter_by_house_no` is enabled.
pub fn house_number_matches_notice(location: &str, address: &AddressEntry) -> bool {
    let street = if !address.street_name_1.trim().is_empty() {
        Some(address.street_name_1.as_str())
    } else if !address.street_name.trim().is_empty() {
        Some(address.street_name.as_str())
    } else {
        None
    };
    crate::utils::match_house_number(&address.house_no, location, street)
}

/// Parses the official WPWiK outage board. The page is a simple server-rendered
/// HTML board with one `.block` per current notice.
pub fn parse_wpwik_walbrzych_html(
    html: &str,
    settings: &Settings,
) -> Result<Vec<UnifiedAlert>, String> {
    let document = Html::parse_document(html);
    let page_heading_selector =
        Selector::parse(".block h1").map_err(|error| format!("Invalid page selector: {error}"))?;
    let page_heading = document
        .select(&page_heading_selector)
        .next()
        .map(element_text)
        .unwrap_or_default();
    if normalize(&page_heading) != "awarie" {
        return Err("WPWiK Wałbrzych outage page heading was not found".to_string());
    }

    let block_selector =
        Selector::parse(".block").map_err(|error| format!("Invalid block selector: {error}"))?;
    let title_selector =
        Selector::parse("p.title").map_err(|error| format!("Invalid title selector: {error}"))?;
    let paragraph_selector = Selector::parse(".block > p:not(.title)")
        .map_err(|error| format!("Invalid paragraph selector: {error}"))?;

    let mut alerts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for block in document.select(&block_selector) {
        let Some(notice) = parse_notice(block, &title_selector, &paragraph_selector)? else {
            continue;
        };
        let identity = notice_hash(&notice);
        if !seen.insert(identity.clone()) {
            continue;
        }

        let combined_text = format!("{}; {}", notice.title, notice.body);
        let (start, end) = parse_schedule(&combined_text);
        let address_index = settings
            .addresses
            .iter()
            .enumerate()
            .find(|(_, address)| {
                address.is_active
                    && crate::api_logic::is_address_applicable_for_provider(
                        &AlertSource::WpwikWalbrzych,
                        address,
                    )
                    && location_matches_address(&combined_text, address)
            })
            .map(|(index, _)| index);

        alerts.push(UnifiedAlert {
            source: AlertSource::WpwikWalbrzych,
            startDate: start.map(format_datetime),
            endDate: end.map(format_datetime),
            message: Some(format!("{}: {}", notice.title, notice.body)),
            location: Some(combined_text),
            address_index,
            is_local: Some(address_index.is_some()),
            hash: Some(identity),
        });
    }

    Ok(alerts)
}

async fn fetch_page(client: &Client) -> Result<String, String> {
    crate::utils::retry(
        || async {
            let response = client
                .get(WPWIK_WALBRZYCH_PAGE_URL)
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

#[async_trait]
impl AlertProvider for WpwikWalbrzychProvider {
    fn id(&self) -> String {
        "wpwik_walbrzych".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::WpwikWalbrzych
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

        match fetch_page(client).await {
            Ok(html) => match parse_wpwik_walbrzych_html(&html, settings) {
                Ok(alerts) => (alerts, Vec::new()),
                Err(error) => {
                    let message = format!("WPWiK Wałbrzych parser error: {error}");
                    log::error!("{}", message);
                    (Vec::new(), vec![message])
                }
            },
            Err(error) => {
                let message = format!("WPWiK Wałbrzych fetch error: {error}");
                log::error!("{}", message);
                (Vec::new(), vec![message])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE_NOTICE: &str = r#"
        <!doctype html><html lang="pl-PL"><body>
          <div class="block">
            <h1>Awarie</h1>
            <p class="title">Awaria Gmina Wałbrzych</p>
            <p><p>Wodociągi Wałbrzych informują, że z powodu awarii na sieci wodociągowej w dniu 27.10.2026 roku, nastąpiło wyłączenie wody na ulicy Senatorskiej w Wałbrzychu. Planowany termin usunięcia awarii i powrotu zasilania wody godzina 19.00. Za utrudnienia serdecznie przepraszamy.</p></p>
          </div>
        </body></html>
    "#;

    fn empty_settings() -> Settings {
        Settings::default()
    }

    fn address(city: &str, street: &str, house_no: &str) -> AddressEntry {
        AddressEntry {
            city_name: city.to_string(),
            commune: city.to_string(),
            voivodeship: "Dolnośląskie".to_string(),
            street_name: street.to_string(),
            street_name_1: street.to_string(),
            house_no: house_no.to_string(),
            is_active: true,
            ..AddressEntry::default()
        }
    }

    #[test]
    fn parses_live_notice_without_inventing_start_time() {
        let alerts = parse_wpwik_walbrzych_html(LIVE_NOTICE, &empty_settings()).unwrap();
        assert_eq!(alerts.len(), 1);
        let alert = &alerts[0];
        assert_eq!(alert.startDate, None);
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-27T19:00:00"));
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("ulicy Senatorskiej w Wałbrzychu")
        );
        assert_eq!(alert.is_local, Some(false));
    }

    #[test]
    fn parses_multiple_blocks_and_deduplicates_identical_notices() {
        let html = LIVE_NOTICE.replace(
            "</body>",
            "<div class=\"block\"><h1>Awarie</h1><p class=\"title\">Awaria Gmina Wałbrzych</p><p>Wodociągi Wałbrzych informują, że z powodu awarii na sieci wodociągowej w dniu 27.10.2026 roku, nastąpiło wyłączenie wody na ulicy Senatorskiej w Wałbrzychu. Planowany termin usunięcia awarii i powrotu zasilania wody godzina 19.00. Za utrudnienia serdecznie przepraszamy.</p></div></body>",
        );
        let alerts = parse_wpwik_walbrzych_html(&html, &empty_settings()).unwrap();
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn accepts_a_valid_empty_board() {
        let html = r#"<div class="block"><h1>Awarie</h1></div>"#;
        assert!(
            parse_wpwik_walbrzych_html(html, &empty_settings())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejects_unexpected_markup_and_malformed_notice() {
        assert!(
            parse_wpwik_walbrzych_html("<html><body>Maintenance</body></html>", &empty_settings())
                .is_err()
        );
        let malformed = r#"<div class="block"><h1>Awarie</h1><p class="title">Awaria Gmina Wałbrzych</p></div>"#;
        assert!(parse_wpwik_walbrzych_html(malformed, &empty_settings()).is_err());
    }

    #[test]
    fn date_fragments_are_not_misread_as_times() {
        let (start, end) = parse_schedule("Awaria 27.10.2026 bez podanej godziny");
        assert_eq!((start, end), (None, None));
        let (start, end) = parse_schedule("Awaria 27.10.2026, godzina 19.00");
        assert_eq!((start, end), (None, None));
    }

    #[test]
    fn parses_explicit_same_day_interval_and_crosses_midnight() {
        let (start, end) = parse_schedule("27.10.2026 od 08:30 do 12:45");
        assert_eq!(
            start.unwrap().format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-10-27T08:30:00"
        );
        assert_eq!(
            end.unwrap().format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-10-27T12:45:00"
        );

        let (start, end) = parse_schedule("27.10.2026 od 22:30 do 01:15");
        assert_eq!(
            start.unwrap().format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-10-27T22:30:00"
        );
        assert_eq!(
            end.unwrap().format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-10-28T01:15:00"
        );
    }

    #[test]
    fn local_matching_requires_the_named_locality_and_street() {
        let addr = address("Wałbrzych", "Senatorska", "12");
        let notice = "Awaria Gmina Wałbrzych; woda wyłączona na ulicy Senatorskiej w Wałbrzychu";
        assert!(location_matches_address(notice, &addr));
        assert!(!location_matches_address(
            "Awaria Gmina Szczawno-Zdrój; ul. Senatorska",
            &addr
        ));
        assert!(!location_matches_address(
            "Awaria Gmina Wałbrzych; ul. Wrocławska",
            &addr
        ));
    }

    #[test]
    fn empty_street_configuration_uses_locality_only() {
        let addr = address("Wałbrzych", "", "");
        assert!(location_matches_address("Awaria Gmina Wałbrzych", &addr));
    }

    #[test]
    fn house_number_matching_uses_shared_helper() {
        let addr = address("Wałbrzych", "Senatorska", "12");
        assert!(house_number_matches_notice(
            "Awaria Gmina Wałbrzych; ul. Senatorska",
            &addr
        ));
        assert!(house_number_matches_notice(
            "Awaria Gmina Wałbrzych; ul. Senatorska 12",
            &addr
        ));
    }
}
