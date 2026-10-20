use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use futures::stream::{self, StreamExt};
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use std::sync::OnceLock;

pub const MPEC_KIELCE_NEWS_URL: &str = "https://www.mpec.kielce.pl/aktualnosci";
const SITE_ORIGIN: &str = "https://www.mpec.kielce.pl";
const MAX_CONCURRENT_ARTICLES: usize = 5;

pub struct MpecKielceProvider;

#[derive(Debug, Clone)]
struct NoticeLink {
    url: String,
    published: Option<NaiveDate>,
}

#[derive(Debug)]
struct Article {
    title: String,
    body: String,
    pdf_text: Option<String>,
    published: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy)]
struct ClockValue {
    time: NaiveTime,
    next_day: bool,
}

#[derive(Debug, Clone)]
struct ParsedSchedule {
    start: String,
    end: String,
}

#[derive(Debug)]
struct RestorationNotice {
    title: String,
    body: String,
    published: Option<NaiveDate>,
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

fn date_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(\d{1,2})\s*\.\s*(\d{1,2})(?:\s*\.\s*(\d{4}))?\s*(?:-|–|—|do|na)\s*(\d{1,2})\s*\.\s*(\d{1,2})(?:\s*\.\s*(\d{4}))?",
        )
        .unwrap()
    })
}

fn numeric_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap())
}

fn yearless_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\b").unwrap())
}

fn named_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|wrzesnia|pazdziernika|listopada|grudnia)\s+(\d{4})\b",
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

fn year_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(20\d{2})\b").unwrap())
}

fn pdf_link_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\.pdf(?:\?|$)").unwrap())
}

fn restoration_title_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)(?:awaria\s+usunieta|zaklocenia\s+usuniete?)\s*[-–:]\s*([^,\n]+)").unwrap()
    })
}

fn area_mention_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:os\.?|osiedle|ul\.?|ulica|rejon(?:\s+ul\.?)?)\s+([\p{L}][\p{L}\s-]{1,50})",
        )
        .unwrap()
    })
}

fn source_street_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)ze\s+zrodla\s+ciepla\s+przy\s+(?:ul\.?\s*)?[^,.;]+").unwrap()
    })
}

fn month_number(name: &str) -> Option<u32> {
    Some(match name {
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

fn datetime_string(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_clock(hour: &str, minute: &str) -> Option<ClockValue> {
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some(ClockValue {
            time: NaiveTime::from_hms_opt(0, 0, 0)?,
            next_day: true,
        });
    }
    Some(ClockValue {
        time: NaiveTime::from_hms_opt(hour, minute, 0)?,
        next_day: false,
    })
}

fn parse_date_range(
    captures: &regex::Captures<'_>,
    fallback_year: i32,
) -> Option<(NaiveDate, NaiveDate)> {
    let start_day = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let start_month = captures.get(2)?.as_str().parse::<u32>().ok()?;
    let end_day = captures.get(4)?.as_str().parse::<u32>().ok()?;
    let end_month = captures.get(5)?.as_str().parse::<u32>().ok()?;
    let first_year = captures
        .get(3)
        .and_then(|value| value.as_str().parse::<i32>().ok());
    let second_year = captures
        .get(6)
        .and_then(|value| value.as_str().parse::<i32>().ok());
    let start_year = first_year.or(second_year).unwrap_or(fallback_year);
    let start = NaiveDate::from_ymd_opt(start_year, start_month, start_day)?;
    let mut end_year = second_year.unwrap_or(start_year);
    if second_year.is_none() && (end_month, end_day) < (start_month, start_day) {
        end_year += 1;
    }
    let end = NaiveDate::from_ymd_opt(end_year, end_month, end_day)?;
    (end >= start).then_some((start, end))
}

fn dates_without_event_date_patterns(text: &str) -> String {
    let without_ranges = date_range_regex().replace_all(text, " ");
    let without_numeric = numeric_date_regex().replace_all(&without_ranges, " ");
    named_date_regex()
        .replace_all(&normalize_polish(&without_numeric), " ")
        .into_owned()
}

fn find_single_date(text: &str, fallback_year: i32) -> Option<NaiveDate> {
    if let Some(captures) = numeric_date_regex().captures(text) {
        return NaiveDate::from_ymd_opt(
            captures.get(3)?.as_str().parse().ok()?,
            captures.get(2)?.as_str().parse().ok()?,
            captures.get(1)?.as_str().parse().ok()?,
        );
    }

    let normalized = normalize_polish(text);
    if let Some(captures) = named_date_regex().captures(&normalized) {
        return NaiveDate::from_ymd_opt(
            captures.get(3)?.as_str().parse().ok()?,
            month_number(captures.get(2)?.as_str())?,
            captures.get(1)?.as_str().parse().ok()?,
        );
    }

    yearless_date_regex().captures(text).and_then(|captures| {
        NaiveDate::from_ymd_opt(
            fallback_year,
            captures.get(2)?.as_str().parse().ok()?,
            captures.get(1)?.as_str().parse().ok()?,
        )
    })
}

fn parse_schedule(text: &str, fallback_year: i32, now: NaiveDateTime) -> Option<ParsedSchedule> {
    let (mut start_date, mut end_date) = if let Some(captures) = date_range_regex().captures(text) {
        parse_date_range(&captures, fallback_year)?
    } else {
        let date = find_single_date(text, fallback_year)?;
        (date, date)
    };

    let time_text = dates_without_event_date_patterns(text);
    let clocks = time_range_regex()
        .captures(&time_text)
        .and_then(|captures| {
            Some((
                parse_clock(captures.get(1)?.as_str(), captures.get(2)?.as_str())?,
                parse_clock(captures.get(3)?.as_str(), captures.get(4)?.as_str())?,
            ))
        });

    let (start_time, end_time) = if let Some((start, end)) = clocks {
        if start.next_day {
            start_date += Duration::days(1);
        }
        if end.next_day {
            end_date += Duration::days(1);
        }
        (start.time, end.time)
    } else {
        (
            NaiveTime::from_hms_opt(0, 0, 0)?,
            NaiveTime::from_hms_opt(23, 59, 0)?,
        )
    };

    let mut start = start_date.and_time(start_time);
    let mut end = end_date.and_time(end_time);
    if end <= start && end_date == start_date {
        end += Duration::days(1);
    }
    if end <= now {
        return None;
    }

    // A 24:00 end is represented as 00:00 on the following local calendar day.
    if end_time == NaiveTime::from_hms_opt(0, 0, 0)? && end_date > start_date {
        end = end_date.and_time(end_time);
    }
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }

    Some(ParsedSchedule {
        start: datetime_string(start),
        end: datetime_string(end),
    })
}

fn is_restoration_notice(text: &str) -> bool {
    let normalized = normalize_polish(text);
    normalized.contains("awaria usunieta")
        || normalized.contains("awarie usunieto")
        || normalized.contains("dostawy ciepla zostaly wznowione")
        || normalized.contains("przywrocono dostawe")
        || normalized.contains("przywrocono dostawy")
        || normalized.contains("zakonczono prace naprawcze")
}

fn outage_kind(text: &str) -> Option<&'static str> {
    let normalized = normalize_polish(text);
    if is_restoration_notice(&normalized) {
        return None;
    }

    let heat_context = normalized.contains("ciepl")
        || normalized.contains("c.w.u")
        || normalized.contains("cwu")
        || normalized.contains("ogrzewan");
    let outage_context = normalized.contains("przer")
        || normalized.contains("awari")
        || normalized.contains("zaklocen")
        || normalized.contains("wstrzyman");
    if !heat_context || !outage_context {
        return None;
    }

    Some(
        if normalized.contains("awari") || normalized.contains("awaryjn") {
            "Awaria ciepła"
        } else {
            "Planowana przerwa w dostawie ciepła"
        },
    )
}

fn clean_text(value: &str) -> String {
    value
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn text_from_element(element: ElementRef<'_>) -> String {
    clean_text(&element.text().collect::<Vec<_>>().join(" "))
}

fn normalize_url(href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty()
        || href.starts_with('#')
        || href.starts_with("mailto:")
        || href.starts_with("javascript:")
    {
        return None;
    }
    let url = if href.starts_with("https://") || href.starts_with("http://") {
        href.to_string()
    } else if href.starts_with('/') {
        format!("{SITE_ORIGIN}{href}")
    } else {
        format!("{SITE_ORIGIN}/{href}")
    };
    url.starts_with("https://www.mpec.kielce.pl/")
        .then_some(url)
}

fn is_outage_candidate(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "awari",
        "przerw",
        "zakloc",
        "dostaw ciepl",
        "harmonogram przerw",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn polish_date_from_text(text: &str) -> Option<NaiveDate> {
    let normalized = normalize_polish(text);
    if let Some(captures) = numeric_date_regex().captures(&normalized) {
        return NaiveDate::from_ymd_opt(
            captures.get(3)?.as_str().parse().ok()?,
            captures.get(2)?.as_str().parse().ok()?,
            captures.get(1)?.as_str().parse().ok()?,
        );
    }
    let captures = named_date_regex().captures(&normalized)?;
    NaiveDate::from_ymd_opt(
        captures.get(3)?.as_str().parse().ok()?,
        month_number(captures.get(2)?.as_str())?,
        captures.get(1)?.as_str().parse().ok()?,
    )
}

fn find_notice_links(html: &str) -> Vec<NoticeLink> {
    let document = Html::parse_document(html);
    let Ok(links) = Selector::parse("a[href]") else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut candidates = Vec::new();

    for link in document.select(&links) {
        if normalize_polish(&text_from_element(link)).trim() != "zobacz wiecej" {
            continue;
        }
        let Some(url) = link.value().attr("href").and_then(normalize_url) else {
            continue;
        };
        if !seen.insert(url.clone()) {
            continue;
        }
        let card = link.parent().and_then(ElementRef::wrap);
        let listing_text = card.map(text_from_element).unwrap_or_default();
        if !is_outage_candidate(&listing_text) {
            continue;
        }
        let published = polish_date_from_text(&listing_text);
        candidates.push(NoticeLink { url, published });
    }
    candidates
}

fn article_details(html: &str) -> (String, String, Option<String>) {
    let document = Html::parse_document(html);
    let Ok(content_selector) = Selector::parse("div.desc_content") else {
        return (String::new(), String::new(), None);
    };
    let Ok(title_selector) = Selector::parse("h1") else {
        return (String::new(), String::new(), None);
    };
    let Ok(pdf_selector) = Selector::parse("a[href]") else {
        return (String::new(), String::new(), None);
    };
    let Some(content) = document.select(&content_selector).next() else {
        return (String::new(), String::new(), None);
    };

    let title = content
        .select(&title_selector)
        .map(text_from_element)
        .find(|heading| !heading.is_empty() && normalize_polish(heading) != "aktualnosci")
        .unwrap_or_default();
    let body = text_from_element(content);
    let pdf = content
        .select(&pdf_selector)
        .filter_map(|anchor| anchor.value().attr("href"))
        .find(|href| pdf_link_regex().is_match(href))
        .and_then(normalize_url);
    (title, body, pdf)
}

fn fallback_year(title: &str, body: &str, published: Option<NaiveDate>, now: NaiveDateTime) -> i32 {
    year_regex()
        .captures(title)
        .or_else(|| year_regex().captures(body))
        .and_then(|captures| captures.get(1)?.as_str().parse::<i32>().ok())
        .or_else(|| published.map(|date| date.year()))
        .unwrap_or_else(|| now.year())
}

fn strip_source_location(text: &str) -> String {
    source_street_regex()
        .replace_all(&normalize_polish(text), " ")
        .into_owned()
}

fn street_matches_address(text: &str, address: &AddressEntry) -> bool {
    let notice = normalize_polish(&strip_source_location(text));
    let mut configured_streets = vec![address.street_name.as_str(), address.street_name_1.as_str()];
    if let Some(second) = address.street_name_2.as_deref() {
        configured_streets.push(second);
    }
    configured_streets.into_iter().any(|street| {
        let street = crate::utils::strip_street_prefixes(street.trim());
        let tokens = normalize_polish(street)
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .map(str::to_string)
            .collect::<Vec<_>>();
        !tokens.is_empty()
            && tokens.iter().all(|token| {
                let stem_length = token.chars().count().saturating_sub(2).max(3);
                let stem = token.chars().take(stem_length).collect::<String>();
                notice
                    .split(|character: char| !character.is_alphanumeric())
                    .any(|word| word.starts_with(&stem))
            })
    })
}

fn is_citywide_notice(text: &str) -> bool {
    let normalized = normalize_polish(text);
    normalized.contains("w calym miescie")
        || normalized.contains("na terenie calego miasta")
        || normalized.contains("na terenie miasta kielce")
        || normalized.contains("wszyscy odbiorcy w kielcach")
}

fn matching_address_index(text: &str, settings: &Settings) -> Option<usize> {
    let citywide = is_citywide_notice(text);
    settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::MpecKielce,
                    address,
                )
                && (citywide || street_matches_address(text, address))
        })
        .map(|(index, _)| index)
}

fn notice_location(text: &str) -> String {
    let normalized = normalize_polish(text);
    let patterns = [
        r"(?i)(?:ul\.?|ulicy|ulica)\s+([\p{L}][\p{L}-]*(?:\s+[\p{L}][\p{L}-]*){0,2})",
        r"(?i)(?:os\.?|osiedle)\s+([\p{L}][\p{L}-]*(?:\s+[\p{L}][\p{L}-]*){0,2})",
        r"(?i)rejon(?:ie|u)?\s*:?\s*([\p{L}][\p{L}-]*(?:\s+[\p{L}][\p{L}-]*){0,2})",
    ];
    for pattern in patterns {
        let regex = Regex::new(pattern).unwrap();
        if let Some(captures) = regex.captures(&normalized)
            && let Some(area) = captures.get(1)
        {
            return format!("Miejscowość: Kielce, {}", clean_text(area.as_str()));
        }
    }
    "Miejscowość: Kielce".to_string()
}

fn create_alert(
    kind: &str,
    display_text: &str,
    match_text: &str,
    schedule: ParsedSchedule,
    location: String,
    settings: &Settings,
) -> UnifiedAlert {
    let address_index = matching_address_index(match_text, settings);
    UnifiedAlert {
        source: AlertSource::MpecKielce,
        startDate: Some(schedule.start),
        endDate: Some(schedule.end),
        message: Some(format!("{kind}: {}", clean_text(display_text))),
        location: Some(location),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: None,
    }
}

fn parse_article_alert(
    title: &str,
    body: &str,
    published: Option<NaiveDate>,
    settings: &Settings,
    now: NaiveDateTime,
) -> Option<UnifiedAlert> {
    let text = format!("{title}\n{body}");
    let kind = outage_kind(&text)?;
    let schedule = parse_schedule(&text, fallback_year(title, body, published, now), now)?;
    let match_text = strip_source_location(&text);
    let location = notice_location(&match_text);
    Some(create_alert(
        kind,
        &text,
        &match_text,
        schedule,
        location,
        settings,
    ))
}

fn pdf_header(line: &str) -> bool {
    let normalized = normalize_polish(line);
    normalized.contains("zaklocenia w dostawach")
        || normalized.contains("przerwa w dostawach")
        || normalized.starts_with("obszar przerwa")
        || normalized.starts_with("2026 - zaklocenia")
        || normalized.contains("powyzsze terminy")
}

fn pdf_ignorable_line(line: &str) -> bool {
    let normalized = normalize_polish(line);
    normalized.is_empty()
        || normalized == "sierpien wrzesien"
        || normalized.starts_with("obszar przerwa")
        || normalized.starts_with("powyzsze terminy")
        || normalized == "2026 - zaklocenia w dostawach"
}

fn parse_pdf_schedule(
    text: &str,
    fallback_year: i32,
    now: NaiveDateTime,
    settings: &Settings,
) -> Vec<UnifiedAlert> {
    let mut alerts = Vec::new();
    let mut pending_area = Vec::new();
    let mut section = "Przerwa w dostawach ciepła".to_string();

    for line in text.lines().map(clean_text).filter(|line| !line.is_empty()) {
        let normalized = normalize_polish(&line);
        if normalized.contains("zaklocenia w dostawach") {
            section = "Zakłócenia w dostawach ciepła".to_string();
            pending_area.clear();
            continue;
        }
        if normalized.contains("przerwa w dostawach ciepla") {
            section = "Przerwa w dostawach ciepła".to_string();
            pending_area.clear();
            continue;
        }

        let ranges = date_range_regex()
            .captures_iter(&line)
            .filter_map(|captures| {
                let matched = captures.get(0)?;
                let dates = parse_date_range(&captures, fallback_year)?;
                let start = dates.0.and_hms_opt(0, 0, 0)?;
                let end = dates.1.and_hms_opt(23, 59, 0)?;
                (end > now).then_some((
                    matched.start(),
                    matched.end(),
                    matched.as_str().to_string(),
                    ParsedSchedule {
                        start: datetime_string(start),
                        end: datetime_string(end),
                    },
                ))
            })
            .collect::<Vec<_>>();

        if ranges.is_empty() {
            if !pdf_ignorable_line(&line) && !line.starts_with("•") {
                pending_area.push(line);
            } else if pdf_header(&line) {
                pending_area.clear();
            }
            continue;
        }

        let first_range_start = ranges[0].0;
        let current_prefix = clean_text(&line[..first_range_start]);
        let area = clean_text(&format!("{} {}", pending_area.join(" "), current_prefix));
        let area = area.trim_matches([' ', '-', '–', '—', ':']).to_string();
        let display_area = if area.is_empty() {
            "odbiorcy w Kielcach"
        } else {
            &area
        };
        let display_text = format!("{section}: {display_area}");
        let match_text = strip_source_location(&display_text);
        let location = notice_location(&match_text);

        for (_, _, date_text, schedule) in ranges {
            let message = format!("{display_text}; termin: {date_text}");
            alerts.push(create_alert(
                "Planowana przerwa w dostawie ciepła",
                &message,
                &match_text,
                schedule,
                location.clone(),
                settings,
            ));
        }
        pending_area.clear();
    }

    crate::api_logic::deduplicate_alerts(alerts)
}

fn meaningful_location_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.len() > 3)
        .filter(|word| {
            !matches!(
                *word,
                "kielce"
                    | "kielcach"
                    | "dostawy"
                    | "ciepla"
                    | "awaria"
                    | "usunieta"
                    | "rejon"
                    | "przerwa"
                    | "cieplnej"
                    | "miejskie"
                    | "przedsiebiorstwo"
                    | "energetyki"
                    | "informuje"
                    | "dostaw"
                    | "wznowiono"
                    | "wznowione"
            )
        })
        .map(str::to_string)
        .collect()
}

fn restoration_location_terms(title: &str, body: &str) -> Vec<String> {
    let normalized_title = normalize_polish(title);
    if let Some(captures) = restoration_title_regex().captures(&normalized_title)
        && let Some(value) = captures.get(1)
    {
        let title_tokens = meaningful_location_tokens(value.as_str());
        if !title_tokens.is_empty() {
            return title_tokens;
        }
    }

    let normalized_body = normalize_polish(body);
    let mut phrases = Vec::new();
    for captures in area_mention_regex().captures_iter(&normalized_body) {
        if let Some(value) = captures.get(1) {
            phrases.push(value.as_str().to_string());
        }
    }
    phrases
        .into_iter()
        .flat_map(|phrase| meaningful_location_tokens(&phrase))
        .collect()
}

fn is_suppressed_by_restoration(alert: &UnifiedAlert, restorations: &[RestorationNotice]) -> bool {
    let Some(start) = alert
        .startDate
        .as_deref()
        .and_then(|value| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok())
    else {
        return false;
    };
    let Some(end) = alert
        .endDate
        .as_deref()
        .and_then(|value| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok())
    else {
        return false;
    };
    let event_text = normalize_polish(alert.message.as_deref().unwrap_or_default());

    restorations.iter().any(|restoration| {
        let Some(published) = restoration.published else {
            return false;
        };
        if published < start.date() || published > end.date() {
            return false;
        }
        restoration_location_terms(&restoration.title, &restoration.body)
            .iter()
            .any(|word| event_text.contains(word))
    })
}

async fn fetch_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (compatible; Awaria/1.0)")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response.text().await.map_err(|error| error.to_string())
}

async fn fetch_article(client: &Client, link: NoticeLink) -> Result<(Article, NoticeLink), String> {
    let html = fetch_text(client, &link.url).await?;
    let (title, body, pdf_url) = article_details(&html);
    let pdf_text = if let Some(pdf_url) = pdf_url {
        let response = client
            .get(&pdf_url)
            .header("User-Agent", "Mozilla/5.0 (compatible; Awaria/1.0)")
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("PDF HTTP {}", response.status()));
        }
        let bytes = response.bytes().await.map_err(|error| error.to_string())?;
        Some(pdf_extract::extract_text_from_mem(&bytes).map_err(|error| error.to_string())?)
    } else {
        None
    };
    Ok((
        Article {
            title,
            body,
            pdf_text,
            published: link.published,
        },
        link,
    ))
}

#[async_trait]
impl AlertProvider for MpecKielceProvider {
    fn id(&self) -> String {
        "mpec_kielce".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::MpecKielce
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

        let listing = match fetch_text(client, MPEC_KIELCE_NEWS_URL).await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("MPEC Kielce request error: {error}")],
                );
            }
        };
        let candidates = find_notice_links(&listing);
        let fetched = stream::iter(
            candidates
                .into_iter()
                .map(|link| async move { fetch_article(client, link).await }),
        )
        .buffer_unordered(MAX_CONCURRENT_ARTICLES)
        .collect::<Vec<_>>()
        .await;

        let now = Local::now().naive_local();
        let mut alerts = Vec::new();
        let mut errors = Vec::new();
        let mut restorations = Vec::new();
        for result in fetched {
            let (article, link) = match result {
                Ok(article) => article,
                Err(error) => {
                    errors.push(format!("MPEC Kielce article request error: {error}"));
                    continue;
                }
            };
            let full_text = format!("{}\n{}", article.title, article.body);
            if is_restoration_notice(&full_text) {
                restorations.push(RestorationNotice {
                    title: article.title,
                    body: article.body,
                    published: article.published.or(link.published),
                });
                continue;
            }

            if let Some(alert) = parse_article_alert(
                &article.title,
                &article.body,
                article.published,
                settings,
                now,
            ) {
                alerts.push(alert);
            }
            if let Some(pdf_text) = article.pdf_text {
                let year = fallback_year(&article.title, &pdf_text, article.published, now);
                alerts.extend(parse_pdf_schedule(&pdf_text, year, now, settings));
            }
        }

        alerts.retain(|alert| !is_suppressed_by_restoration(alert, &restorations));
        (crate::api_logic::deduplicate_alerts(alerts), errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn fixed_now(date: &str) -> NaiveDateTime {
        NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    fn settings(city_name: &str, city_id: Option<u64>, street: &str) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Home".to_string(),
                city_name: city_name.to_string(),
                voivodeship: "Świętokrzyskie".to_string(),
                district: "Kielce".to_string(),
                commune: "Kielce".to_string(),
                street_name: street.to_string(),
                street_name_1: street.to_string(),
                street_name_2: None,
                house_no: "25".to_string(),
                city_id,
                street_id: None,
                is_active: true,
            }],
            ..Default::default()
        }
    }

    const PDF_TEXT: &str = r#"
Sierpień Wrzesień
Przerwa w dostawach ciepła Przerwa w dostawach ciepła
Słoneczne Wzgórze (wszystkie budynki) S74 17.08 - 22.08 07.09 - 12.09
Na Stoku (wszystkie budynki) S74 17.08 - 22.08 07.09 - 12.09
Rejon: ul. Uniwersytecka (UJK) / Solidarności 34 S74 07.09 - 12.09
2026 - ZAKŁÓCENIA W DOSTAWACH
Zakłócenia w dostawach ciepła
Nowy Bocianek S74 17.08 - 22.08 07.09 - 12.09
"#;

    #[test]
    fn discovers_only_outage_article_links_from_notice_cards() {
        let html = r#"
            <div>
              <div>12 września 2026</div><h2>KOMUNIKAT - przerwa w dostawach ciepła</h2>
              <div>Wystąpi przerwa w dostawie ciepła...</div>
              <a href="komunikat-przerwa">zobacz więcej</a>
            </div>
            <div>
              <div>13 września 2026</div><h2>Biuro nieczynne</h2>
              <div>Informacja o godzinach pracy.</div>
              <a href="biuro-nieczynne">zobacz więcej</a>
            </div>
        "#;

        let links = find_notice_links(html);
        assert_eq!(links.len(), 1);
        assert!(links[0].url.ends_with("/komunikat-przerwa"));
        assert_eq!(
            links[0].published,
            Some(NaiveDate::from_ymd_opt(2026, 9, 12).unwrap())
        );
        assert!(find_notice_links("<html><body>Brak ogłoszeń</body></html>").is_empty());
    }

    #[test]
    fn parses_date_only_multi_day_interruption_and_expires_old_notices() {
        let now = fixed_now("2026-09-16");
        let schedule = parse_schedule(
            "W dniach 15.09 – 20.09.2026 wystąpi przerwa w dostawach ciepła",
            2026,
            now,
        )
        .unwrap();
        assert_eq!(schedule.start, "2026-09-15T00:00:00");
        assert_eq!(schedule.end, "2026-09-20T23:59:00");

        assert!(
            parse_schedule(
                "Przerwa w dostawach ciepła 04.08 - 05.08.2026",
                2026,
                fixed_now("2026-08-06")
            )
            .is_none()
        );
    }

    #[test]
    fn parses_named_dates_and_explicit_clock_ranges() {
        let schedule = parse_schedule(
            "Awaria sieci w dniu 17 kwietnia 2026 r. w godzinach od 8:00 do 12:30",
            2026,
            fixed_now("2026-04-16"),
        )
        .unwrap();
        assert_eq!(schedule.start, "2026-04-17T08:00:00");
        assert_eq!(schedule.end, "2026-04-17T12:30:00");
        assert!(
            parse_schedule(
                "Przerwa w dostawie ciepła bez daty",
                2026,
                fixed_now("2026-04-16")
            )
            .is_none()
        );
        assert!(
            parse_schedule(
                "Przerwa w dostawie ciepła 32.13.2026",
                2026,
                fixed_now("2026-04-16")
            )
            .is_none()
        );
    }

    #[test]
    fn classifies_failures_planned_work_and_restorations() {
        assert_eq!(
            outage_kind("Przerwa w dostawie ciepłej wody z powodu stanu awaryjnego"),
            Some("Awaria ciepła")
        );
        assert_eq!(
            outage_kind("Prace technologiczne spowodują przerwę w dostawach ciepła"),
            Some("Planowana przerwa w dostawie ciepła")
        );
        assert_eq!(
            outage_kind("Awaria usunięta. Dostawy ciepła wznowiono."),
            None
        );
        assert_eq!(outage_kind("Biuro MPEC będzie nieczynne"), None);
    }

    #[test]
    fn extracts_full_article_body_and_its_linked_schedule() {
        let html = r#"
            <div class="desc_content">
              <h1>Przerwa w dostawie ciepłej wody 04.08 - 05.08.2026 r.</h1>
              <p>W związku z wystąpieniem stanu awaryjnego w komorze ciepłowniczej</p>
              <p>w dniach 04.08 - 05.08.2026 wystąpi przerwa dla budynków przy ul. Kościuszki 25 i ul. Niska 2.</p>
              <a href="storage/images/sites/11/files/schedule.pdf">Harmonogram (pdf, 20 KiB)</a>
            </div>
        "#;
        let (title, body, pdf) = article_details(html);
        assert!(title.starts_with("Przerwa w dostawie ciepłej wody"));
        assert!(body.contains("Kościuszki 25") && body.contains("Niska 2"));
        assert_eq!(
            pdf.as_deref(),
            Some("https://www.mpec.kielce.pl/storage/images/sites/11/files/schedule.pdf")
        );

        let alert = parse_article_alert(
            &title,
            &body,
            Some(NaiveDate::from_ymd_opt(2026, 8, 4).unwrap()),
            &settings("Kielce", Some(945930), "Niska"),
            fixed_now("2026-08-04"),
        )
        .unwrap();
        assert_eq!(alert.startDate.as_deref(), Some("2026-08-04T00:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-08-05T23:59:00"));
        assert_eq!(alert.is_local, Some(true));
    }

    #[test]
    fn matches_kielce_addresses_by_affected_street_not_heat_source_address() {
        let kielce = settings("Kielce", Some(945930), "Wrzosowa");
        let wrong_city = settings("Lublin", Some(959423), "Wrzosowa");
        let alert = parse_article_alert(
            "Zakłócenia w dostawie ciepła Barwinek",
            "W związku z wystąpieniem stanu awaryjnego przy ul. Wrzosowej w Kielcach w dniu 17.04.2026 r. wystąpią zakłócenia w dostawie ciepła.",
            Some(NaiveDate::from_ymd_opt(2026, 4, 17).unwrap()),
            &kielce,
            fixed_now("2026-04-17"),
        )
        .unwrap();
        assert_eq!(alert.is_local, Some(true));
        assert_eq!(alert.address_index, Some(0));

        let unrelated = parse_article_alert(
            "Zakłócenia w dostawie ciepła Barwinek",
            "W związku z wystąpieniem stanu awaryjnego przy ul. Wrzosowej w Kielcach w dniu 17.04.2026 r. wystąpią zakłócenia w dostawie ciepła.",
            Some(NaiveDate::from_ymd_opt(2026, 4, 17).unwrap()),
            &wrong_city,
            fixed_now("2026-04-17"),
        )
        .unwrap();
        assert_eq!(unrelated.is_local, Some(false));

        let source_only = parse_article_alert(
            "Przerwa w dostawach ciepła",
            "W dniach 15.09 - 20.09.2026 wystąpi przerwa dla odbiorców zasilanych ze źródła ciepła przy ul. Hauke-Bosaka 2A w Kielcach.",
            Some(NaiveDate::from_ymd_opt(2026, 9, 11).unwrap()),
            &settings("Kielce", Some(945930), "Hauke-Bosaka"),
            fixed_now("2026-09-15"),
        )
        .unwrap();
        assert_eq!(source_only.is_local, Some(false));
    }

    #[test]
    fn matches_multiple_streets_without_applying_house_number_filter_in_provider() {
        let notice = "Przerwa w dostawie ciepła w dniu 17.04.2026 przy ul. Kościuszki 25 i 50 oraz ul. Niska 2.";
        for street in ["Kościuszki", "Niska"] {
            let mut without_house_filter = settings("Kielce", Some(945930), street);
            let with_house_filter = Settings {
                filter_by_house_no: true,
                ..without_house_filter.clone()
            };
            without_house_filter.addresses[0].house_no = "999".to_string();
            let with_house_filter = Settings {
                addresses: without_house_filter.addresses.clone(),
                ..with_house_filter
            };
            for settings in [&without_house_filter, &with_house_filter] {
                let alert = parse_article_alert(
                    "Przerwa w dostawie ciepła",
                    notice,
                    Some(NaiveDate::from_ymd_opt(2026, 4, 17).unwrap()),
                    settings,
                    fixed_now("2026-04-17"),
                )
                .unwrap();
                assert_eq!(alert.is_local, Some(true));
            }
        }
    }

    #[test]
    fn parses_pdf_rows_and_multiple_periods_without_using_publication_date_as_event_date() {
        let alerts = parse_pdf_schedule(
            PDF_TEXT,
            2026,
            fixed_now("2026-08-18"),
            &settings("Kielce", Some(945930), "Grunwaldzka"),
        );
        assert!(alerts.len() >= 4);
        assert!(alerts.iter().any(|alert| {
            alert
                .message
                .as_deref()
                .is_some_and(|message| message.contains("Słoneczne Wzgórze"))
                && alert.startDate.as_deref() == Some("2026-08-17T00:00:00")
                && alert.endDate.as_deref() == Some("2026-08-22T23:59:00")
        }));
        assert!(alerts.iter().any(|alert| {
            alert
                .message
                .as_deref()
                .is_some_and(|message| message.contains("Uniwersytecka"))
                && alert.startDate.as_deref() == Some("2026-09-07T00:00:00")
        }));
        assert!(
            parse_pdf_schedule(
                PDF_TEXT,
                2026,
                fixed_now("2026-10-01"),
                &settings("Kielce", Some(945930), "Grunwaldzka"),
            )
            .is_empty()
        );
    }

    #[test]
    fn restoration_suppresses_only_matching_active_location_and_date() {
        let alert = UnifiedAlert {
            source: AlertSource::MpecKielce,
            startDate: Some("2026-04-17T00:00:00".to_string()),
            endDate: Some("2026-04-17T23:59:00".to_string()),
            message: Some("Awaria ciepła: przerwa przy ul. Wrzosowej w Kielcach".to_string()),
            location: Some("Miejscowość: Kielce, Wrzosowej".to_string()),
            address_index: Some(0),
            is_local: Some(true),
            hash: None,
        };
        let matching = vec![RestorationNotice {
            title: "Awaria usunięta - ul. Wrzosowej".to_string(),
            body: "Dostawy ciepła wznowiono.".to_string(),
            published: Some(NaiveDate::from_ymd_opt(2026, 4, 17).unwrap()),
        }];
        let unrelated = vec![RestorationNotice {
            title: "Awaria usunięta - Barwinek".to_string(),
            body: "Dostawy ciepła wznowiono.".to_string(),
            published: Some(NaiveDate::from_ymd_opt(2026, 4, 17).unwrap()),
        }];
        assert!(is_suppressed_by_restoration(&alert, &matching));
        assert!(!is_suppressed_by_restoration(&alert, &unrelated));
    }

    #[tokio::test]
    #[ignore = "requires live access to the MPEC Kielce news archive"]
    async fn live_archive_and_notices_fetch_with_app_http_client() {
        let client = crate::network_state::NetworkState::build_client().unwrap();
        let listing = fetch_text(&client, MPEC_KIELCE_NEWS_URL).await.unwrap();
        let candidates = find_notice_links(&listing);
        assert!(!candidates.is_empty());
        if let Some(schedule_link) = candidates
            .iter()
            .find(|link| link.url.contains("harmonogram-przerw"))
            .cloned()
        {
            let (article, _) = fetch_article(&client, schedule_link).await.unwrap();
            assert!(article.title.contains("Harmonogram"));
            assert!(
                article
                    .pdf_text
                    .as_deref()
                    .is_some_and(|text| text.contains("Słoneczne Wzgórze"))
            );
        }
        let (alerts, errors) = MpecKielceProvider
            .fetch(&client, &client, &Settings::default(), None)
            .await;
        assert!(errors.is_empty(), "{errors:?}");
        log::info!("MPEC Kielce returned {} current alerts", alerts.len());
    }
}
