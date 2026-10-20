use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const WODOCIAGI_BIALOSTOCKIE_FEED_URL: &str =
    "https://www.wobi.pl/aktualnosci/awarie-i-remonty.feed?type=rss";

const ARTICLE_HOST: &str = "www.wobi.pl";
const ARTICLE_PATH_PREFIX: &str = "/aktualnosci/awarie-i-remonty/";
const SCHEDULELESS_NOTICE_MAX_AGE: Duration = Duration::hours(24);

pub struct WodociagiBialostockieProvider;

#[derive(Debug, Deserialize)]
struct RssFeed {
    channel: RssChannel,
}

#[derive(Debug, Deserialize)]
struct RssChannel {
    #[serde(rename = "item", default)]
    items: Vec<RssItem>,
}

#[derive(Debug, Deserialize)]
struct RssItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    guid: String,
    #[serde(rename = "pubDate", default)]
    pub_date: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "category", default)]
    categories: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedDate {
    date: NaiveDate,
    position: usize,
    explicit_year: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedTime {
    time: NaiveTime,
    next_day: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSchedule {
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
    reference_date: Option<NaiveDate>,
}

fn normalize_polish(text: &str) -> String {
    text.to_lowercase()
        .replace('ą', "a")
        .replace('ć', "c")
        .replace('ę', "e")
        .replace('ł', "l")
        .replace('ń', "n")
        .replace('ó', "o")
        .replace('ś', "s")
        .replace(['ź', 'ż'], "z")
}

fn numeric_date_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap())
}

fn numeric_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap())
}

fn month_date_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b(\d{1,2})/(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|wrzesnia|pazdziernika|listopada|grudnia)(?:\s+(\d{4}))?\b",
        )
        .unwrap()
    })
}

fn month_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|wrzesnia|pazdziernika|listopada|grudnia)(?:\s+(\d{4}))?\b",
        )
        .unwrap()
    })
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(2[0-4]|[01]?\d)[:.]([0-5]\d)\b").unwrap())
}

fn month_number(month: &str) -> Option<u32> {
    match month {
        "stycznia" => Some(1),
        "lutego" => Some(2),
        "marca" => Some(3),
        "kwietnia" => Some(4),
        "maja" => Some(5),
        "czerwca" => Some(6),
        "lipca" => Some(7),
        "sierpnia" => Some(8),
        "wrzesnia" => Some(9),
        "pazdziernika" => Some(10),
        "listopada" => Some(11),
        "grudnia" => Some(12),
        _ => None,
    }
}

fn date_from_parts(
    day: &str,
    month: u32,
    year: Option<&str>,
    publication_date: Option<NaiveDate>,
) -> Option<ParsedDate> {
    let explicit_year = year.is_some();
    let day = day.parse::<u32>().ok()?;
    let explicit_year_value = year.and_then(|value| value.parse::<i32>().ok());
    let mut resolved_year =
        explicit_year_value.or_else(|| publication_date.map(|date| date.year()))?;
    if explicit_year_value.is_none()
        && publication_date.is_some_and(|publication| publication.month() >= 10 && month <= 3)
        && NaiveDate::from_ymd_opt(resolved_year, month, day)
            .is_some_and(|date| date < publication_date.unwrap())
    {
        resolved_year += 1;
    }
    Some(ParsedDate {
        date: NaiveDate::from_ymd_opt(resolved_year, month, day)?,
        position: 0,
        explicit_year,
    })
}

fn add_date(dates: &mut Vec<ParsedDate>, position: usize, mut date: ParsedDate) {
    if let Some(existing) = dates.iter_mut().find(|existing| existing.date == date.date) {
        existing.explicit_year |= date.explicit_year;
        existing.position = existing.position.min(position);
        return;
    }
    date.position = position;
    dates.push(date);
}

fn extract_dates(text: &str, publication_date: Option<NaiveDate>) -> Vec<ParsedDate> {
    let normalized = normalize_polish(text);
    let mut dates = Vec::new();

    for captures in numeric_date_range_regex().captures_iter(&normalized) {
        let Some(month) = captures[3].parse::<u32>().ok() else {
            continue;
        };
        let year = captures.get(4).unwrap().as_str();
        for day_index in [1, 2] {
            if let Some(date) =
                date_from_parts(&captures[day_index], month, Some(year), publication_date)
            {
                add_date(&mut dates, captures.get(day_index).unwrap().start(), date);
            }
        }
    }

    for captures in numeric_date_regex().captures_iter(&normalized) {
        let Some(month) = captures[2].parse::<u32>().ok() else {
            continue;
        };
        let year = captures.get(3).unwrap().as_str();
        if let Some(date) = date_from_parts(&captures[1], month, Some(year), publication_date) {
            add_date(&mut dates, captures.get(1).unwrap().start(), date);
        }
    }

    for captures in month_date_range_regex().captures_iter(&normalized) {
        let Some(month) = month_number(&captures[3]) else {
            continue;
        };
        let year = captures.get(4).map(|value| value.as_str());
        for day_index in [1, 2] {
            if let Some(date) = date_from_parts(&captures[day_index], month, year, publication_date)
            {
                add_date(&mut dates, captures.get(day_index).unwrap().start(), date);
            }
        }
    }

    for captures in month_date_regex().captures_iter(&normalized) {
        let Some(month) = month_number(&captures[2]) else {
            continue;
        };
        let year = captures.get(3).map(|value| value.as_str());
        if let Some(date) = date_from_parts(&captures[1], month, year, publication_date) {
            add_date(&mut dates, captures.get(1).unwrap().start(), date);
        }
    }

    if dates.is_empty()
        && ["dzisiaj", "dzisiejszy", "dzisiejszym", "dzisiejszego"]
            .iter()
            .any(|word| normalized.contains(word))
        && let Some(publication_date) = publication_date
    {
        dates.push(ParsedDate {
            date: publication_date,
            position: 0,
            explicit_year: false,
        });
    }

    dates.sort_by_key(|date| date.position);
    dates
}

fn strip_dates(text: &str) -> String {
    let text = numeric_date_range_regex().replace_all(text, " ");
    let text = numeric_date_regex().replace_all(&text, " ");
    let normalized = normalize_polish(&text);
    let normalized = month_date_range_regex().replace_all(&normalized, " ");
    month_date_regex().replace_all(&normalized, " ").to_string()
}

fn parse_time(hour: &str, minute: &str) -> Option<ParsedTime> {
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some(ParsedTime {
            time: NaiveTime::from_hms_opt(0, 0, 0)?,
            next_day: true,
        });
    }
    Some(ParsedTime {
        time: NaiveTime::from_hms_opt(hour, minute, 0)?,
        next_day: false,
    })
}

fn extract_times(text: &str) -> Vec<ParsedTime> {
    time_regex()
        .captures_iter(&strip_dates(text))
        .filter_map(|captures| parse_time(&captures[1], &captures[2]))
        .collect()
}

fn only_time_is_end(text: &str) -> bool {
    let normalized = strip_dates(text);
    let Some(time_position) = time_regex().find(&normalized).map(|time| time.start()) else {
        return false;
    };
    [
        "do godz",
        "przewidywany czas usuniecia",
        "czas usuniecia awarii",
    ]
    .iter()
    .filter_map(|marker| normalized.rfind(marker))
    .any(|marker_position| marker_position < time_position)
}

fn extract_schedule(text: &str, publication_date: Option<NaiveDate>) -> ParsedSchedule {
    let dates = extract_dates(text, publication_date);
    let times = extract_times(text);
    let Some(start_date) = dates.first().map(|date| date.date) else {
        return ParsedSchedule {
            start: None,
            end: None,
            reference_date: None,
        };
    };
    let mut end_date = dates.get(1).map(|date| date.date).unwrap_or(start_date);
    if end_date < start_date
        && !dates
            .get(1)
            .is_some_and(|date| dates[0].explicit_year && date.explicit_year)
    {
        end_date = NaiveDate::from_ymd_opt(start_date.year() + 1, end_date.month(), end_date.day())
            .unwrap_or(end_date);
    }

    let (start, end) = match times.as_slice() {
        [] => (None, None),
        [time] if only_time_is_end(text) => {
            let end_date = if time.next_day {
                end_date + Duration::days(1)
            } else {
                end_date
            };
            (None, Some(end_date.and_time(time.time)))
        }
        [time] => {
            let start_date = if time.next_day {
                start_date + Duration::days(1)
            } else {
                start_date
            };
            (Some(start_date.and_time(time.time)), None)
        }
        [start_time, rest @ ..] => {
            let end_time = rest.last().unwrap();
            let start_date = if start_time.next_day {
                start_date + Duration::days(1)
            } else {
                start_date
            };
            let mut end_date = end_date;
            if end_time.next_day || (end_date == start_date && end_time.time <= start_time.time) {
                end_date += Duration::days(1);
            }
            (
                Some(start_date.and_time(start_time.time)),
                Some(end_date.and_time(end_time.time)),
            )
        }
    };

    ParsedSchedule {
        start,
        end,
        reference_date: Some(start_date),
    }
}

fn parse_publication_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|datetime| datetime.with_timezone(&Utc))
}

fn publication_date_in_warsaw(value: &str) -> Option<NaiveDate> {
    parse_publication_date(value).map(|datetime| datetime.with_timezone(&Warsaw).date_naive())
}

fn publication_is_fresh(value: &str, now: DateTime<Utc>) -> bool {
    let Some(published) = parse_publication_date(value) else {
        return false;
    };
    let age = now.signed_duration_since(published);
    age >= Duration::minutes(-10) && age <= SCHEDULELESS_NOTICE_MAX_AGE
}

fn is_resolved(text: &str) -> bool {
    let normalized = normalize_polish(text);
    normalized.contains("odwol")
        || normalized.contains("usunieta")
        || normalized.contains("usunieto awarie")
}

fn outage_kind(item: &RssItem) -> &'static str {
    let title = normalize_polish(&item.title);
    let categories = normalize_polish(&item.categories.join(" "));
    if title.contains("awari") || title.contains("awaryj") || categories.contains("awari") {
        "Awaria wodociągowa"
    } else if title.contains("remont") || categories.contains("remont") {
        "Planowane prace wodociągowe"
    } else {
        "Planowane wyłączenie wody"
    }
}

fn html_to_text(fragment: &str) -> String {
    let document = Html::parse_fragment(fragment);
    document
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn safe_article_url(link: &str) -> Result<Url, String> {
    let url = Url::parse(link).map_err(|error| format!("invalid article URL: {error}"))?;
    if url.scheme() != "https"
        || url.host_str() != Some(ARTICLE_HOST)
        || !url.path().starts_with(ARTICLE_PATH_PREFIX)
    {
        return Err("article URL is outside the WOBI outage site".to_string());
    }
    Ok(url)
}

fn extract_article_body(html: &str) -> Result<String, String> {
    let document = Html::parse_document(html);
    let main_selector = Selector::parse("main").unwrap();
    let paragraph_selector = Selector::parse("p").unwrap();
    let body = document
        .select(&main_selector)
        .flat_map(|main| main.select(&paragraph_selector))
        .map(|paragraph| {
            paragraph
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if body.is_empty() {
        Err("article page did not contain notice paragraphs".to_string())
    } else {
        Ok(body)
    }
}

async fn fetch_article_body(client: &Client, link: &str) -> Result<String, String> {
    let url = safe_article_url(link)?;
    let html = crate::utils::retry(
        || async {
            let response = client
                .get(url.clone())
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
    .await?;
    extract_article_body(&html)
}

fn parse_rss(xml: &str) -> Result<Vec<RssItem>, String> {
    quick_xml::de::from_str::<RssFeed>(xml)
        .map(|feed| feed.channel.items)
        .map_err(|error| format!("RSS parse error: {error}"))
}

async fn fetch_feed(client: &Client) -> Result<Vec<RssItem>, String> {
    let xml = crate::utils::retry(
        || async {
            let response = client
                .get(WODOCIAGI_BIALOSTOCKIE_FEED_URL)
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
    .await?;
    parse_rss(&xml)
}

fn locality_labels(text: &str) -> Vec<&'static str> {
    let normalized = normalize_polish(text);
    let mut labels = Vec::new();
    if normalized.contains("bialystok") || normalized.contains("bialymstoku") {
        labels.push("Białystok");
    }
    if normalized.contains("wasilkow")
        || ["jurowc", "sochon", "woroszyl", "dabrowk", "studzian"]
            .iter()
            .any(|locality| normalized.contains(locality))
        || [
            "wolka poduchown",
            "wolce poduchown",
            "wolka przedmiesc",
            "wolce przedmiesc",
        ]
        .iter()
        .any(|locality| normalized.contains(locality))
    {
        labels.push("Wasilków");
    }
    labels
}

fn is_wasilkow_address(address: &AddressEntry) -> bool {
    let city = normalize_polish(&address.city_name);
    let commune = normalize_polish(&address.commune);
    address.city_id == Some(923526)
        || city.starts_with("wasilkow")
        || commune.starts_with("wasilkow")
        || [
            "jurowc",
            "sochon",
            "woroszyl",
            "wolka poduchown",
            "wolka przedmiesc",
            "dabrowk",
            "studzian",
        ]
        .iter()
        .any(|locality| city.starts_with(locality))
}

fn locality_matches_notice(text: &str, address: &AddressEntry) -> bool {
    let labels = locality_labels(text);
    labels.is_empty()
        || labels.iter().any(|label| match *label {
            "Białystok" => {
                address.city_id == Some(922410)
                    || normalize_polish(&address.city_name).starts_with("bialystok")
            }
            "Wasilków" => is_wasilkow_address(address),
            _ => false,
        })
}

fn street_matches_notice(text: &str, address: &AddressEntry) -> bool {
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let notice_words: Vec<String> = normalize_polish(text)
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_string)
        .collect();

    candidates.into_iter().any(|candidate| {
        let stripped = crate::utils::strip_street_prefixes(candidate.trim());
        let tokens: Vec<String> = normalize_polish(stripped)
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .map(str::to_string)
            .collect();
        !tokens.is_empty()
            && tokens.iter().all(|token| {
                let stem = token.strip_suffix('a').unwrap_or(token);
                notice_words.iter().any(|word| word.starts_with(stem))
            })
    })
}

fn address_matches_notice(text: &str, address: &AddressEntry) -> bool {
    if !locality_matches_notice(text, address) {
        return false;
    }
    let has_street = !address.street_name.trim().is_empty()
        || !address.street_name_1.trim().is_empty()
        || address
            .street_name_2
            .as_deref()
            .is_some_and(|street| !street.trim().is_empty());
    !has_street || street_matches_notice(text, address)
}

fn is_expired(schedule: &ParsedSchedule, item: &RssItem, now_utc: DateTime<Utc>) -> bool {
    let now_local = now_utc.with_timezone(&Warsaw).naive_local();
    if schedule.end.is_some_and(|end| end < now_local) {
        return true;
    }
    if schedule.reference_date.is_none() {
        return !publication_is_fresh(&item.pub_date, now_utc);
    }
    if schedule
        .reference_date
        .is_some_and(|date| date < now_local.date())
    {
        return !publication_is_fresh(&item.pub_date, now_utc);
    }
    schedule.start.is_none()
        && schedule.end.is_none()
        && schedule.reference_date == Some(now_local.date())
        && !publication_is_fresh(&item.pub_date, now_utc)
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_notice(
    item: &RssItem,
    body: &str,
    settings: &Settings,
    now_utc: DateTime<Utc>,
) -> Option<UnifiedAlert> {
    let combined = format!("{} {}", item.title, body);
    if is_resolved(&combined) {
        return None;
    }
    let publication_date = publication_date_in_warsaw(&item.pub_date);
    let schedule = extract_schedule(&combined, publication_date);
    if is_expired(&schedule, item, now_utc) {
        return None;
    }

    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::WodociagiBialostockie,
                    address,
                )
                && address_matches_notice(&combined, address)
        })
        .map(|(index, _)| index);

    let locality = locality_labels(&combined);
    let location = if locality.is_empty() {
        "Białystok i gmina Wasilków".to_string()
    } else {
        locality.join(", ")
    };

    let mut hasher = DefaultHasher::new();
    item.guid.hash(&mut hasher);
    item.link.hash(&mut hasher);
    item.title.hash(&mut hasher);
    body.hash(&mut hasher);
    schedule.start.hash(&mut hasher);
    schedule.end.hash(&mut hasher);

    Some(UnifiedAlert {
        source: AlertSource::WodociagiBialostockie,
        startDate: schedule.start.map(format_datetime),
        endDate: schedule.end.map(format_datetime),
        message: Some(format!("{}: {}\n{}", outage_kind(item), item.title, body)),
        location: Some(format!("Miejscowość: {location}")),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(format!("wodociagi_bialostockie_{:x}", hasher.finish())),
    })
}

#[async_trait]
impl AlertProvider for WodociagiBialostockieProvider {
    fn id(&self) -> String {
        "wodociagi_bialostockie".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::WodociagiBialostockie
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

        let items = match fetch_feed(client).await {
            Ok(items) => items,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("Wodociągi Białostockie RSS error: {error}")],
                );
            }
        };

        let mut seen = std::collections::HashSet::new();
        let mut alerts = Vec::new();
        let mut errors = Vec::new();
        let now_utc = Utc::now();

        for item in items {
            let identity = if item.guid.trim().is_empty() {
                item.link.trim()
            } else {
                item.guid.trim()
            };
            if !identity.is_empty() && !seen.insert(identity.to_string()) {
                continue;
            }

            let body = html_to_text(&item.description);
            let body = if body.is_empty() {
                match fetch_article_body(client, &item.link).await {
                    Ok(body) => body,
                    Err(error) => {
                        errors.push(format!(
                            "Wodociągi Białostockie article error for {}: {error}",
                            item.link
                        ));
                        continue;
                    }
                }
            } else {
                body
            };

            if let Some(alert) = parse_notice(&item, &body, settings, now_utc) {
                alerts.push(alert);
            }
        }

        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "Tue, 20 Oct 2026 09:00:00 +0200";

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc2822(NOW)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn sample_item(title: &str, description: &str, category: &str) -> RssItem {
        RssItem {
            title: title.to_string(),
            link: "https://www.wobi.pl/aktualnosci/awarie-i-remonty/informacje-o-wylaczeniach/1880-test.html".to_string(),
            guid: "https://www.wobi.pl/aktualnosci/awarie-i-remonty/informacje-o-wylaczeniach/1880-test.html".to_string(),
            pub_date: NOW.to_string(),
            description: description.to_string(),
            categories: vec![category.to_string()],
        }
    }

    fn address(city_name: &str, commune: &str, city_id: u64, street_name: &str) -> AddressEntry {
        AddressEntry {
            city_name: city_name.to_string(),
            commune: commune.to_string(),
            city_id: Some(city_id),
            street_name: street_name.to_string(),
            street_name_1: street_name.to_string(),
            voivodeship: "podlaskie".to_string(),
            is_active: true,
            ..AddressEntry::default()
        }
    }

    #[test]
    fn parses_rss_items_and_html_cdata_description() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
            <rss version="2.0"><channel><title>WOBI</title>
                <item><title>Planowane wyłączenia wody</title>
                    <link>https://www.wobi.pl/aktualnosci/awarie-i-remonty/1-test.html</link>
                    <guid isPermaLink="true">https://www.wobi.pl/aktualnosci/awarie-i-remonty/1-test.html</guid>
                    <description><![CDATA[<p>Przerwa przy ul. Chmielnej.</p>]]></description>
                    <category>Wyłączenia</category>
                    <pubDate>Tue, 20 Oct 2026 07:23:25 +0200</pubDate>
                </item>
                <item><title>Awaria wodociągowa</title>
                    <link>https://www.wobi.pl/aktualnosci/awarie-i-remonty/awarie/2-test.html</link>
                    <guid isPermaLink="true">https://www.wobi.pl/aktualnosci/awarie-i-remonty/awarie/2-test.html</guid>
                    <description><![CDATA[<p>Awaria przy ul. Palmowej.</p>]]></description>
                    <category>Awarie</category>
                    <pubDate>Tue, 20 Oct 2026 08:00:00 +0200</pubDate>
                </item>
            </channel></rss>"#;
        let items = parse_rss(xml).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].categories, vec!["Wyłączenia"]);
        assert_eq!(
            html_to_text(&items[0].description),
            "Przerwa przy ul. Chmielnej."
        );
    }

    #[test]
    fn empty_feed_is_a_successful_empty_result() {
        let xml = r#"<rss version="2.0"><channel><title>WOBI</title></channel></rss>"#;
        assert!(parse_rss(xml).unwrap().is_empty());
    }

    #[test]
    fn parses_planned_bialystok_date_time_and_multiple_streets() {
        let schedule = extract_schedule(
            "Wodociągi informują, że w dniu 23.10.2026 r. w godz. 8.00-14.00 nastąpi przerwa przy ulicach Chmielnej i Wąskiej 15.",
            Some(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()),
        );
        assert_eq!(
            schedule.start.unwrap().format("%Y-%m-%d %H:%M").to_string(),
            "2026-10-23 08:00"
        );
        assert_eq!(
            schedule.end.unwrap().format("%Y-%m-%d %H:%M").to_string(),
            "2026-10-23 14:00"
        );
        assert!(street_matches_notice(
            "Planowane wyłączenie przy ulicach Chmielnej i Wąskiej 15",
            &address("Białystok", "Białystok", 922410, "Wąska")
        ));
    }

    #[test]
    fn parses_month_name_failure_and_repair_estimate() {
        let schedule = extract_schedule(
            "6 sierpnia, godz. 8:20 awaria wodociągowa. Przewidywany czas usunięcia awarii: godz. 15:00.",
            Some(NaiveDate::from_ymd_opt(2026, 8, 6).unwrap()),
        );
        assert_eq!(
            schedule.start.unwrap().format("%Y-%m-%d %H:%M").to_string(),
            "2026-08-06 08:20"
        );
        assert_eq!(
            schedule.end.unwrap().format("%Y-%m-%d %H:%M").to_string(),
            "2026-08-06 15:00"
        );

        let start_only = extract_schedule(
            "6 sierpnia, godz. 8:20 awaria wodociągowa. Przewidywany czas usunięcia awarii nie podano.",
            Some(NaiveDate::from_ymd_opt(2026, 8, 6).unwrap()),
        );
        assert_eq!(
            start_only
                .start
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-08-06 08:20"
        );
        assert_eq!(start_only.end, None);
    }

    #[test]
    fn handles_overnight_and_24_hour_end_time() {
        let overnight = extract_schedule(
            "Prace w dniu 22/23.10.2026 r. w godz. 22:00-06:00.",
            Some(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()),
        );
        assert_eq!(
            overnight
                .start
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-10-22 22:00"
        );
        assert_eq!(
            overnight.end.unwrap().format("%Y-%m-%d %H:%M").to_string(),
            "2026-10-23 06:00"
        );

        let end_of_day = extract_schedule(
            "Przerwa 23.10.2026 r. od godz. 20:00 do godz. 24:00.",
            Some(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()),
        );
        assert_eq!(
            end_of_day.end.unwrap().format("%Y-%m-%d %H:%M").to_string(),
            "2026-10-24 00:00"
        );
    }

    #[test]
    fn uses_publication_date_for_yearless_notice_without_inventing_start_time() {
        let publication_date = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let schedule = extract_schedule(
            "Planowana przerwa w dniu 23 października w godzinach porannych.",
            Some(publication_date),
        );
        assert_eq!(
            schedule.reference_date,
            Some(NaiveDate::from_ymd_opt(2026, 10, 23).unwrap())
        );
        assert_eq!(schedule.start, None);
        assert_eq!(schedule.end, None);

        let winter_publication = NaiveDate::from_ymd_opt(2026, 12, 20).unwrap();
        let rollover = extract_schedule(
            "Planowana przerwa 5 stycznia w godz. 7:00-8:00.",
            Some(winter_publication),
        );
        assert_eq!(
            rollover.start.unwrap().format("%Y-%m-%d %H:%M").to_string(),
            "2027-01-05 07:00"
        );
    }

    #[test]
    fn cancellation_and_explicit_resolution_are_ignored() {
        for title in [
            "Odwołane planowane wyłączenie wody",
            "Awaria wodociągowa (usunięta)",
        ] {
            let item = sample_item(title, "Przerwa przy ul. Chmielnej.", "Wyłączenia");
            assert!(
                parse_notice(
                    &item,
                    "Przerwa przy ul. Chmielnej.",
                    &Settings::default(),
                    now()
                )
                .is_none()
            );
        }
    }

    #[test]
    fn marks_only_matching_service_locality_and_street_as_local() {
        let item = sample_item(
            "Planowane wyłączenia wody - Białystok, ul. Chmielna/Wąska",
            "W dniu 23.10.2026 r. w godz. 8.00-14.00 nastąpi przerwa przy ulicach Chmielnej i Wąskiej 15.",
            "Wyłączenia",
        );
        let settings = Settings {
            addresses: vec![address("Białystok", "Białystok", 922410, "Wąska")],
            ..Settings::default()
        };
        let mut settings = settings;
        settings.addresses[0].house_no = "99".to_string();
        let alert =
            parse_notice(&item, &html_to_text(&item.description), &settings, now()).unwrap();
        assert_eq!(alert.is_local, Some(true));
        assert_eq!(alert.address_index, Some(0));
        assert!(alert.message.unwrap().contains("Wąskiej 15"));
    }

    #[test]
    fn matches_wasilkow_commune_and_named_villages() {
        let item = sample_item(
            "Planowane wyłączenie wody - Wasilków",
            "Przerwa w dostawie wody do miejscowości Jurowce.",
            "Wyłączenia",
        );
        let settings = Settings {
            addresses: vec![address("Jurowce", "Wasilków", 0, "")],
            ..Settings::default()
        };
        let alert =
            parse_notice(&item, &html_to_text(&item.description), &settings, now()).unwrap();
        assert_eq!(alert.is_local, Some(true));
        assert!(crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::WodociagiBialostockie,
            &settings.addresses[0]
        ));
        assert!(!crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::WodociagiBialostockie,
            &AddressEntry {
                city_name: "Wasilków".to_string(),
                voivodeship: "mazowieckie".to_string(),
                ..AddressEntry::default()
            }
        ));
    }

    #[test]
    fn validates_article_fallback_links_and_extracts_notice_paragraphs() {
        assert!(
            safe_article_url("https://www.wobi.pl/aktualnosci/awarie-i-remonty/1-test.html")
                .is_ok()
        );
        assert!(
            safe_article_url("https://example.com/aktualnosci/awarie-i-remonty/1-test.html")
                .is_err()
        );
        assert_eq!(
            extract_article_body("<main><h2>Tytuł</h2><p>Przerwa przy ul. Chmielnej.</p></main>")
                .unwrap(),
            "Przerwa przy ul. Chmielnej."
        );
    }

    #[test]
    fn source_categories_distinguish_failures_from_planned_work() {
        let failure = sample_item("Awaria wodociągowa - ul. Palmowa", "", "Awarie");
        let planned = sample_item("Planowane wyłączenia wody", "", "Wyłączenia");
        assert_eq!(outage_kind(&failure), "Awaria wodociągowa");
        assert_eq!(outage_kind(&planned), "Planowane wyłączenie wody");
    }

    #[test]
    fn publication_timestamp_is_interpreted_in_warsaw_time() {
        let date = publication_date_in_warsaw("Tue, 20 Oct 2026 07:23:25 +0200");
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()));
        assert!(publication_is_fresh(NOW, now()));
        let local = DateTime::parse_from_rfc2822("Tue, 20 Oct 2026 07:23:25 +0200")
            .unwrap()
            .with_timezone(&Warsaw);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(7, 23, 25).unwrap());
    }
}
