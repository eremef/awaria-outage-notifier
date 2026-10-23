use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const PWIK_GORZOW_PAGE_URL: &str = "https://pwik.gorzow.pl/awarie-i-planowane-wylaczenia.html";

pub struct PwikGorzowProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Notice {
    id: String,
    title: String,
    status: String,
    content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedTime {
    time: NaiveTime,
    next_day: bool,
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap())
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(2[0-4]|[01]?\d):([0-5]\d)\b").unwrap())
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

fn parse_date(text: &str) -> Option<NaiveDate> {
    let captures = date_regex().captures(text)?;
    let day = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let month = captures.get(2)?.as_str().parse::<u32>().ok()?;
    let year = captures.get(3)?.as_str().parse::<i32>().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

fn parse_time(value: &str) -> Option<ParsedTime> {
    let captures = time_regex().captures(value)?;
    let hour = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let minute = captures.get(2)?.as_str().parse::<u32>().ok()?;
    if hour == 24 {
        if minute != 0 {
            return None;
        }
        return Some(ParsedTime {
            time: NaiveTime::MIN,
            next_day: true,
        });
    }
    Some(ParsedTime {
        time: NaiveTime::from_hms_opt(hour, minute, 0)?,
        next_day: false,
    })
}

fn parse_schedule(content: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let dates = date_regex()
        .captures_iter(content)
        .filter_map(|captures| parse_date(captures.get(0)?.as_str()))
        .collect::<Vec<_>>();
    let times = time_regex()
        .captures_iter(content)
        .filter_map(|captures| parse_time(captures.get(0)?.as_str()))
        .collect::<Vec<_>>();

    let (Some(start_date), Some(start_time), Some(end_time)) = (
        dates.first().copied(),
        times.first().copied(),
        times.last().copied(),
    ) else {
        return (None, None);
    };
    if times.len() < 2 {
        return (None, None);
    }

    let mut start_date = start_date;
    if start_time.next_day {
        start_date += Duration::days(1);
    }
    let mut end_date = dates.last().copied().unwrap_or(start_date);
    if end_time.next_day {
        end_date += Duration::days(1);
    }

    let start = start_date.and_time(start_time.time);
    let mut end = end_date.and_time(end_time.time);
    if end <= start && dates.len() == 1 && !end_time.next_day {
        end_date += Duration::days(1);
        end = end_date.and_time(end_time.time);
    }
    (Some(start), Some(end))
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn card_content(element: ElementRef<'_>, paragraph_selector: &Selector) -> String {
    let paragraphs = element
        .select(paragraph_selector)
        .map(element_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();
    if paragraphs.is_empty() {
        element_text(element)
    } else {
        paragraphs.join("\n")
    }
}

fn parse_notice(
    element: ElementRef<'_>,
    header_selector: &Selector,
    title_selector: &Selector,
    status_selector: &Selector,
    content_selector: &Selector,
    paragraph_selector: &Selector,
) -> Result<Notice, String> {
    let id = element
        .value()
        .attr("id")
        .filter(|id| id.starts_with("event-"))
        .unwrap_or_default()
        .to_string();
    let header = element
        .select(header_selector)
        .next()
        .ok_or_else(|| format!("PWiK Gorzów event {id:?} has no header"))?;
    let title = header
        .select(title_selector)
        .next()
        .map(element_text)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("PWiK Gorzów event {id:?} has no title"))?;
    let status = header
        .select(status_selector)
        .next()
        .map(element_text)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("PWiK Gorzów event {id:?} has no status"))?;
    let content = element
        .select(content_selector)
        .next()
        .map(|content| card_content(content, paragraph_selector))
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("PWiK Gorzów event {id:?} has no content"))?;

    Ok(Notice {
        id,
        title,
        status,
        content,
    })
}

fn contains_phrase(text: &str, phrase: &str) -> bool {
    let text_tokens = normalize(text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let phrase_tokens = normalize(phrase)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    !phrase_tokens.is_empty()
        && text_tokens
            .windows(phrase_tokens.len())
            .any(|window| window == phrase_tokens)
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
    if !commune.is_empty() {
        candidates.push(commune.clone());
    }

    match commune.as_str() {
        "bogdaniec" => candidates.push("Bogdańcu".to_string()),
        "deszczno" => candidates.push("Deszcznie".to_string()),
        "klodawa" => candidates.push("Kłodawie".to_string()),
        "lubiszyn" => candidates.push("Lubiszynie".to_string()),
        "santok" => candidates.push("Santoku".to_string()),
        _ => {}
    }

    if address.city_id == Some(935_140)
        || normalize(&address.city_name).starts_with("gorzow wielkopolski")
    {
        candidates.extend([
            "Gorzów".to_string(),
            "Gorzowie".to_string(),
            "Gorzów Wlkp.".to_string(),
            "Gorzowie Wlkp.".to_string(),
            "Gorzowie Wielkopolskim".to_string(),
        ]);
    }
    candidates
}

fn supported_notice_kind(title: &str) -> bool {
    let normalized = normalize(title);
    normalized == "awaria" || normalized.contains("planowane wylaczenie")
}

fn is_no_outages_notice(notice: &Notice) -> bool {
    normalize(&notice.content).contains("brak awarii i wylaczen na sieci wodociagowej")
}

fn notice_identity(notice: &Notice) -> String {
    let mut hasher = DefaultHasher::new();
    notice.id.hash(&mut hasher);
    normalize(&notice.title).hash(&mut hasher);
    normalize(&notice.status).hash(&mut hasher);
    normalize(&notice.content).hash(&mut hasher);
    format!("pwik_gorzow_{:x}", hasher.finish())
}

fn parse_event_card(
    element: ElementRef<'_>,
    header_selector: &Selector,
    title_selector: &Selector,
    status_selector: &Selector,
    content_selector: &Selector,
    paragraph_selector: &Selector,
    settings: &Settings,
) -> Result<Option<UnifiedAlert>, String> {
    let notice = parse_notice(
        element,
        header_selector,
        title_selector,
        status_selector,
        content_selector,
        paragraph_selector,
    )?;
    if normalize(&notice.status) == "naprawiona"
        || is_no_outages_notice(&notice)
        || !supported_notice_kind(&notice.title)
    {
        return Ok(None);
    }

    let (start, end) = parse_schedule(&notice.content);
    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::PwikGorzow,
                    address,
                )
                && location_matches_address(&notice.content, address)
        })
        .map(|(index, _)| index);

    let label = if normalize(&notice.title).contains("planowane wylaczenie") {
        "Planowane wyłączenie"
    } else {
        "Awaria"
    };

    Ok(Some(UnifiedAlert {
        source: AlertSource::PwikGorzow,
        startDate: start.map(format_datetime),
        endDate: end.map(format_datetime),
        message: Some(format!("{label} — {}: {}", notice.status, notice.content)),
        location: Some(notice.content.clone()),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(notice_identity(&notice)),
    }))
}

/// Returns whether a notice names the configured city or commune and one of
/// its configured streets. This prevents same-named streets across the
/// provider's six municipalities from matching the wrong address.
pub fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    if !locality_candidates(address)
        .iter()
        .any(|locality| contains_phrase(location, locality))
    {
        return false;
    }

    crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    )
    .iter()
    .filter(|street| !street.trim().is_empty())
    .any(|street| contains_phrase(location, street))
}

/// Uses the shared house-number matcher. Call this only from the central
/// filtering path when `Settings.filter_by_house_no` is enabled.
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

/// Parses the server-rendered official PWiK outage page into active alerts.
pub fn parse_pwik_gorzow_html(
    html: &str,
    settings: &Settings,
) -> Result<Vec<UnifiedAlert>, String> {
    let document = Html::parse_document(html);
    let page_heading_selector =
        Selector::parse("main h1").map_err(|error| format!("Invalid page selector: {error}"))?;
    let page_heading = document
        .select(&page_heading_selector)
        .next()
        .map(element_text)
        .unwrap_or_default();
    if !normalize(&page_heading).contains("awarie i planowane wylaczenia") {
        return Err("PWiK Gorzów outage page heading was not found".to_string());
    }

    let card_selector = Selector::parse("div[id^='event-']")
        .map_err(|error| format!("Invalid event selector: {error}"))?;
    let header_selector = Selector::parse(".box-header")
        .map_err(|error| format!("Invalid header selector: {error}"))?;
    let title_selector =
        Selector::parse("h5").map_err(|error| format!("Invalid title selector: {error}"))?;
    let status_selector =
        Selector::parse(".pill").map_err(|error| format!("Invalid status selector: {error}"))?;
    let content_selector = Selector::parse(".box-content")
        .map_err(|error| format!("Invalid content selector: {error}"))?;
    let paragraph_selector =
        Selector::parse("p").map_err(|error| format!("Invalid paragraph selector: {error}"))?;

    let mut alerts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for element in document.select(&card_selector) {
        if let Some(alert) = parse_event_card(
            element,
            &header_selector,
            &title_selector,
            &status_selector,
            &content_selector,
            &paragraph_selector,
            settings,
        )? && seen.insert(alert.hash.clone())
        {
            alerts.push(alert);
        }
    }
    Ok(alerts)
}

async fn fetch_page(client: &Client) -> Result<String, String> {
    crate::utils::retry(
        || async {
            let response = client
                .get(PWIK_GORZOW_PAGE_URL)
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
impl AlertProvider for PwikGorzowProvider {
    fn id(&self) -> String {
        "pwik_gorzow".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PwikGorzow
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
            Ok(html) => match parse_pwik_gorzow_html(&html, settings) {
                Ok(alerts) => (alerts, Vec::new()),
                Err(error) => (Vec::new(), vec![error]),
            },
            Err(error) => (
                Vec::new(),
                vec![format!("PWiK Gorzów request error: {error}")],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE_PAGE_FIXTURE: &str = r#"
        <main>
          <h1>Awarie i planowane wyłączenia</h1>
          <div class="box" id="event-19555">
            <div class="box-header"><h5>Awaria</h5><div class="pill">Naprawiona</div></div>
            <div class="box-content"><p><strong>Brak awarii i wyłączeń na sieci wodociągowej.</strong></p></div>
          </div>
          <div class="box" id="event-21770">
            <div class="box-header"><h5>Awaria</h5><div class="pill">NAPRAWIONA</div></div>
            <div class="box-content">
              <p>Adres:<br><strong>Gorzów Wielkopolski</strong><br><strong>- Ul. Osiedlowa nr od 14 do 25 oraz od 31 do 35</strong></p>
              <p>Czas trwania:<br><strong>Od 23.10.2026 godz. 05:30</strong><br><strong>Do 23.10.2026 godz. 14:00</strong></p>
            </div>
          </div>
          <div class="box" id="event-19111">
            <div class="box-header planowane"><h5>Planowane wyłączenie</h5><div class="pill">Planowane</div></div>
            <div class="box-content">
              <p>PWiK Sp. z o.o. informuje: W związku z pracami modernizacyjnymi sieci wodociągowej w Gorzowie Wlkp., w dniu <strong>27.10.2026 r.</strong> od godz. <strong>8:00</strong> do godz. <strong>14:00</strong> nastąpi przerwa w dostawie wody do budynków przy ulicach: <strong>- Aleksandra Fredry 3 A -D, 5 A - D, 7 A - D</strong>, <strong>- Krzysztofa Kamila Baczyńskiego 24.</strong></p>
              <p>Beczkowóz z wodą zostanie podstawiony.</p>
            </div>
          </div>
        </main>
    "#;

    fn address(
        city: &str,
        commune: &str,
        city_id: Option<u64>,
        street: &str,
        house_no: &str,
    ) -> AddressEntry {
        AddressEntry {
            name: "Dom".to_string(),
            city_name: city.to_string(),
            voivodeship: "Lubuskie".to_string(),
            district: String::new(),
            commune: commune.to_string(),
            street_name: street.to_string(),
            street_name_1: street.to_string(),
            street_name_2: None,
            house_no: house_no.to_string(),
            city_id,
            street_id: None,
            is_active: true,
        }
    }

    #[test]
    fn excludes_repaired_and_no_outages_cards_but_keeps_planned_notice() {
        let settings = Settings {
            addresses: vec![address(
                "Gorzów Wielkopolski",
                "Gorzów Wielkopolski",
                Some(935_140),
                "Aleksandra Fredry",
                "3",
            )],
            ..Settings::default()
        };
        let alerts = parse_pwik_gorzow_html(LIVE_PAGE_FIXTURE, &settings).unwrap();

        assert_eq!(alerts.len(), 1);
        let alert = &alerts[0];
        assert_eq!(alert.source, AlertSource::PwikGorzow);
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-27T08:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-27T14:00:00"));
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("Planowane wyłączenie")
        );
        assert!(alert.message.as_deref().unwrap().contains("Beczkowóz"));
        assert_eq!(alert.is_local, Some(true));
    }

    #[test]
    fn repaired_status_suppresses_details_even_when_end_time_is_future() {
        let settings = Settings::default();
        let html = r#"
            <main><h1>Awarie i planowane wyłączenia</h1>
              <div id="event-1"><div class="box-header"><h5>Awaria</h5><span class="pill"> Naprawiona </span></div>
                <div class="box-content"><p>Gorzów Wielkopolski, ul. Polna 1, od 24.10.2026 08:00 do 24.10.2026 12:00</p></div>
              </div>
            </main>
        "#;
        assert!(parse_pwik_gorzow_html(html, &settings).unwrap().is_empty());
    }

    #[test]
    fn keeps_non_repaired_failure_and_parses_same_day_schedule() {
        let settings = Settings {
            addresses: vec![address(
                "Gorzów Wielkopolski",
                "Gorzów Wielkopolski",
                Some(935_140),
                "Osiedlowa",
                "14",
            )],
            ..Settings::default()
        };
        let html = r#"
            <main><h1>Awarie i planowane wyłączenia</h1>
              <div id="event-2"><div class="box-header"><h5>Awaria</h5><span class="pill">W trakcie</span></div>
                <div class="box-content"><p>Adres: Gorzów Wielkopolski - Ul. Osiedlowa nr od 14 do 25 oraz od 31 do 35</p>
                  <p>Czas trwania: Od 23.10.2026 godz. 05:30 Do 23.10.2026 godz. 14:00</p></div>
              </div>
            </main>
        "#;
        let alerts = parse_pwik_gorzow_html(html, &settings).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-23T05:30:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-10-23T14:00:00"));
        assert_eq!(alerts[0].is_local, Some(true));
        assert!(house_number_matches_notice(
            alerts[0].location.as_deref().unwrap(),
            &settings.addresses[0]
        ));
    }

    #[test]
    fn matches_locality_and_street_without_cross_matching_service_cities() {
        let gorzow = address(
            "Gorzów Wielkopolski",
            "Gorzów Wielkopolski",
            Some(935_140),
            "Osiedlowa",
            "14",
        );
        assert!(location_matches_address(
            "Gorzowie Wlkp. - ul. Osiedlowa nr od 14 do 25",
            &gorzow
        ));
        let klodawa = address("Wojcieszyce", "Gmina Kłodawa", Some(93_000), "Polna", "1");
        assert!(location_matches_address(
            "Kłodawie, ul. Polna 1-3",
            &klodawa
        ));
        assert!(!location_matches_address(
            "Santoku, ul. Polna 1-3",
            &klodawa
        ));
        assert!(!location_matches_address(
            "Gorzowie Wlkp., ul. Polna 1-3",
            &klodawa
        ));
    }

    #[test]
    fn rejects_an_unexpected_page_instead_of_silently_returning_no_alerts() {
        assert!(
            parse_pwik_gorzow_html("<main><h1>Unavailable</h1></main>", &Settings::default())
                .is_err()
        );
        assert!(parse_pwik_gorzow_html(
            "<main><h1>Awarie i planowane wyłączenia</h1><div id='event-1'><div class='box-header'></div></div></main>",
            &Settings::default()
        ).is_err());
    }
}
