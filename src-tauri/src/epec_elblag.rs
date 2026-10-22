use crate::api_logic::{
    AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert,
    is_address_applicable_for_provider,
};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use futures::stream::{self, StreamExt};
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub const EPEC_ELBLAG_ARCHIVE_URL: &str = "https://epec.pl/kategorie-aktualnosci/komunikaty/";
const PUBLICATION_LOOKBACK_DAYS: i64 = 14;
const MAX_CONCURRENT_ARTICLES: usize = 4;

pub struct EpecElblagProvider;

#[derive(Debug, Clone)]
struct NoticeLink {
    title: String,
    excerpt: String,
    url: String,
    published: Option<NaiveDate>,
}

#[derive(Debug)]
struct Article {
    title: String,
    body: String,
}

#[derive(Debug, Clone)]
struct EventDates {
    separate_dates: Vec<NaiveDate>,
    continuous_range: Option<(NaiveDate, NaiveDate)>,
}

#[derive(Debug, Clone, Copy)]
struct EventTimes {
    start: Option<NaiveTime>,
    end: Option<NaiveTime>,
    end_next_day: bool,
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

fn normalize_whitespace(value: &str) -> String {
    value
        .replace(['\u{a0}', '\u{202f}'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn block_break_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?is)<br\s*/?>|</(?:p|div|li|ul|ol|tr|section|article|h[1-6])\s*>").unwrap()
    })
}

fn list_item_start_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)<li\b[^>]*>").unwrap())
}

fn html_tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)<[^>]+>").unwrap())
}

fn archive_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})[./](\d{1,2})[./](20\d{2})\b").unwrap())
}

fn full_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})[./-](\d{1,2})[./-](20\d{2})\b").unwrap())
}

fn partial_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})[./-](\d{1,2})(?:[./-](20\d{2}))?\b").unwrap())
}

fn multi_day_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{1,2})\s+i\s+(\d{1,2})\s+(\d{1,2})\s*[./-]\s*(20\d{2})\b").unwrap()
    })
}

fn short_date_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"\b(\d{1,2})\s*[-–—]\s*(\d{1,2})\s*[./]\s*(\d{1,2})\s*[./]\s*(20\d{2})\b")
            .unwrap()
    })
}

fn full_date_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"\b(\d{1,2})[./-](\d{1,2})[./-](20\d{2})\s*[-–—]\s*(\d{1,2})[./-](\d{1,2})[./-](20\d{2})\b",
        )
        .unwrap()
    })
}

fn named_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|wrzesnia|pazdziernika|listopada|grudnia)\s+(20\d{2})\b",
        )
        .unwrap()
    })
}

fn time_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2})\s*[:.]\s*(\d{2})\s*(?:do|[-–—])\s*(\d{1,2})\s*[:.]\s*(\d{2})\b",
        )
        .unwrap()
    })
}

fn single_time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})\s*[:.]\s*(\d{2})\b").unwrap())
}

fn year_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(20\d{2})\b").unwrap())
}

fn street_prefix_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?iu)\b(?:ul(?:ica)?\.?|aleja|al\.?|plac|pl\.?)\s*").unwrap())
}

fn house_variant_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d+)\s*([a-z])(?:\s*/\s*(\d+)\s*([a-z])?)?|\b(\d+)\s*/\s*(\d+)\s*([a-z])?\b",
        )
        .unwrap()
    })
}

fn suffixed_house_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"(?i)\b(\d+)\s*([a-z]?)\s*[-–—]\s*(\d+)\s*([a-z]?)\b").unwrap())
}

fn month_number(month: &str) -> Option<u32> {
    Some(match month {
        "stycznia" => 1,
        "lutego" => 2,
        "marca" => 3,
        "kwietnia" => 4,
        "maja" => 5,
        "czerwca" => 6,
        "lipca" => 7,
        "sierpnia" => 8,
        "wrzesnia" => 9,
        "pazdziernika" => 10,
        "listopada" => 11,
        "grudnia" => 12,
        _ => return None,
    })
}

fn date_from_parts(day: &str, month: &str, year: &str) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)
}

fn parse_archive_date(value: &str) -> Option<NaiveDate> {
    let captures = archive_date_regex().captures(value)?;
    date_from_parts(
        captures.get(1)?.as_str(),
        captures.get(2)?.as_str(),
        captures.get(3)?.as_str(),
    )
}

fn html_to_text(html: &str) -> String {
    let with_list_markers = list_item_start_regex().replace_all(html, "\n• ");
    let with_breaks = block_break_regex().replace_all(&with_list_markers, "\n");
    let without_tags = html_tag_regex().replace_all(&with_breaks, " ");
    let fragment = Html::parse_fragment(&without_tags);
    let decoded = fragment.root_element().text().collect::<Vec<_>>().join("");

    decoded
        .lines()
        .map(normalize_whitespace)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn element_text(element: ElementRef<'_>) -> String {
    normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn archive_links(html: &str, page_url: &str) -> Vec<NoticeLink> {
    let document = Html::parse_document(html);
    let card_selector = Selector::parse("ul.grid-col-3.cont-y-grey.cont-x-inner li.grid-item")
        .expect("valid EPEC archive selector");
    let link_selector = Selector::parse("a.post-box[href]").expect("valid EPEC link selector");
    let date_selector = Selector::parse(".post-box__date span").expect("valid EPEC date selector");
    let title_selector = Selector::parse("h3.post-box__title").expect("valid EPEC title selector");
    let excerpt_selector =
        Selector::parse(".post-box__excerpt").expect("valid EPEC excerpt selector");
    let page_url = Url::parse(page_url).ok();

    document
        .select(&card_selector)
        .filter_map(|card| {
            let link = card.select(&link_selector).next()?;
            let href = link.value().attr("href")?;
            let url = page_url.as_ref()?.join(href).ok()?.to_string();
            let title = card
                .select(&title_selector)
                .next()
                .map(element_text)
                .unwrap_or_else(|| element_text(link));
            let excerpt = card
                .select(&excerpt_selector)
                .next()
                .map(element_text)
                .unwrap_or_default();
            let published = card
                .select(&date_selector)
                .next()
                .map(element_text)
                .and_then(|date| parse_archive_date(&date));

            Some(NoticeLink {
                title,
                excerpt,
                url,
                published,
            })
        })
        .collect()
}

fn is_outage_candidate(text: &str) -> bool {
    let normalized = normalize_polish(text);
    let has_heat = ["ciepl", "ogrzew", "ciepla woda", "cwu"]
        .iter()
        .any(|keyword| normalized.contains(keyword));
    let has_interruption = ["awari", "przerw", "wstrzym", "wylaczen", "brak dostaw"]
        .iter()
        .any(|keyword| normalized.contains(keyword));
    has_heat && has_interruption
}

fn is_cancelled_or_resolved(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "odwolana",
        "odwolano",
        "nie odbedzie",
        "awaria usunieta",
        "awaria zostala usunieta",
        "przerwa zostala odwolana",
        "dostawy przywrocone",
        "dostawa zostala przywrocona",
        "przywrocono dostawy",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

fn parse_article(html: &str, fallback_title: &str) -> Option<Article> {
    let document = Html::parse_document(html);
    let title_selector = Selector::parse("main h1").expect("valid EPEC article title selector");
    let body_selector =
        Selector::parse("main .intro__text.wp-editor").expect("valid EPEC article body selector");
    let generic_body_selector =
        Selector::parse("main .wp-editor").expect("valid EPEC generic body selector");
    let title = document
        .select(&title_selector)
        .next()
        .map(element_text)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback_title.to_string());
    let body_element = document
        .select(&body_selector)
        .next()
        .or_else(|| document.select(&generic_body_selector).next())?;
    let body = html_to_text(&body_element.inner_html());
    (!body.trim().is_empty()).then_some(Article { title, body })
}

fn parse_event_dates(text: &str) -> EventDates {
    if let Some(captures) = multi_day_date_regex().captures(text) {
        let day_one = captures.get(1).map(|value| value.as_str());
        let day_two = captures.get(2).map(|value| value.as_str());
        let month = captures.get(3).map(|value| value.as_str());
        let year = captures.get(4).map(|value| value.as_str());
        if let (Some(day_one), Some(day_two), Some(month), Some(year)) =
            (day_one, day_two, month, year)
        {
            let dates = [day_one, day_two]
                .into_iter()
                .filter_map(|day| date_from_parts(day, month, year))
                .collect::<Vec<_>>();
            if !dates.is_empty() {
                return EventDates {
                    separate_dates: dates,
                    continuous_range: None,
                };
            }
        }
    }

    if let Some(captures) = full_date_range_regex().captures(text) {
        if let (Some(start), Some(end)) = (
            date_from_parts(&captures[1], &captures[2], &captures[3]),
            date_from_parts(&captures[4], &captures[5], &captures[6]),
        ) {
            if end >= start {
                return EventDates {
                    separate_dates: Vec::new(),
                    continuous_range: Some((start, end)),
                };
            }
        }
    }

    if let Some(captures) = short_date_range_regex().captures(text) {
        let month = captures.get(3).map(|value| value.as_str());
        let year = captures.get(4).map(|value| value.as_str());
        if let (Some(month), Some(year)) = (month, year) {
            if let (Some(start), Some(end)) = (
                date_from_parts(&captures[1], month, year),
                date_from_parts(&captures[2], month, year),
            ) {
                if end >= start {
                    return EventDates {
                        separate_dates: Vec::new(),
                        continuous_range: Some((start, end)),
                    };
                }
            }
        }
    }

    let mut dates = Vec::new();
    for captures in full_date_regex().captures_iter(text) {
        if let Some(date) = date_from_parts(&captures[1], &captures[2], &captures[3]) {
            if !dates.contains(&date) {
                dates.push(date);
            }
        }
    }
    if dates.is_empty() {
        let normalized = normalize_polish(text);
        for captures in named_date_regex().captures_iter(&normalized) {
            let Some(month) = month_number(&captures[2]) else {
                continue;
            };
            if let (Ok(day), Ok(year)) = (captures[1].parse::<u32>(), captures[3].parse::<i32>())
                && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
                && !dates.contains(&date)
            {
                dates.push(date);
            }
        }
    }

    if dates.is_empty() {
        let year = year_regex()
            .captures(text)
            .and_then(|captures| captures.get(1))
            .and_then(|value| value.as_str().parse::<i32>().ok());
        if let Some(year) = year {
            for captures in partial_date_regex().captures_iter(text) {
                let Some(day) = captures
                    .get(1)
                    .and_then(|value| value.as_str().parse::<u32>().ok())
                else {
                    continue;
                };
                let Some(month) = captures
                    .get(2)
                    .and_then(|value| value.as_str().parse::<u32>().ok())
                else {
                    continue;
                };
                if captures.get(3).is_some() {
                    continue;
                }
                if let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
                    && !dates.contains(&date)
                {
                    dates.push(date);
                }
            }
        }
    }

    EventDates {
        separate_dates: dates,
        continuous_range: None,
    }
}

fn parse_event_dates_from_notice(body: &str, title: &str) -> EventDates {
    let dates = parse_event_dates(body);
    if !dates.separate_dates.is_empty() || dates.continuous_range.is_some() {
        dates
    } else {
        parse_event_dates(title)
    }
}

fn parse_clock(hour: &str, minute: &str) -> Option<(NaiveTime, bool)> {
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some((NaiveTime::from_hms_opt(0, 0, 0)?, true));
    }
    Some((NaiveTime::from_hms_opt(hour, minute, 0)?, false))
}

fn parse_event_times(text: &str) -> EventTimes {
    if let Some(captures) = time_range_regex().captures(text) {
        let start = parse_clock(&captures[1], &captures[2]).map(|(time, _)| time);
        let end = parse_clock(&captures[3], &captures[4]);
        return EventTimes {
            start,
            end: end.map(|(time, _)| time),
            end_next_day: end.is_some_and(|(_, next_day)| next_day),
        };
    }

    for captures in single_time_regex().captures_iter(text) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        // A full date such as 14.07.2026 begins with a valid-looking clock
        // token; do not mistake that date for an event time.
        let after_date_token = &text[matched.end()..];
        if let Some(year_with_separator) = after_date_token.strip_prefix('.')
            && year_with_separator
                .get(..4)
                .is_some_and(|year| year.starts_with("20") && year.chars().all(char::is_numeric))
        {
            continue;
        }

        let Some((time, next_day)) = parse_clock(&captures[1], &captures[2]) else {
            continue;
        };
        let before = normalize_polish(&text[..matched.start()]);
        let after = normalize_polish(&text[matched.end()..]);
        if [
            "zakonczenie",
            "koniec prac",
            "do godz",
            "do godziny",
            "przywrocen",
        ]
        .iter()
        .any(|phrase| before.contains(phrase))
        {
            return EventTimes {
                start: None,
                end: Some(time),
                end_next_day: next_day,
            };
        } else if ["od godz", "od godziny", "rozpoczecie", "od dnia"]
            .iter()
            .any(|phrase| before.contains(phrase))
            || after.contains("rozpocznie sie")
        {
            return EventTimes {
                start: Some(time),
                end: None,
                end_next_day: false,
            };
        }
    }

    EventTimes {
        start: None,
        end: None,
        end_next_day: false,
    }
}

fn datetime_string(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn alert_dates(dates: &EventDates, times: EventTimes) -> Vec<(Option<String>, Option<String>)> {
    if let Some((start_date, end_date)) = dates.continuous_range {
        let start = times.start.map(|time| start_date.and_time(time));
        let end_day = if times.end_next_day {
            end_date.succ_opt().unwrap_or(end_date)
        } else {
            end_date
        };
        let end = times.end.map(|time| end_day.and_time(time));
        return vec![(
            start
                .map(datetime_string)
                .or_else(|| Some(start_date.to_string())),
            end.map(datetime_string)
                .or_else(|| Some(end_date.to_string())),
        )];
    }

    if !dates.separate_dates.is_empty() {
        return dates
            .separate_dates
            .iter()
            .map(|date| {
                let start = times
                    .start
                    .map(|time| datetime_string(date.and_time(time)))
                    .or_else(|| (times.end.is_none()).then(|| date.to_string()));
                let end_day = if times.end_next_day {
                    date.succ_opt().unwrap_or(*date)
                } else {
                    *date
                };
                let end = times
                    .end
                    .map(|time| datetime_string(end_day.and_time(time)));
                (start, end)
            })
            .collect();
    }

    vec![(None, None)]
}

fn make_alerts(notice: &NoticeLink, article: Article) -> Vec<UnifiedAlert> {
    if is_cancelled_or_resolved(&format!("{}\n{}", article.title, article.body))
        || (!is_outage_candidate(&article.title) && !is_outage_candidate(&article.body))
    {
        return Vec::new();
    }

    let full_message = if article.title.trim().is_empty() {
        article.body.clone()
    } else {
        format!("{}\n\n{}", article.title, article.body)
    };
    let dates = parse_event_dates_from_notice(&article.body, &article.title);
    let times = parse_event_times(&article.body);
    let intervals = alert_dates(&dates, times);

    intervals
        .into_iter()
        .map(|(start_date, end_date)| {
            let mut hasher = Sha256::new();
            hasher.update(notice.url.as_bytes());
            hasher.update(start_date.as_deref().unwrap_or_default().as_bytes());
            hasher.update(end_date.as_deref().unwrap_or_default().as_bytes());
            hasher.update(full_message.as_bytes());
            let hash = hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            UnifiedAlert {
                source: AlertSource::EpecElblag,
                startDate: start_date,
                endDate: end_date,
                message: Some(full_message.clone()),
                location: Some("Elbląg".to_string()),
                address_index: None,
                is_local: None,
                hash: Some(hash),
            }
        })
        .collect()
}

fn normalized_tokens(value: &str) -> Vec<String> {
    normalize_polish(&crate::utils::clean_text(value))
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn street_token_matches(configured: &str, notice: &str) -> bool {
    if configured == notice {
        return true;
    }
    if configured.chars().all(char::is_numeric) || notice.chars().all(char::is_numeric) {
        return false;
    }
    let shared_prefix_len = configured.chars().count().saturating_sub(2).max(3);
    let configured_prefix = configured
        .chars()
        .take(shared_prefix_len)
        .collect::<String>();
    let notice_prefix = notice
        .chars()
        .take(shared_prefix_len.min(notice.chars().count()))
        .collect::<String>();
    notice.starts_with(&configured_prefix) || configured.starts_with(&notice_prefix)
}

fn street_candidates(address: &AddressEntry) -> Vec<String> {
    crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    )
    .into_iter()
    .map(|street| crate::utils::strip_street_prefixes(street.trim()).to_string())
    .filter(|street| !street.is_empty())
    .collect()
}

fn street_matches_segment(segment: &str, address: &AddressEntry) -> bool {
    street_candidates(address)
        .iter()
        .any(|street| street_candidate_matches_segment(segment, street))
}

fn street_candidate_matches_segment(segment: &str, street: &str) -> bool {
    let notice_tokens = normalized_tokens(segment);
    let configured_tokens = normalized_tokens(street)
        .into_iter()
        .filter(|token| {
            token.chars().all(char::is_numeric)
                || token.chars().count() > 2
                    && !matches!(
                        token.as_str(),
                        "im" | "sw"
                            | "swiety"
                            | "swieta"
                            | "swietego"
                            | "swietej"
                            | "pulkownika"
                            | "plk"
                            | "generala"
                            | "gen"
                            | "marszalka"
                            | "profesora"
                            | "prof"
                            | "ksiedza"
                            | "ks"
                            | "aleja"
                            | "plac"
                    )
        })
        .collect::<Vec<_>>();
    !configured_tokens.is_empty()
        && configured_tokens.iter().all(|configured| {
            notice_tokens
                .iter()
                .any(|notice| street_token_matches(configured, notice))
        })
}

fn location_segments(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let prefixes = street_prefix_regex().find_iter(line).collect::<Vec<_>>();
        if prefixes.is_empty() {
            for segment in line
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                segments.push(segment.to_string());
            }
            continue;
        }

        for (index, prefix) in prefixes.iter().enumerate() {
            let end = prefixes
                .get(index + 1)
                .map(|next| next.start())
                .unwrap_or(line.len());
            let segment = line[prefix.start()..end]
                .trim()
                .trim_matches([',', ';', '•', ':'])
                .to_string();
            if !segment.is_empty() {
                segments.push(segment);
            }
        }
    }
    segments
}

fn notice_text(message: &str, location: &str) -> String {
    format!("{message}\n{location}")
}

fn is_city_wide_notice(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "cale miasto",
        "calym elblagu",
        "calego elblaga",
        "wszyscy odbiorcy",
        "wszyscy klienci",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

pub fn address_matches_notice(message: &str, location: &str, address: &AddressEntry) -> bool {
    if !is_address_applicable_for_provider(&AlertSource::EpecElblag, address) {
        return false;
    }
    let text = notice_text(message, location);
    if is_city_wide_notice(&text) {
        return true;
    }
    location_segments(&text)
        .iter()
        .any(|segment| street_matches_segment(segment, address))
}

pub fn house_number_matches_notice(message: &str, location: &str, address: &AddressEntry) -> bool {
    if address.house_no.trim().is_empty() {
        return true;
    }
    let text = notice_text(message, location);
    if is_city_wide_notice(&text) {
        return true;
    }
    location_segments(&text).iter().any(|segment| {
        street_candidates(address)
            .into_iter()
            .find(|street| street_candidate_matches_segment(segment, street))
            .is_some_and(|street| {
                let number_spec = crate::utils::isolate_street_segment(segment, &street);
                crate::utils::match_house_number(&address.house_no, segment, Some(&street))
                    && house_variant_matches(&address.house_no, &number_spec)
            })
    })
}

fn house_variant_matches(requested: &str, segment: &str) -> bool {
    let requested = normalize_polish(requested)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let requested_digits = requested
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if requested_digits.is_empty() {
        return true;
    }
    let requested_variant = requested[requested_digits.len()..]
        .chars()
        .any(|character| character.is_ascii_alphabetic() || character == '/');
    if !requested_variant {
        return true;
    }

    let exact_variant = house_variant_regex()
        .captures_iter(segment)
        .any(|captures| {
            let matched = captures
                .get(0)
                .map(|value| {
                    normalize_polish(value.as_str())
                        .chars()
                        .filter(|character| !character.is_whitespace())
                        .collect::<String>()
                })
                .unwrap_or_default();
            matched == requested
        });
    if exact_variant {
        return true;
    }

    let requested_number = requested_digits.parse::<u32>().ok();
    let requested_suffix = requested[requested_digits.len()..]
        .chars()
        .take_while(|character| character.is_ascii_alphabetic())
        .collect::<String>();
    if requested_suffix.len() == 1 && !requested.contains('/') {
        let requested_suffix = requested_suffix.chars().next().unwrap_or_default();
        for captures in suffixed_house_range_regex().captures_iter(segment) {
            let (Ok(start_number), Ok(end_number)) =
                (captures[1].parse::<u32>(), captures[3].parse::<u32>())
            else {
                continue;
            };
            let Some(requested_number) = requested_number else {
                continue;
            };
            if requested_number < start_number || requested_number > end_number {
                continue;
            }
            let start_suffix = captures.get(2).map(|value| value.as_str()).unwrap_or("");
            let end_suffix = captures.get(4).map(|value| value.as_str()).unwrap_or("");
            let lower_suffix = if requested_number == start_number {
                start_suffix.chars().next().unwrap_or('\0')
            } else {
                '\0'
            };
            let upper_suffix = if requested_number == end_number {
                end_suffix.chars().next().unwrap_or('z')
            } else {
                'z'
            };
            if lower_suffix <= requested_suffix && requested_suffix <= upper_suffix {
                return true;
            }
        }
    }

    let same_base_variant_present = house_variant_regex()
        .captures_iter(segment)
        .any(|captures| {
            let base = captures
                .get(1)
                .or_else(|| captures.get(5))
                .map(|value| value.as_str());
            base == Some(requested_digits.as_str())
        });
    if same_base_variant_present {
        return false;
    }

    true
}

fn matching_address_index(message: &str, location: &str, settings: &Settings) -> Option<usize> {
    settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active && address_matches_notice(message, location, address)
        })
        .map(|(index, _)| index)
}

async fn fetch_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP status {}", response.status()));
    }
    response
        .text()
        .await
        .map_err(|error| format!("response read failed: {error}"))
}

async fn fetch_articles(
    client: &Client,
    notices: Vec<NoticeLink>,
) -> (Vec<(NoticeLink, Result<Article, String>)>, Vec<String>) {
    let results = stream::iter(notices.into_iter().map(|notice| async move {
        let result = match fetch_text(client, &notice.url).await {
            Ok(html) => parse_article(&html, &notice.title)
                .ok_or_else(|| "article title or body not found".to_string()),
            Err(error) => Err(error),
        };
        (notice, result)
    }))
    .buffer_unordered(MAX_CONCURRENT_ARTICLES)
    .collect::<Vec<_>>()
    .await;
    (results, Vec::new())
}

fn page_url(page: usize) -> String {
    if page <= 1 {
        EPEC_ELBLAG_ARCHIVE_URL.to_string()
    } else {
        format!("{}page/{page}/", EPEC_ELBLAG_ARCHIVE_URL)
    }
}

fn archive_is_old_enough(links: &[NoticeLink], cutoff: NaiveDate) -> bool {
    !links.is_empty()
        && links
            .iter()
            .all(|notice| notice.published.is_some_and(|date| date < cutoff))
}

fn archive_page_number(url: &str) -> Option<usize> {
    Url::parse(url)
        .ok()?
        .path_segments()?
        .collect::<Vec<_>>()
        .windows(2)
        .find(|segments| segments[0] == "page")?
        .get(1)?
        .parse()
        .ok()
}

fn next_archive_page(
    html: &str,
    current_page: usize,
    current_url: &str,
) -> Option<(usize, String)> {
    let document = Html::parse_document(html);
    let anchor_selector = Selector::parse("a[href]").expect("valid EPEC pagination selector");
    let current_url = Url::parse(current_url).ok()?;

    document
        .select(&anchor_selector)
        .filter_map(|anchor| {
            let href = anchor.value().attr("href")?;
            let url = current_url.join(href).ok()?.to_string();
            let page = archive_page_number(&url)?;
            (page > current_page).then_some((page, url))
        })
        .min_by_key(|(page, _)| *page)
}

async fn collect_recent_notices(client: &Client) -> Result<Vec<NoticeLink>, String> {
    let cutoff =
        Utc::now().with_timezone(&Warsaw).date_naive() - Duration::days(PUBLICATION_LOOKBACK_DAYS);
    let mut notices = Vec::new();
    let mut current_page = 1;
    let mut url = page_url(current_page);

    loop {
        let html = fetch_text(client, &url)
            .await
            .map_err(|error| format!("archive fetch failed for {url}: {error}"))?;
        let page_notices = archive_links(&html, &url);
        if page_notices.is_empty() {
            if current_page == 1 {
                return Err("EPEC archive notice cards were not found".to_string());
            }
            break;
        }

        let page_links = page_notices
            .iter()
            .filter(|notice| {
                is_outage_candidate(&format!("{} {}", notice.title, notice.excerpt))
                    && notice.published.is_none_or(|published| published >= cutoff)
            })
            .cloned()
            .collect::<Vec<_>>();
        notices.extend(page_links);

        if archive_is_old_enough(&page_notices, cutoff) {
            break;
        }
        let Some((next_page, next_url)) = next_archive_page(&html, current_page, &url) else {
            break;
        };
        current_page = next_page;
        url = next_url;
    }

    let mut unique = std::collections::HashSet::new();
    notices.retain(|notice| unique.insert(notice.url.clone()));
    Ok(notices)
}

#[async_trait]
impl AlertProvider for EpecElblagProvider {
    fn id(&self) -> String {
        "epec_elblag".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::EpecElblag
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

        let notices = match collect_recent_notices(client).await {
            Ok(notices) => notices,
            Err(error) => return (Vec::new(), vec![format!("EPEC Elbląg {error}")]),
        };
        let (articles, mut errors) = fetch_articles(client, notices).await;
        let mut alerts = Vec::new();

        for (notice, result) in articles {
            match result {
                Ok(article) => {
                    let mut article_alerts = make_alerts(&notice, article);
                    for alert in &mut article_alerts {
                        let message = alert.message.as_deref().unwrap_or_default();
                        let location = alert.location.as_deref().unwrap_or_default();
                        alert.address_index = matching_address_index(message, location, settings);
                        alert.is_local = Some(alert.address_index.is_some());
                    }
                    alerts.extend(article_alerts);
                }
                Err(error) => errors.push(format!(
                    "EPEC Elbląg article parse failed for {}: {error}",
                    notice.url
                )),
            }
        }

        (crate::api_logic::deduplicate_alerts(alerts), errors)
    }
}
