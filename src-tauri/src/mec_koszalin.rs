use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

pub const MEC_KOSZALIN_HEAT_URL: &str = "https://meckoszalin.pl/przerwy-w-dostawie-ciepla/";
const MEC_KOSZALIN_FEED_URL: &str = "https://meckoszalin.pl/feed/";
const SITE_HOST: &str = "meckoszalin.pl";
const MAX_NOTICE_LINKS: usize = 20;

pub struct MecKoszalinProvider;

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

fn whitespace(value: &str) -> String {
    value
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn numeric_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})\s*\.\s*(\d{1,2})\s*\.\s*(20\d{2})\b").unwrap())
}

fn event_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\bw\s+dniu\s+(\d{1,2})\s*\.\s*(\d{1,2})\s*\.\s*(20\d{2})\b").unwrap()
    })
}

fn time_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(\d{1,2})\s*[:.]\s*(\d{2})\s*(?:do|[-–—])\s*(?:godz(?:ina|iny|\.)?\s*)?(\d{1,2})\s*[:.]\s*(\d{2})",
        )
        .unwrap()
    })
}

fn cancelled_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:przerwa.{0,50}(?:odwołan|anulowan)|dostaw.{0,40}przywrócon|prace.{0,40}zakończon|awaria.{0,40}usunięt)",
        )
        .unwrap()
    })
}

fn affected_buildings_marker_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:do\s+niżej\s+wymienionych\s+budynków|do\s+wymienionych\s+budynków|do\s+niżej\s+wymienionych\s+odbiorców)\s*:\s*",
        )
        .unwrap()
    })
}

fn apology_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\bprzepraszamy\b").unwrap())
}

fn letter_digit_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"([\p{L}])([0-9])").unwrap())
}

fn parse_date(day: &str, month: &str, year: &str) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)
}

fn parse_clock(hour: &str, minute: &str) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_event_date(text: &str) -> Option<NaiveDate> {
    if let Some(captures) = event_date_regex().captures(text) {
        return parse_date(&captures[1], &captures[2], &captures[3]);
    }

    // If the wording changes but the date remains beside the schedule, use the
    // closest preceding numeric date. Do not treat a publication date at the
    // start of the article as the outage date.
    let schedule = time_range_regex().find(text)?;
    numeric_date_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            (whole.end() <= schedule.start() && schedule.start() - whole.end() <= 160)
                .then(|| parse_date(&captures[1], &captures[2], &captures[3]))
                .flatten()
        })
        .last()
}

fn parse_schedule(text: &str) -> (Option<String>, Option<String>) {
    let Some(date) = parse_event_date(text) else {
        return (None, None);
    };

    let Some(captures) = time_range_regex().captures(text) else {
        return (Some(date.format("%Y-%m-%d").to_string()), None);
    };
    let (Some(start_time), Some(end_time)) = (
        parse_clock(&captures[1], &captures[2]),
        parse_clock(&captures[3], &captures[4]),
    ) else {
        return (Some(date.format("%Y-%m-%d").to_string()), None);
    };

    let start = date.and_time(start_time);
    let mut end = date.and_time(end_time);
    if end <= start {
        end += Duration::days(1);
    }
    (Some(format_datetime(start)), Some(format_datetime(end)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoticeCity {
    Koszalin,
    Sianow,
}

impl NoticeCity {
    fn label(self) -> &'static str {
        match self {
            Self::Koszalin => "Koszalin",
            Self::Sianow => "Sianów",
        }
    }
}

fn notice_city(text: &str) -> NoticeCity {
    let tokens = word_tokens(text);
    // A Sianów outage must be explicitly stated in the notice. The supplied
    // Koszalin page is only a Koszalin fallback, not evidence of a Sianów event.
    if tokens.windows(2).any(|pair| {
        matches!(pair[0].as_str(), "w" | "na" | "terenie" | "miejscowosc")
            && matches!(pair[1].as_str(), "sianow" | "sianowa" | "sianowie")
    }) {
        NoticeCity::Sianow
    } else {
        NoticeCity::Koszalin
    }
}

fn address_city_matches(address: &AddressEntry, city: NoticeCity) -> bool {
    let city_name = normalize_polish(address.city_name.trim());
    let commune = normalize_polish(address.commune.trim());
    match city {
        NoticeCity::Koszalin => {
            city_name.starts_with("koszalin")
                || address.city_id == Some(949_448)
                || (city_name.is_empty() && commune.starts_with("koszalin"))
        }
        NoticeCity::Sianow => {
            city_name.starts_with("sianow") || (city_name.is_empty() && commune == "sianow")
        }
    }
}

fn has_configured_street(address: &AddressEntry) -> bool {
    !address.street_name.trim().is_empty()
        || !address.street_name_1.trim().is_empty()
        || address
            .street_name_2
            .as_deref()
            .is_some_and(|street| !street.trim().is_empty())
}

fn word_tokens(value: &str) -> Vec<String> {
    let normalized = normalize_polish(value);
    let separated = letter_digit_regex().replace_all(&normalized, "$1 $2");
    separated
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn street_matches_address(location_text: &str, address: &AddressEntry) -> bool {
    let notice_tokens = word_tokens(location_text);
    crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    )
    .into_iter()
    .any(|street| {
        let street = crate::utils::strip_street_prefixes(street.trim());
        let street_tokens = word_tokens(street);
        !street_tokens.is_empty()
            && notice_tokens
                .windows(street_tokens.len())
                .any(|candidate| candidate == street_tokens.as_slice())
    })
}

fn affected_buildings(text: &str) -> String {
    let Some(marker) = affected_buildings_marker_regex().find(text) else {
        return String::new();
    };
    let mut affected = text[marker.end()..].trim();
    if let Some(apology) = apology_regex().find(affected) {
        affected = affected[..apology.start()].trim();
    }
    affected
        .trim_end_matches(['.', ';', ' ', '\n', '\r'])
        .to_string()
}

fn is_heat_interruption(text: &str) -> bool {
    if cancelled_regex().is_match(text) {
        return false;
    }
    let normalized = normalize_polish(text);
    let heat_related = ["ciepl", "ogrzew", "czynnika grzewcz", "cieplej wody"]
        .iter()
        .any(|term| normalized.contains(term));
    let interruption_related = ["przerw", "wstrzym", "wylacz", "zaklocen"]
        .iter()
        .any(|term| normalized.contains(term));
    heat_related && interruption_related
}

fn element_lines(element: ElementRef<'_>) -> String {
    let Ok(line_selector) = Selector::parse("h1, h2, h3, h4, p, li") else {
        return whitespace(&element.text().collect::<Vec<_>>().join(" "));
    };
    let lines = element
        .select(&line_selector)
        .map(|part| whitespace(&part.text().collect::<Vec<_>>().join(" ")))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        whitespace(&element.text().collect::<Vec<_>>().join(" "))
    } else {
        lines.join("\n")
    }
}

fn article_text(html: &str) -> Result<String, String> {
    let document = Html::parse_document(html);
    for selector_text in [
        "article",
        "main",
        ".entry-content",
        ".wp-block-post-content",
        ".post-content",
        ".article-content",
        "#content",
    ] {
        let selector = Selector::parse(selector_text).map_err(|error| error.to_string())?;
        if let Some(candidate) = document
            .select(&selector)
            .map(element_lines)
            .filter(|text| is_heat_interruption(text))
            .max_by_key(String::len)
        {
            return Ok(candidate);
        }
    }

    let body_selector = Selector::parse("body").map_err(|error| error.to_string())?;
    let body = document
        .select(&body_selector)
        .next()
        .map(element_lines)
        .unwrap_or_default();
    if is_heat_interruption(&body) {
        Ok(body)
    } else {
        Err("MEC Koszalin page has no heat interruption content".to_string())
    }
}

fn article_title(html: &str) -> String {
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse("article h1, main h1, h1") else {
        return String::new();
    };
    document
        .select(&selector)
        .next()
        .map(|element| whitespace(&element.text().collect::<Vec<_>>().join(" ")))
        .unwrap_or_default()
}

fn parse_article_text(title: &str, text: &str, settings: &Settings) -> Option<UnifiedAlert> {
    let all_text = if title.trim().is_empty() {
        text.to_string()
    } else {
        format!("{title}\n{text}")
    };
    if !is_heat_interruption(&all_text) {
        return None;
    }

    let (start_date, end_date) = parse_schedule(&all_text);
    let affected = affected_buildings(&all_text);
    let city = notice_city(&all_text);
    let source = AlertSource::MecKoszalin;
    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            if !address.is_active
                || !crate::api_logic::is_address_applicable_for_provider(&source, address)
                || !address_city_matches(address, city)
            {
                return false;
            }
            !has_configured_street(address) || street_matches_address(&affected, address)
        })
        .map(|(index, _)| index);

    let location = if affected.is_empty() {
        format!("Miejscowość: {}", city.label())
    } else {
        format!("Miejscowość: {}; {affected}", city.label())
    };

    Some(UnifiedAlert {
        source,
        startDate: start_date,
        endDate: end_date,
        message: Some(format!(
            "Planowana przerwa w dostawie ciepła: {}",
            whitespace(&all_text)
        )),
        location: Some(location),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: None,
    })
}

pub fn parse_article_html(html: &str, settings: &Settings) -> Result<Option<UnifiedAlert>, String> {
    let text = article_text(html)?;
    let title = article_title(html);
    Ok(parse_article_text(&title, &text, settings))
}

fn valid_notice_url(base: &Url, href: &str) -> Option<Url> {
    let url = base.join(href).ok()?;
    let path = url.path().to_lowercase();
    (url.scheme() == "https"
        && url.host_str() == Some(SITE_HOST)
        && path.starts_with("/przerwa-w-dostawie-")
        && !path.ends_with("/przerwy-w-dostawie-ciepla/"))
    .then_some(url)
}

fn notice_urls_from_page(html: &str) -> Result<Vec<Url>, String> {
    let document = Html::parse_document(html);
    let heading_selector = Selector::parse("h1, h2").map_err(|error| error.to_string())?;
    if !document.select(&heading_selector).any(|heading| {
        normalize_polish(&heading.text().collect::<Vec<_>>().join(" "))
            .contains("przerwy w dostawie ciepla")
    }) {
        return Err("MEC Koszalin heat interruption page has no expected heading".to_string());
    }

    let base = Url::parse(MEC_KOSZALIN_HEAT_URL).map_err(|error| error.to_string())?;
    let anchor_selector = Selector::parse("a[href]").map_err(|error| error.to_string())?;
    let mut urls = Vec::new();
    for anchor in document.select(&anchor_selector) {
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Some(url) = valid_notice_url(&base, href) else {
            continue;
        };
        if !urls.contains(&url) {
            urls.push(url);
            if urls.len() >= MAX_NOTICE_LINKS {
                break;
            }
        }
    }
    Ok(urls)
}

fn notice_urls_from_feed(feed: &str) -> Vec<Url> {
    let document = Html::parse_document(feed);
    let Ok(link_selector) = Selector::parse("item link, entry link[href]") else {
        return Vec::new();
    };
    let Ok(base) = Url::parse(MEC_KOSZALIN_FEED_URL) else {
        return Vec::new();
    };
    let mut urls = Vec::new();
    for link in document.select(&link_selector) {
        let href = link.value().attr("href").map(str::to_string).or_else(|| {
            let text = link.text().collect::<Vec<_>>().join("").trim().to_string();
            (!text.is_empty()).then_some(text)
        });
        let Some(href) = href else {
            continue;
        };
        let Some(url) = valid_notice_url(&base, &href) else {
            continue;
        };
        if !urls.contains(&url) {
            urls.push(url);
            if urls.len() >= MAX_NOTICE_LINKS {
                break;
            }
        }
    }
    urls
}

async fn fetch_notice_urls(client: &Client) -> Result<Vec<Url>, String> {
    // WordPress exposes a standard RSS route, but it may contain unrelated
    // site-wide posts. Use it only when it yields links in the dedicated heat
    // interruption URL family; otherwise fall back to the supplied page.
    if let Ok(response) = client
        .get(MEC_KOSZALIN_FEED_URL)
        .header(
            reqwest::header::ACCEPT,
            "application/rss+xml, application/atom+xml, application/xml, text/xml, */*",
        )
        .send()
        .await
        && response.status().is_success()
        && let Ok(feed) = response.text().await
    {
        let urls = notice_urls_from_feed(&feed);
        if !urls.is_empty() {
            return Ok(urls);
        }
    }

    let response = client
        .get(MEC_KOSZALIN_HEAT_URL)
        .send()
        .await
        .map_err(|error| format!("MEC Koszalin list request error: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "MEC Koszalin list HTTP error: {}",
            response.status()
        ));
    }
    let html = response
        .text()
        .await
        .map_err(|error| format!("MEC Koszalin list read error: {error}"))?;
    notice_urls_from_page(&html)
}

#[async_trait]
impl AlertProvider for MecKoszalinProvider {
    fn id(&self) -> String {
        "mec_koszalin".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::MecKoszalin
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

        let urls = match fetch_notice_urls(client).await {
            Ok(urls) => urls,
            Err(error) => return (Vec::new(), vec![error]),
        };

        let mut alerts = Vec::new();
        let mut errors = Vec::new();
        for url in urls {
            let response = match client.get(url.clone()).send().await {
                Ok(response) if response.status().is_success() => response,
                Ok(response) => {
                    errors.push(format!(
                        "MEC Koszalin article HTTP error for {url}: {}",
                        response.status()
                    ));
                    continue;
                }
                Err(error) => {
                    errors.push(format!(
                        "MEC Koszalin article request error for {url}: {error}"
                    ));
                    continue;
                }
            };
            let html = match response.text().await {
                Ok(html) => html,
                Err(error) => {
                    errors.push(format!(
                        "MEC Koszalin article read error for {url}: {error}"
                    ));
                    continue;
                }
            };
            match parse_article_html(&html, settings) {
                Ok(Some(alert)) => alerts.push(alert),
                Ok(None) => {}
                Err(error) => errors.push(format!(
                    "MEC Koszalin article parse error for {url}: {error}"
                )),
            }
        }
        (crate::api_logic::deduplicate_alerts(alerts), errors)
    }
}
