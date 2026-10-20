use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc, Weekday};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const WODOCIAGI_KIELECKIE_FEED_URL: &str = "https://wod-kiel.com.pl/komunikaty/feed/";

const ARTICLE_HOST: &str = "wod-kiel.com.pl";
const SCHEDULELESS_NOTICE_MAX_AGE: Duration = Duration::hours(24);

pub struct WodociagiKieleckieProvider;

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
    #[serde(rename = "pubDate", default)]
    pub_date: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Clone, Copy)]
struct ParsedDate {
    date: NaiveDate,
    year_is_explicit: bool,
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

fn number_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})(?:\.(\d{4}))?\b").unwrap())
}

fn month_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b(\d{1,2})\s+na\s+(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|wrzesnia|pazdziernika|listopada|grudnia)(?:\s+(\d{4}))?\b",
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
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})[:.](\d{2})\b").unwrap())
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

fn parse_date_parts(
    day: &str,
    month: u32,
    year: Option<&str>,
    publication_date: Option<NaiveDate>,
) -> Option<ParsedDate> {
    let year_is_explicit = year.is_some();
    let year = year
        .and_then(|value| value.parse::<i32>().ok())
        .or_else(|| publication_date.map(|date| date.year()))?;
    let date = NaiveDate::from_ymd_opt(year, month, day.parse().ok()?)?;
    Some(ParsedDate {
        date,
        year_is_explicit,
    })
}

fn add_date(dates: &mut Vec<(usize, ParsedDate)>, position: usize, date: ParsedDate) {
    if let Some((_, existing)) = dates
        .iter_mut()
        .find(|(_, existing)| existing.date == date.date)
    {
        existing.year_is_explicit |= date.year_is_explicit;
        return;
    }
    dates.push((position, date));
}

fn weekday_in_text(text: &str) -> Option<Weekday> {
    let normalized = normalize_polish(text);
    let re = Regex::new(
        r"\b(?:w|we)\s+(poniedzialek|poniedzialku|wtorek|wtorku|sroda|srode|czwartek|czwartku|piatek|piatku|sobota|sobote|soboty|niedziela|niedziele)\b",
    )
    .unwrap();
    let weekday = re.captures(&normalized)?.get(1)?.as_str();
    match weekday {
        "poniedzialek" | "poniedzialku" => Some(Weekday::Mon),
        "wtorek" | "wtorku" => Some(Weekday::Tue),
        "sroda" | "srode" => Some(Weekday::Wed),
        "czwartek" | "czwartku" => Some(Weekday::Thu),
        "piatek" | "piatku" => Some(Weekday::Fri),
        "sobota" | "sobote" | "soboty" => Some(Weekday::Sat),
        "niedziela" | "niedziele" => Some(Weekday::Sun),
        _ => None,
    }
}

fn extract_dates(text: &str, publication_date: Option<NaiveDate>) -> Vec<ParsedDate> {
    let normalized = normalize_polish(text);
    let mut dates = Vec::new();

    for captures in month_range_regex().captures_iter(&normalized) {
        let Some(month) = captures
            .get(3)
            .and_then(|value| month_number(value.as_str()))
        else {
            continue;
        };
        let year = captures.get(4).map(|value| value.as_str());
        if let Some(date) = parse_date_parts(&captures[1], month, year, publication_date) {
            add_date(&mut dates, captures.get(1).unwrap().start(), date);
        }
        if let Some(date) = parse_date_parts(&captures[2], month, year, publication_date) {
            add_date(&mut dates, captures.get(2).unwrap().start(), date);
        }
    }

    for captures in number_date_regex().captures_iter(&normalized) {
        let Some(month) = captures
            .get(2)
            .and_then(|value| value.as_str().parse().ok())
        else {
            continue;
        };
        if let Some(date) = parse_date_parts(
            &captures[1],
            month,
            captures.get(3).map(|value| value.as_str()),
            publication_date,
        ) {
            add_date(&mut dates, captures.get(1).unwrap().start(), date);
        }
    }

    for captures in month_date_regex().captures_iter(&normalized) {
        let Some(month) = captures
            .get(2)
            .and_then(|value| month_number(value.as_str()))
        else {
            continue;
        };
        if let Some(date) = parse_date_parts(
            &captures[1],
            month,
            captures.get(3).map(|value| value.as_str()),
            publication_date,
        ) {
            add_date(&mut dates, captures.get(1).unwrap().start(), date);
        }
    }

    dates.sort_by_key(|(position, _)| *position);
    let mut dates: Vec<_> = dates.into_iter().map(|(_, date)| date).collect();

    if dates.is_empty()
        && let (Some(publication_date), Some(weekday)) = (publication_date, weekday_in_text(text))
        && publication_date.weekday() == weekday
    {
        dates.push(ParsedDate {
            date: publication_date,
            year_is_explicit: false,
        });
    }

    dates
}

fn parse_time(hour: &str, minute: &str) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
}

fn extract_times(text: &str) -> Vec<NaiveTime> {
    let without_numeric_dates = number_date_regex().replace_all(text, " ");
    let mut times = Vec::new();
    for captures in time_regex().captures_iter(&without_numeric_dates) {
        if let Some(time) = parse_time(&captures[1], &captures[2])
            && !times.contains(&time)
        {
            times.push(time);
        }
    }
    times
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn extract_schedule(
    text: &str,
    publication_date: Option<NaiveDate>,
) -> (Option<String>, Option<String>) {
    let dates = extract_dates(text, publication_date);
    let times = extract_times(text);
    let Some(start_date) = dates.first().copied() else {
        return (None, None);
    };

    let midnight = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    let end_of_day = NaiveTime::from_hms_opt(23, 59, 0).unwrap();
    let start_time = times.first().copied().unwrap_or(midnight);
    let start = start_date.date.and_time(start_time);

    let mut end_date = dates
        .get(1)
        .copied()
        .map(|date| date.date)
        .unwrap_or(start_date.date);
    let end_time = times.get(1).copied().unwrap_or(end_of_day);
    if end_date < start_date.date
        && !dates
            .get(1)
            .is_some_and(|date| start_date.year_is_explicit && date.year_is_explicit)
    {
        end_date =
            NaiveDate::from_ymd_opt(start_date.date.year() + 1, end_date.month(), end_date.day())
                .unwrap_or(end_date);
    }
    if end_date == start_date.date && times.len() > 1 && end_time <= start_time {
        end_date += Duration::days(1);
    }

    (
        Some(format_datetime(start)),
        Some(format_datetime(end_date.and_time(end_time))),
    )
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

fn is_expired(end_date: Option<&str>, now: NaiveDateTime) -> bool {
    end_date
        .and_then(|value| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok())
        .is_some_and(|end| end < now)
}

fn is_resolved(text: &str) -> bool {
    let text = normalize_polish(text);
    [
        "usunieta awaria",
        "awaria usunieta",
        "awaria zostala usunieta",
        "usunieto awarie",
        "modernizacja zakonczona",
        "prace zostaly zakonczone",
        "prace zakonczone",
        "zakonczono prace",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn outage_kind(text: &str) -> Option<&'static str> {
    let text = normalize_polish(text);
    let is_water_notice = text.contains("awari")
        || text.contains("przerw")
        || text.contains("wylaczenie wody")
        || text.contains("zamkniecie wody")
        || text.contains("brak wody")
        || text.contains("obnizenie cisnienia")
        || text.contains("sieci wodociagowej");
    if !is_water_notice {
        return None;
    }

    if text.contains("awari") {
        Some("Awaria wodociągowa")
    } else if text.contains("planowan") || text.contains("prac") || text.contains("modernizac") {
        Some("Planowana przerwa w dostawie wody")
    } else {
        Some("Przerwa w dostawie wody")
    }
}

fn is_outage_candidate(item: &RssItem) -> bool {
    outage_kind(&format!("{} {}", item.title, item.description)).is_some()
}

fn element_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_article_body(html: &str) -> Result<String, String> {
    let document = Html::parse_document(html);
    let body_selector =
        Selector::parse(".pe-article-content .pf-content").map_err(|error| error.to_string())?;
    let body = document
        .select(&body_selector)
        .next()
        .map(element_text)
        .filter(|body| !body.is_empty())
        .ok_or_else(|| "article body .pe-article-content .pf-content was not found".to_string())?;
    Ok(body)
}

fn parse_rss(xml: &str) -> Result<Vec<RssItem>, String> {
    let feed: RssFeed = quick_xml::de::from_str(xml).map_err(|error| error.to_string())?;
    Ok(feed.channel.items)
}

fn safe_article_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|error| error.to_string())?;
    if url.scheme() != "https" || url.host_str() != Some(ARTICLE_HOST) {
        return Err("RSS item link is outside the official Wodociągi Kieleckie site".to_string());
    }
    Ok(url)
}

fn locality_labels(text: &str) -> Vec<&'static str> {
    let normalized = normalize_polish(text);
    let words: Vec<_> = normalized
        .split(|character: char| !character.is_alphanumeric())
        .collect();
    let mut labels = Vec::new();
    for (word, label) in [
        ("kielce", "Kielce"),
        ("kielc", "Kielce"),
        ("maslow", "Masłów"),
        ("maslowie", "Masłów"),
        ("nowiny", "Nowiny"),
        ("nowinach", "Nowiny"),
        ("zagnansk", "Zagnańsk"),
        ("zagnansku", "Zagnańsk"),
        ("gorno", "Górno"),
        ("gornie", "Górno"),
        ("cedzyna", "Cedzyna"),
        ("cedzynie", "Cedzyna"),
    ] {
        if words.contains(&word) && !labels.contains(&label) {
            labels.push(label);
        }
    }
    if labels.is_empty() {
        labels.push("Kielce");
    }
    labels
}

fn normalize_address(value: &str) -> String {
    normalize_polish(value)
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn address_matches_locality(label: &str, address: &AddressEntry) -> bool {
    let city = normalize_address(&address.city_name);
    let commune = normalize_address(&address.commune);
    match label {
        "Kielce" => address.city_id == Some(945930) || city.starts_with("kielce"),
        "Masłów" => commune.starts_with("maslow"),
        "Nowiny" => commune.starts_with("nowiny") || commune.starts_with("sitkowka nowiny"),
        "Zagnańsk" => commune.starts_with("zagnansk"),
        "Górno" => commune.starts_with("gorno"),
        "Cedzyna" => city.starts_with("cedzyna") || commune.starts_with("gorno"),
        _ => false,
    }
}

fn locality_matches_notice(text: &str, address: &AddressEntry) -> bool {
    locality_labels(text)
        .iter()
        .any(|label| address_matches_locality(label, address))
}

fn street_matches_notice(text: &str, address: &AddressEntry) -> bool {
    let mut candidates = vec![address.street_name.as_str(), address.street_name_1.as_str()];
    if let Some(second) = address.street_name_2.as_deref() {
        candidates.push(second);
    }

    let notice_words: Vec<_> = normalize_polish(text)
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_string)
        .collect();
    candidates.into_iter().any(|candidate| {
        let candidate = crate::utils::strip_street_prefixes(candidate.trim());
        let tokens: Vec<_> = normalize_polish(candidate)
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .map(str::to_string)
            .collect();
        !tokens.is_empty()
            && tokens.iter().all(|token| {
                let stem = if token.ends_with('a') && token.chars().count() > 5 {
                    &token[..token.len() - 1]
                } else {
                    token.as_str()
                };
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
    let kind = outage_kind(&combined)?;
    let publication_date = publication_date_in_warsaw(&item.pub_date);
    let (start_date, end_date) = extract_schedule(&combined, publication_date);
    let now_local = now_utc.with_timezone(&Warsaw).naive_local();
    if is_expired(end_date.as_deref(), now_local) {
        return None;
    }
    if start_date.is_none() && !publication_is_fresh(&item.pub_date, now_utc) {
        return None;
    }

    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::WodociagiKieleckie,
                    address,
                )
                && address_matches_notice(&combined, address)
        })
        .map(|(index, _)| index);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    item.link.hash(&mut hasher);
    body.hash(&mut hasher);
    start_date.hash(&mut hasher);
    end_date.hash(&mut hasher);

    Some(UnifiedAlert {
        source: AlertSource::WodociagiKieleckie,
        startDate: start_date,
        endDate: end_date,
        message: Some(format!("{kind}: {body}")),
        location: Some(format!(
            "Miejscowość: {}",
            locality_labels(&combined).join(", ")
        )),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(format!("wodociagi_kieleckie_{:x}", hasher.finish())),
    })
}

async fn fetch_feed(client: &Client) -> Result<Vec<RssItem>, String> {
    let xml = crate::utils::retry(
        || async {
            let response = client
                .get(WODOCIAGI_KIELECKIE_FEED_URL)
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

async fn fetch_article(client: &Client, url: &Url) -> Result<String, String> {
    crate::utils::retry(
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
    .await
}

#[async_trait]
impl AlertProvider for WodociagiKieleckieProvider {
    fn id(&self) -> String {
        "wodociagi_kieleckie".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::WodociagiKieleckie
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
                    vec![format!("Wodociągi Kieleckie RSS error: {error}")],
                );
            }
        };

        let candidates: Vec<_> = items.into_iter().filter(is_outage_candidate).collect();
        let article_requests = candidates.into_iter().map(|item| async move {
            let result = match safe_article_url(&item.link) {
                Ok(url) => fetch_article(client, &url).await,
                Err(error) => Err(error),
            };
            (item, result)
        });

        let mut alerts = Vec::new();
        let mut errors = Vec::new();
        let now_utc = Utc::now();
        for (item, result) in futures::future::join_all(article_requests).await {
            match result {
                Ok(html) => match extract_article_body(&html) {
                    Ok(body) => {
                        if let Some(alert) = parse_notice(&item, &body, settings, now_utc) {
                            alerts.push(alert);
                        }
                    }
                    Err(error) => errors.push(format!(
                        "Wodociągi Kieleckie article parse error for {}: {error}",
                        item.link
                    )),
                },
                Err(error) => errors.push(format!(
                    "Wodociągi Kieleckie article request error for {}: {error}",
                    item.link
                )),
            }
        }

        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const FEED_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
    <rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/">
      <channel>
        <title>Wodociągi Kieleckie – komunikaty</title>
        <item>
          <title>Przerwy w dostawie wody w Cedzynie</title>
          <link>https://wod-kiel.com.pl/przerwy-w-dostawie-wody-w-cedzynie/</link>
          <pubDate>Tue, 18 Aug 2026 06:12:31 +0000</pubDate>
          <description>W środę, 19 sierpnia 2026 r., w godz. 8:00–16:00 mogą wystąpić przerwy w dostawie wody w Cedzynie.</description>
        </item>
        <item>
          <title>Biuro Obsługi Klienta i Kasa nieczynne</title>
          <link>https://wod-kiel.com.pl/biuro-nieczynne/</link>
          <pubDate>Wed, 12 Aug 2026 12:03:35 +0000</pubDate>
          <description>Biuro Wodociągów Kieleckich będzie nieczynne.</description>
        </item>
      </channel>
    </rss>"#;

    fn settings(city: &str, commune: &str, street: &str, filter_by_house_no: bool) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Home".into(),
                city_name: city.into(),
                voivodeship: "Świętokrzyskie".into(),
                district: "kielecki".into(),
                commune: commune.into(),
                street_name: street.into(),
                street_name_1: street.into(),
                street_name_2: None,
                house_no: "15".into(),
                city_id: None,
                street_id: None,
                is_active: true,
            }],
            primary_address_index: Some(0),
            theme: None,
            language: Some("pl".into()),
            font_size: None,
            enabled_sources: None,
            notification_preferences: HashMap::new(),
            upcoming_notification_enabled: false,
            upcoming_notification_hours: 24,
            show_other_outages: true,
            filter_by_house_no,
        }
    }

    fn item(title: &str, pub_date: &str) -> RssItem {
        RssItem {
            title: title.to_string(),
            link: "https://wod-kiel.com.pl/example/".to_string(),
            pub_date: pub_date.to_string(),
            description: String::new(),
        }
    }

    fn utc_datetime(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn parses_official_category_rss_items_and_ignores_other_posts() {
        let items = parse_rss(FEED_FIXTURE).unwrap();
        assert_eq!(items.len(), 2);
        assert!(is_outage_candidate(&items[0]));
        assert!(!is_outage_candidate(&items[1]));
        assert_eq!(
            items[0].link,
            "https://wod-kiel.com.pl/przerwy-w-dostawie-wody-w-cedzynie/"
        );
    }

    #[test]
    fn parses_polish_dates_and_overnight_windows() {
        let publication_date = NaiveDate::from_ymd_opt(2026, 10, 13);
        let (start, end) = extract_schedule(
            "W nocy z 13 na 14 października 2026 r., w godzinach 22:00–7:00",
            publication_date,
        );
        assert_eq!(start.as_deref(), Some("2026-10-13T22:00:00"));
        assert_eq!(end.as_deref(), Some("2026-10-14T07:00:00"));

        let (start, end) = extract_schedule(
            "w środę, 19 sierpnia 2026 r., w godz. 8:00–16:00",
            publication_date,
        );
        assert_eq!(start.as_deref(), Some("2026-08-19T08:00:00"));
        assert_eq!(end.as_deref(), Some("2026-08-19T16:00:00"));

        let publication_date = NaiveDate::from_ymd_opt(2026, 5, 27);
        let (start, end) = extract_schedule(
            "w nocy z czwartku na piątek (28.05 na 29.05) w godz. 22:00 – 7:00",
            publication_date,
        );
        assert_eq!(start.as_deref(), Some("2026-05-28T22:00:00"));
        assert_eq!(end.as_deref(), Some("2026-05-29T07:00:00"));
    }

    #[test]
    fn relative_weekday_uses_publication_day_and_missing_end_is_same_day() {
        let publication_date = NaiveDate::from_ymd_opt(2026, 8, 5);
        let (start, end) = extract_schedule(
            "W środę przed południem konieczne było zamknięcie wody",
            publication_date,
        );
        assert_eq!(start.as_deref(), Some("2026-08-05T00:00:00"));
        assert_eq!(end.as_deref(), Some("2026-08-05T23:59:00"));

        let (start, end) = extract_schedule(
            "Planowane prace w dniu 20.10.2026 od godz. 08:00",
            publication_date,
        );
        assert_eq!(start.as_deref(), Some("2026-10-20T08:00:00"));
        assert_eq!(end.as_deref(), Some("2026-10-20T23:59:00"));
        assert_eq!(
            extract_schedule("Awaria bez daty i godziny", publication_date),
            (None, None)
        );
    }

    #[test]
    fn parses_full_article_html_and_matches_city_street_and_commune() {
        let html = r#"<main id="pe-maincontent"><div class="pe-article-content"><div class="pf-content">
          <p>Zakład Usług Komunalnych w Górnie informuje, że w środę, 19 sierpnia 2026 r., w godz. 8:00–16:00 mogą wystąpić przerwy w dostawie wody w miejscowości Cedzyna – ul. Lubrzanka, ul. Kielecka oraz wszystkie ulice wewnętrzne przy ul. Kieleckiej.</p>
        </div></div></main>"#;
        let body = extract_article_body(html).unwrap();
        assert!(body.contains("ul. Lubrzanka"));

        let item = item(
            "Przerwy w dostawie wody w Cedzynie",
            "Tue, 18 Aug 2026 06:12:31 +0000",
        );
        let now = utc_datetime("2026-08-18T12:00:00Z");
        let alert = parse_notice(
            &item,
            &body,
            &settings("Cedzyna", "Górno", "Lubrzanka", false),
            now,
        )
        .unwrap();
        assert_eq!(alert.startDate.as_deref(), Some("2026-08-19T08:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-08-19T16:00:00"));
        assert_eq!(
            alert.location.as_deref(),
            Some("Miejscowość: Górno, Cedzyna")
        );
        assert_eq!(alert.is_local, Some(true));
        assert!(alert.message.as_deref().unwrap().contains("ul. Kielecka"));

        let wrong_locality = settings("Nowiny", "Nowiny", "Lubrzanka", false);
        assert_eq!(
            parse_notice(&item, &body, &wrong_locality, now)
                .unwrap()
                .is_local,
            Some(false)
        );
    }

    #[test]
    fn excludes_resolved_posts_and_expired_schedules() {
        let resolved = item(
            "Usunięta awaria w rejonie ul. Zapolskiej",
            "Mon, 24 Aug 2026 09:00:05 +0000",
        );
        assert!(
            parse_notice(
                &resolved,
                "Awaria usunięta o godz. 12.15.",
                &settings("Kielce", "Kielce", "Zapolskiej", false),
                utc_datetime("2026-08-24T10:00:00Z"),
            )
            .is_none()
        );

        let completed = item(
            "Modernizacja sieci w rejonie skrzyżowania Jagiellońska/Krakowska",
            "Tue, 13 Oct 2026 06:54:12 +0000",
        );
        assert!(parse_notice(
            &completed,
            "W nocy z 13 na 14 października 2026 r. w godzinach 22:00–7:00. Modernizacja zakończona pomyślnie.",
            &settings("Kielce", "Kielce", "Krakowska", false),
            utc_datetime("2026-10-13T10:00:00Z"),
        )
        .is_none());

        let past = item("Przerwa w dostawie wody", "Tue, 18 Aug 2026 06:12:31 +0000");
        assert!(
            parse_notice(
                &past,
                "19 sierpnia 2026 r. w godz. 8:00–16:00 przerwa w dostawie wody.",
                &settings("Kielce", "Kielce", "Krakowska", false),
                utc_datetime("2026-08-20T12:00:00Z"),
            )
            .is_none()
        );
    }

    #[test]
    fn scheduleless_notices_have_a_short_publication_freshness_window() {
        let notice = item("Usuwamy awarię", "Mon, 19 Oct 2026 09:00:00 +0000");
        let fresh = parse_notice(
            &notice,
            "Awaria przewodu wodociągowego przy ul. Krakowskiej.",
            &settings("Kielce", "Kielce", "Krakowskiej", false),
            utc_datetime("2026-10-19T10:00:00Z"),
        );
        assert!(fresh.is_some());
        let stale = parse_notice(
            &notice,
            "Awaria przewodu wodociągowego przy ul. Krakowskiej.",
            &settings("Kielce", "Kielce", "Krakowskiej", false),
            utc_datetime("2026-10-20T10:00:01Z"),
        );
        assert!(stale.is_none());
    }

    #[test]
    fn house_number_preference_does_not_change_street_matching_or_notice_text() {
        let notice = item(
            "Usuwamy awarię na Ślichowicach",
            "Wed, 05 Aug 2026 09:16:57 +0000",
        );
        let body = "W środę przed południem konieczne było zamknięcie wody przy ul. Puscha 15 i 17. Mogą wystąpić przerwy w dostawie wody.";
        let now = utc_datetime("2026-08-05T12:00:00Z");
        let without_filter = parse_notice(
            &notice,
            body,
            &settings("Kielce", "Kielce", "Puscha", false),
            now,
        )
        .unwrap();
        let with_filter = parse_notice(
            &notice,
            body,
            &settings("Kielce", "Kielce", "Puscha", true),
            now,
        )
        .unwrap();
        assert_eq!(without_filter.is_local, Some(true));
        assert_eq!(with_filter.is_local, Some(true));
        assert!(
            with_filter
                .message
                .as_deref()
                .unwrap()
                .contains("Puscha 15 i 17")
        );
    }

    #[test]
    fn rejects_article_urls_outside_the_official_host() {
        assert!(safe_article_url("https://wod-kiel.com.pl/notices/").is_ok());
        assert!(safe_article_url("https://example.com/notices/").is_err());
        assert!(safe_article_url("http://wod-kiel.com.pl/notices/").is_err());
    }

    #[tokio::test]
    #[ignore = "requires live access to the official Wodociągi Kieleckie RSS and article pages"]
    async fn fetches_current_official_feed_with_awaria_http_client() {
        let client = crate::network_state::NetworkState::build_client().unwrap();
        let settings = Settings::default();
        let (_alerts, errors) = WodociagiKieleckieProvider
            .fetch(&client, &client, &settings, None)
            .await;
        assert!(errors.is_empty(), "{errors:?}");
    }
}
