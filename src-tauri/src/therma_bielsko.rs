use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

pub const THERMA_BIELSKO_URL: &str = "https://www.therma.bielsko.pl/przerwy-w-dostawie";

pub struct ThermaBielskoProvider;

fn date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{1,2})\s*\.\s*(\d{1,2})\s*\.\s*(20\d{2})\b")
            .expect("valid Therma date regex")
    })
}

fn time_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:od\s*)?(?:godz(?:ina)?\.?\s*)?(\d{1,2})\s*:\s*(\d{2})\s*(?:do|[-–—])\s*(?:godz(?:ina)?\.?\s*)?(\d{1,2})\s*:\s*(\d{2})",
        )
        .expect("valid Therma time range regex")
    })
}

fn time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{1,2})\s*:\s*(\d{2})\b").expect("valid Therma time regex")
    })
}

fn normalize_whitespace(value: &str) -> String {
    value
        .replace(['\u{00a0}', '\u{202f}'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

fn element_text(element: ElementRef<'_>) -> String {
    normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn date_from_text(text: &str) -> Option<NaiveDate> {
    let captures = date_regex().captures(text)?;
    NaiveDate::from_ymd_opt(
        captures.get(3)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        captures.get(1)?.as_str().parse().ok()?,
    )
}

#[derive(Clone, Copy)]
struct ClockValue {
    time: NaiveTime,
    next_day: bool,
}

fn parse_clock(hour: &str, minute: &str) -> Option<ClockValue> {
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some(ClockValue {
            time: NaiveTime::MIN,
            next_day: true,
        });
    }

    Some(ClockValue {
        time: NaiveTime::from_hms_opt(hour, minute, 0)?,
        next_day: false,
    })
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_schedule(text: &str) -> Option<(String, Option<String>)> {
    let date = date_from_text(text)?;
    let range = time_range_regex().captures(text);
    let (start, end) = if let Some(captures) = range {
        let start_clock = parse_clock(&captures[1], &captures[2])?;
        let end_clock = parse_clock(&captures[3], &captures[4])?;
        let start_date = if start_clock.next_day {
            date.succ_opt()?
        } else {
            date
        };
        let start = start_date.and_time(start_clock.time);
        let mut end_date = date;
        if start_clock.next_day {
            end_date = end_date.succ_opt()?;
        }
        if end_clock.next_day {
            end_date = end_date.succ_opt()?;
        }
        let mut end = end_date.and_time(end_clock.time);
        if end <= start && !end_clock.next_day {
            end = end.checked_add_signed(Duration::days(1))?;
        }
        (start, Some(end))
    } else {
        let clocks = time_regex().captures_iter(text).collect::<Vec<_>>();
        let start = if let Some(captures) = clocks.first() {
            let clock = parse_clock(&captures[1], &captures[2])?;
            let start_date = if clock.next_day {
                date.succ_opt()?
            } else {
                date
            };
            start_date.and_time(clock.time)
        } else {
            date.and_time(NaiveTime::MIN)
        };
        (start, None)
    };

    Some((format_datetime(start), end.map(format_datetime)))
}

fn is_notice_heading(text: &str) -> bool {
    let normalized = normalize_polish(text);
    normalized.contains("w dniu")
        && date_regex().is_match(text)
        && (normalized.contains("przerw") || normalized.contains("dostaw"))
}

fn block_texts(root: ElementRef<'_>) -> Vec<String> {
    let Ok(block_selector) = Selector::parse("p, li") else {
        return Vec::new();
    };
    let Ok(paragraph_selector) = Selector::parse("p") else {
        return Vec::new();
    };

    root.select(&block_selector)
        .filter(|element| {
            // A list item containing paragraphs would otherwise repeat all of its text.
            element.value().name() != "li" || element.select(&paragraph_selector).next().is_none()
        })
        .map(element_text)
        .filter(|text| !text.is_empty())
        .collect()
}

fn notice_content_root(document: &Html) -> Option<ElementRef<'_>> {
    let selectors = [
        ".entry-content",
        ".post-content",
        ".wp-block-post-content",
        "main",
        "article",
        "#content",
    ];
    let mut candidates = Vec::new();
    for selector_text in selectors {
        let Ok(selector) = Selector::parse(selector_text) else {
            continue;
        };
        candidates.extend(document.select(&selector));
    }

    let mut scored = candidates
        .into_iter()
        .filter_map(|element| {
            let text = element.text().collect::<Vec<_>>().join(" ");
            let heading_count = text
                .match_indices("W dniu")
                .filter(|(index, _)| date_regex().is_match(&text[*index..]))
                .count();
            (heading_count > 0).then_some((heading_count, text.len(), element))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|(count, length, _)| (std::cmp::Reverse(*count), *length));
    if let Some((_, _, root)) = scored.into_iter().next() {
        return Some(root);
    }

    let body_selector = Selector::parse("body").ok()?;
    document.select(&body_selector).next()
}

#[derive(Default)]
struct NoticeDraft {
    heading: String,
    details: Vec<String>,
}

fn draft_notices(blocks: &[String]) -> Vec<NoticeDraft> {
    let mut notices = Vec::new();
    let mut current: Option<NoticeDraft> = None;
    for block in blocks {
        if is_notice_heading(block) {
            if let Some(previous) = current.take() {
                notices.push(previous);
            }
            current = Some(NoticeDraft {
                heading: block.clone(),
                details: Vec::new(),
            });
        } else if let Some(notice) = current.as_mut() {
            notice.details.push(block.clone());
        }
    }
    if let Some(notice) = current {
        notices.push(notice);
    }
    notices
}

fn street_token_matches(expected: &str, found: &str) -> bool {
    if expected.chars().count() <= 3 {
        return expected == found;
    }
    expected == found || expected.starts_with(found) || found.starts_with(expected)
}

fn street_matches_address(notice_text: &str, address: &AddressEntry) -> bool {
    let notice_tokens = normalize_polish(notice_text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let streets = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );

    streets.into_iter().any(|street| {
        let street_tokens = normalize_polish(crate::utils::strip_street_prefixes(street.trim()))
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| !token.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        !street_tokens.is_empty()
            && street_tokens.iter().all(|expected| {
                notice_tokens
                    .iter()
                    .any(|found| street_token_matches(expected, found))
            })
    })
}

fn matching_address_index(notice_text: &str, settings: &Settings) -> Option<usize> {
    settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::ThermaBielsko,
                    address,
                )
                && street_matches_address(notice_text, address)
        })
        .map(|(index, _)| index)
}

fn notice_to_alert(notice: NoticeDraft, settings: &Settings) -> Option<UnifiedAlert> {
    let (start, end) = parse_schedule(&notice.heading)?;
    let place_details = normalize_whitespace(&notice.details.join("; "));
    let address_index = matching_address_index(&place_details, settings);
    let location = if place_details.is_empty() {
        "Miejscowość: Bielsko-Biała".to_string()
    } else {
        format!("Miejscowość: Bielsko-Biała; {place_details}")
    };
    let message = if place_details.is_empty() {
        "Planowana przerwa w dostawie ciepła.".to_string()
    } else {
        format!("Planowana przerwa w dostawie ciepła. {place_details}")
    };
    let mut alert = UnifiedAlert {
        source: AlertSource::ThermaBielsko,
        startDate: Some(start),
        endDate: end,
        message: Some(message),
        location: Some(location),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: None,
    };
    alert.hash = Some(alert.to_hash());
    Some(alert)
}

pub fn parse_therma_bielsko_html(
    html: &str,
    settings: &Settings,
) -> Result<Vec<UnifiedAlert>, String> {
    let document = Html::parse_document(html);
    let root = notice_content_root(&document)
        .ok_or_else(|| "Therma Bielsko outage content not found".to_string())?;
    let blocks = block_texts(root);
    let notices = draft_notices(&blocks);
    if notices.is_empty() {
        let page_text =
            normalize_polish(&document.root_element().text().collect::<Vec<_>>().join(" "));
        if page_text.contains("brak przerw") || page_text.contains("nie ma przerw") {
            return Ok(Vec::new());
        }
        return Err("Therma Bielsko outage notices not found".to_string());
    }

    Ok(notices
        .into_iter()
        .filter_map(|notice| notice_to_alert(notice, settings))
        .collect())
}

async fn fetch_page(client: &Client) -> Result<String, String> {
    let response = crate::utils::retry(
        || async {
            let response = client
                .get(THERMA_BIELSKO_URL)
                .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
                .header("Accept-Language", "pl-PL,pl;q=0.9,en;q=0.7")
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
    .await?;
    Ok(response)
}

#[async_trait]
impl AlertProvider for ThermaBielskoProvider {
    fn id(&self) -> String {
        "therma_bielsko".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::ThermaBielsko
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
            Ok(html) => match parse_therma_bielsko_html(&html, settings) {
                Ok(alerts) => (crate::api_logic::deduplicate_alerts(alerts), Vec::new()),
                Err(error) => (Vec::new(), vec![error]),
            },
            Err(error) => (
                Vec::new(),
                vec![format!("Therma Bielsko request error: {error}")],
            ),
        }
    }
}
