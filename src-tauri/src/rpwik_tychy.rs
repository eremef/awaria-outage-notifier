use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const RPWIK_TYCHY_PAGE_URL: &str = "https://rpwik.tychy.pl/komunikaty/";

pub struct RpwikTychyProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Notice {
    title: String,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimedValue {
    index: usize,
    time: NaiveTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeRole {
    Start,
    End,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Schedule {
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    start_time: Option<NaiveTime>,
    end_time: Option<NaiveTime>,
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn element_text(element: ElementRef<'_>) -> String {
    collapse_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn normalized(text: &str) -> String {
    crate::utils::clean_text(text)
        .replace(['–', '—', '−'], "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn numeric_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:(?P<range_start>\d{1,2})\s*[-–—]\s*(?P<range_end>\d{1,2})[./](?P<range_month>\d{1,2})|(?P<day>\d{1,2})[./-](?P<month>\d{1,2}))[./-](?P<year>\d{4})(?:r\.?|\b)",
        )
        .unwrap()
    })
}

fn written_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|września|wrzesnia|października|pazdziernika|listopada|grudnia)\s+(\d{4})(?:r\.|\b)",
        )
        .unwrap()
    })
}

fn month_number(month: &str) -> Option<u32> {
    match crate::utils::clean_text(month).as_str() {
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

fn notice_dates(text: &str) -> Vec<NaiveDate> {
    let mut hits: Vec<(usize, Vec<NaiveDate>)> = Vec::new();

    for captures in numeric_date_regex().captures_iter(text) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        let Some(year) = captures
            .name("year")
            .and_then(|value| value.as_str().parse::<i32>().ok())
        else {
            continue;
        };
        let dates = if let (Some(day), Some(end_day), Some(month)) = (
            captures.name("range_start"),
            captures.name("range_end"),
            captures.name("range_month"),
        ) {
            let Some(day) = day.as_str().parse::<u32>().ok() else {
                continue;
            };
            let Some(end_day) = end_day.as_str().parse::<u32>().ok() else {
                continue;
            };
            let Some(month) = month.as_str().parse::<u32>().ok() else {
                continue;
            };
            let (Some(start), Some(end)) = (
                NaiveDate::from_ymd_opt(year, month, day),
                NaiveDate::from_ymd_opt(year, month, end_day),
            ) else {
                continue;
            };
            vec![start, end]
        } else {
            let (Some(day), Some(month)) = (
                captures
                    .name("day")
                    .and_then(|value| value.as_str().parse::<u32>().ok()),
                captures
                    .name("month")
                    .and_then(|value| value.as_str().parse::<u32>().ok()),
            ) else {
                continue;
            };
            let Some(date) = NaiveDate::from_ymd_opt(year, month, day) else {
                continue;
            };
            vec![date]
        };
        hits.push((matched.start(), dates));
    }

    for captures in written_date_regex().captures_iter(text) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        let Some(day) = captures
            .get(1)
            .and_then(|value| value.as_str().parse::<u32>().ok())
        else {
            continue;
        };
        let Some(month) = captures
            .get(2)
            .and_then(|value| month_number(value.as_str()))
        else {
            continue;
        };
        let Some(year) = captures
            .get(3)
            .and_then(|value| value.as_str().parse::<i32>().ok())
        else {
            continue;
        };
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            hits.push((matched.start(), vec![date]));
        }
    }

    hits.sort_by_key(|(index, _)| *index);
    let mut dates = Vec::new();
    for (_, hit_dates) in hits {
        for date in hit_dates {
            if dates.last() != Some(&date) && !dates.contains(&date) {
                dates.push(date);
            }
        }
    }
    dates
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(\d{1,2})[:.](\d{2})\b").unwrap())
}

fn notice_times(text: &str) -> Vec<TimedValue> {
    time_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            let matched = captures.get(0)?;
            // A numeric date such as `08.10.2026` also matches the time shape
            // `08.10`; discard that prefix when it is followed by the year.
            let suffix_bytes = &text.as_bytes()[matched.end()..];
            if suffix_bytes.len() >= 5
                && matches!(suffix_bytes[0], b'.' | b'/' | b'-')
                && suffix_bytes[1..5].iter().all(|byte| byte.is_ascii_digit())
            {
                return None;
            }
            let hour = captures.get(1)?.as_str().parse::<u32>().ok()?;
            let minute = captures.get(2)?.as_str().parse::<u32>().ok()?;
            Some(TimedValue {
                index: matched.start(),
                time: NaiveTime::from_hms_opt(hour, minute, 0)?,
            })
        })
        .collect()
}

fn single_time_role(text: &str, value: TimedValue) -> TimeRole {
    let before = normalized(&text[..value.index]);
    let tail = before.trim_end_matches(|character: char| !character.is_alphanumeric());
    if [" do", " do godz", " do godziny"]
        .iter()
        .any(|suffix| tail.ends_with(suffix))
    {
        TimeRole::End
    } else if [" od", " od godz", " od godziny"]
        .iter()
        .any(|suffix| tail.ends_with(suffix))
    {
        TimeRole::Start
    } else {
        TimeRole::Unknown
    }
}

fn parse_schedule(text: &str) -> Schedule {
    let dates = notice_dates(text);
    let times = notice_times(text);
    let (start_date, end_date) = (dates.first().copied(), dates.last().copied());

    if times.len() >= 2 {
        let start_time = times.first().map(|value| value.time);
        let end_time = times.last().map(|value| value.time);
        let mut end_date = end_date.or(start_date);
        if dates.len() == 1
            && let (Some(start), Some(end), Some(date)) = (start_time, end_time, end_date)
            && end < start
        {
            end_date = Some(date + Duration::days(1));
        }
        return Schedule {
            start_date,
            end_date,
            start_time,
            end_time,
        };
    }

    if let Some(single) = times.first().copied() {
        return match single_time_role(text, single) {
            TimeRole::Start => Schedule {
                start_date,
                end_date: None,
                start_time: Some(single.time),
                end_time: None,
            },
            TimeRole::End => Schedule {
                start_date,
                end_date: end_date.or(start_date),
                start_time: None,
                end_time: Some(single.time),
            },
            TimeRole::Unknown => Schedule {
                start_date,
                end_date,
                start_time: None,
                end_time: None,
            },
        };
    }

    Schedule {
        start_date,
        end_date,
        start_time: None,
        end_time: None,
    }
}

fn format_datetime(date: NaiveDate, time: Option<NaiveTime>) -> String {
    match time {
        Some(time) => date.and_time(time).format("%Y-%m-%dT%H:%M:%S").to_string(),
        None => date.format("%Y-%m-%d").to_string(),
    }
}

fn schedule_values(schedule: Schedule) -> (Option<String>, Option<String>) {
    (
        schedule
            .start_date
            .map(|date| format_datetime(date, schedule.start_time)),
        schedule
            .end_date
            .map(|date| format_datetime(date, schedule.end_time)),
    )
}

fn schedule_is_expired(schedule: Schedule, now_utc: DateTime<Utc>) -> bool {
    let Some(end_date) = schedule.end_date else {
        return false;
    };
    let end = schedule
        .end_time
        .map(|time| end_date.and_time(time))
        .unwrap_or_else(|| end_date.and_hms_opt(23, 59, 59).unwrap());
    now_utc.with_timezone(&Warsaw).naive_local() > end
}

fn parse_notice(
    article: ElementRef<'_>,
    title_selector: &Selector,
    paragraph_selector: &Selector,
) -> Result<Notice, String> {
    let title = article
        .select(title_selector)
        .next()
        .map(element_text)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "RPWiK Tychy notice has no heading".to_string())?;
    let paragraphs = article
        .select(paragraph_selector)
        .map(element_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();
    if paragraphs.is_empty() {
        return Err(format!("RPWiK Tychy notice {title:?} has no body"));
    }
    let text = format!("{title}\n{}", paragraphs.join("\n"));
    Ok(Notice { title, text })
}

fn page_heading_is_komunikaty(document: &Html) -> bool {
    let Ok(selector) = Selector::parse("h1, h2, h3") else {
        return false;
    };
    document
        .select(&selector)
        .map(element_text)
        .any(|heading| normalized(&heading).contains("komunikaty"))
}

fn parse_notices(html: &str) -> Result<Vec<Notice>, String> {
    let document = Html::parse_document(html);
    let article_selector = Selector::parse("article.posts-grid__item")
        .map_err(|error| format!("invalid article selector: {error}"))?;
    let title_selector = Selector::parse("h1, h2, h3, h4")
        .map_err(|error| format!("invalid title selector: {error}"))?;
    let paragraph_selector =
        Selector::parse("p").map_err(|error| format!("invalid paragraph selector: {error}"))?;

    let articles = document.select(&article_selector).collect::<Vec<_>>();
    if articles.is_empty() {
        return if page_heading_is_komunikaty(&document) {
            Ok(Vec::new())
        } else {
            Err("RPWiK Tychy Komunikaty archive structure was not found".to_string())
        };
    }

    articles
        .into_iter()
        .map(|article| parse_notice(article, &title_selector, &paragraph_selector))
        .collect()
}

fn strip_commune_prefix(value: &str) -> &str {
    let value = value.trim();
    for prefix in ["gmina ", "gminy ", "gm. ", "gm "] {
        if value.to_lowercase().starts_with(prefix) {
            return value[prefix.len()..].trim();
        }
    }
    value
}

fn label_matches_text(text: &str, label: &str) -> bool {
    let words = normalized(strip_commune_prefix(label))
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if words.is_empty() {
        return false;
    }
    let pattern = words
        .iter()
        .map(|word| {
            let stem_len = word.chars().count().saturating_sub(1).max(3);
            let stem = word.chars().take(stem_len).collect::<String>();
            format!("{}\\p{{L}}*", regex::escape(&stem))
        })
        .collect::<Vec<_>>()
        .join(r"[^\p{L}\p{N}]+");
    Regex::new(&format!(
        r"(?i)(?:^|[^\p{{L}}\p{{N}}]){}(?:[^\p{{L}}\p{{N}}]|$)",
        pattern
    ))
    .is_ok_and(|regex| regex.is_match(&normalized(text)))
}

fn address_locality_matches(location: &str, address: &AddressEntry) -> bool {
    let mut candidates = vec![address.city_name.as_str(), address.commune.as_str()];
    if normalized(&address.city_name).starts_with("laziska gorne") {
        candidates.push("Łaziska");
    }
    candidates
        .into_iter()
        .filter(|candidate| !candidate.trim().is_empty())
        .any(|candidate| label_matches_text(location, candidate))
}

fn address_street_matches(location: &str, address: &AddressEntry) -> bool {
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let candidates = candidates
        .iter()
        .map(|candidate| {
            candidate
                .split_whitespace()
                .filter(|word| {
                    !matches!(
                        normalized(word).as_str(),
                        "ul" | "ulica" | "al" | "aleja" | "pl" | "plac"
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|candidate| !candidate.trim().is_empty())
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return true;
    }
    candidates
        .iter()
        .any(|candidate| label_matches_text(location, candidate))
}

fn location_segments(location: &str) -> Vec<String> {
    static STREET_GROUP: OnceLock<Regex> = OnceLock::new();
    let separator =
        STREET_GROUP.get_or_init(|| Regex::new(r"(?i),\s*ulic(?:a|y|e)?\s*:?\s*").unwrap());
    location
        .lines()
        .flat_map(|line| {
            separator
                .split(line)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Re-evaluate an article's locality and street against one configured address.
/// A matching street in another named locality must not make the alert local.
pub fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    location_segments(location).iter().any(|segment| {
        address_locality_matches(segment, address) && address_street_matches(segment, address)
    })
}

/// Keep house-number filtering in the shared `Settings.filter_by_house_no` path.
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

fn notice_identity(notice: &Notice, start: &Option<String>, end: &Option<String>) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized(&notice.title).hash(&mut hasher);
    normalized(&notice.text).hash(&mut hasher);
    start.hash(&mut hasher);
    end.hash(&mut hasher);
    format!("rpwik_tychy_{:x}", hasher.finish())
}

fn build_alert(
    notice: &Notice,
    settings: &Settings,
    now_utc: DateTime<Utc>,
) -> Option<UnifiedAlert> {
    let schedule = parse_schedule(&notice.text);
    // This page retains historic articles. Without any service date the item
    // cannot be distinguished from a stale notice, so do not publish it as live.
    if schedule.start_date.is_none() && schedule.end_date.is_none() {
        return None;
    }
    if schedule_is_expired(schedule, now_utc) {
        return None;
    }
    let matched_address = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::RpwikTychy,
                    address,
                )
                && location_matches_address(&notice.text, address)
        })
        .map(|(index, _)| index);
    let (start_date, end_date) = schedule_values(schedule);
    let hash = notice_identity(notice, &start_date, &end_date);

    Some(UnifiedAlert {
        source: AlertSource::RpwikTychy,
        startDate: start_date,
        endDate: end_date,
        message: Some(notice.text.clone()),
        location: Some(notice.text.clone()),
        address_index: matched_address,
        is_local: Some(matched_address.is_some()),
        hash: Some(hash),
    })
}

/// Parses all populated RPWiK Tychy water notices from the official archive.
pub fn parse_rpwik_tychy_html(
    html: &str,
    settings: &Settings,
    now_utc: DateTime<Utc>,
) -> Result<Vec<UnifiedAlert>, String> {
    let notices = parse_notices(html)?;
    let mut alerts = Vec::new();
    let mut seen = HashSet::new();
    for notice in notices {
        if let Some(alert) = build_alert(&notice, settings, now_utc)
            && seen.insert(alert.hash.clone())
        {
            alerts.push(alert);
        }
    }
    Ok(alerts)
}

async fn fetch_page(client: &Client) -> Result<String, String> {
    let response = client
        .get(RPWIK_TYCHY_PAGE_URL)
        .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response.text().await.map_err(|error| error.to_string())
}

#[async_trait]
impl AlertProvider for RpwikTychyProvider {
    fn id(&self) -> String {
        "rpwik_tychy".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::RpwikTychy
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
        let html = match fetch_page(client).await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("RPWiK Tychy notices error: {error}")],
                );
            }
        };
        match parse_rpwik_tychy_html(&html, settings, Utc::now()) {
            Ok(alerts) => (alerts, Vec::new()),
            Err(error) => (
                Vec::new(),
                vec![format!("RPWiK Tychy notices error: {error}")],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const ARCHIVE_HTML: &str = r#"
      <h2 class="section-title__title">Komunikaty</h2>
      <article class="posts-grid__item">
        <div class="text typo">
          <h1>Kobiór – Komunikat dotyczący dostawy wody</h1>
          <p>Ważny komunikat dotyczący dostawy wody.</p>
          <p>Szanowni mieszkańcy ulic Liliowej, Żołędziowej 18 w Kobiórze.</p>
          <p>Uprzejmie informujemy, że 27.10.2026 do godz. 17:00 mogą wystąpić zakłócenia w dostawie wody.</p>
          <p>Utrudnienia są spowodowane pilnymi pracami na sieci wodociągowej.</p>
          <footer><p>od 27-10-2026 do 27-10-2026</p></footer>
        </div>
      </article>
      <article class="posts-grid__item">
        <h1>Tychy – Komunikat dotyczący dostawy wody</h1>
        <p>Szanowni mieszkańcy ul. Studziennej 1-28 i Damrota 116, 116a, 118, 120 w Tychach.</p>
        <p>W dniu 28.10.2026r. od godz. 08:00 do godz. 14:00 mogą wystąpić zakłócenia.</p>
        <footer><p>od 28-10-2026 do 28-10-2026</p></footer>
      </article>
      <article class="posts-grid__item">
        <h1>Brzeszcze – Komunikat dotyczący dostawy wody</h1>
        <p>Ulica Narutowicza 32, 33, 34, 35 w Brzeszczach.</p>
        <p>W dniach 29-30.10.2026 od godz. 8:00 do godz. 15:00 mogą wystąpić zakłócenia.</p>
        <footer><p>od 29-10-2026 do 30-10-2026</p></footer>
      </article>
      <article class="posts-grid__item">
        <h1>Jawiszowice – Komunikat dotyczący dostawy wody</h1>
        <p>Ulica Bielska 6-18 w Jawiszowicach.</p>
        <p>Od dnia 22.10.2026r. od godz. 23:30 do dnia 23.10.2026r. do godziny 11:00.</p>
        <footer><p>od 22-10-2026 do 23-10-2026</p></footer>
      </article>
    "#;

    fn settings_with_address(city: &str, commune: &str, street: &str, house_no: &str) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Dom".into(),
                city_name: city.into(),
                voivodeship: "Śląskie".into(),
                district: String::new(),
                commune: commune.into(),
                street_name: street.into(),
                street_name_1: street.into(),
                street_name_2: None,
                house_no: house_no.into(),
                city_id: None,
                street_id: None,
                is_active: true,
            }],
            primary_address_index: Some(0),
            theme: None,
            language: None,
            font_size: None,
            enabled_sources: None,
            notification_preferences: HashMap::new(),
            upcoming_notification_enabled: false,
            upcoming_notification_hours: 24,
            show_other_outages: true,
            filter_by_house_no: false,
        }
    }

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn parses_multiple_articles_and_schedule_variants() {
        let notices = parse_notices(ARCHIVE_HTML).unwrap();
        assert_eq!(notices.len(), 4);
        let kobior = parse_schedule(&notices[0].text);
        assert_eq!(kobior.start_date, Some(NaiveDate::from_ymd_opt(2026, 10, 27).unwrap()));
        assert_eq!(kobior.end_date.unwrap().to_string(), "2026-10-27");
        assert_eq!(
            kobior.end_time.unwrap().format("%H:%M").to_string(),
            "17:00"
        );

        let tychy = parse_schedule(&notices[1].text);
        assert_eq!(tychy.start_date.unwrap().to_string(), "2026-10-28");
        assert_eq!(tychy.end_date.unwrap().to_string(), "2026-10-28");
        assert_eq!(
            tychy.start_time.unwrap().format("%H:%M").to_string(),
            "08:00"
        );
        assert_eq!(tychy.end_time.unwrap().format("%H:%M").to_string(), "14:00");

        let brzeszcze = parse_schedule(&notices[2].text);
        assert_eq!(brzeszcze.start_date.unwrap().to_string(), "2026-10-29");
        assert_eq!(brzeszcze.end_date.unwrap().to_string(), "2026-10-30");

        let overnight = parse_schedule(&notices[3].text);
        assert_eq!(overnight.start_date.unwrap().to_string(), "2026-10-22");
        assert_eq!(overnight.end_date.unwrap().to_string(), "2026-10-23");
        assert_eq!(
            overnight.start_time.unwrap().format("%H:%M").to_string(),
            "23:30"
        );
        assert_eq!(
            overnight.end_time.unwrap().format("%H:%M").to_string(),
            "11:00"
        );
    }

    #[test]
    fn parses_written_dates_and_invalid_dates_safely() {
        assert_eq!(
            notice_dates("w dniu 25 lipca 2025r.")[0].to_string(),
            "2025-07-25"
        );
        assert_eq!(notice_dates("29-30.02.2026").len(), 0);
        assert_eq!(notice_dates("31.13.2026").len(), 0);
    }

    #[test]
    fn empty_archive_is_valid_but_unrelated_markup_is_an_error() {
        assert!(
            parse_notices("<h2>Komunikaty</h2><p>Brak wpisów</p>")
                .unwrap()
                .is_empty()
        );
        assert!(parse_notices("<html><body>Cloudflare error</body></html>").is_err());
        assert!(
            parse_notices(
                "<h2>Komunikaty</h2><article class='posts-grid__item'><p>empty</p></article>"
            )
            .is_err()
        );
    }

    #[test]
    fn address_matching_requires_locality_and_street() {
        let tychy = settings_with_address("Tychy", "Tychy", "Studzienna", "12");
        let address = &tychy.addresses[0];
        assert!(location_matches_address(
            "Tychy – ul. Studzienna 1-28, przerwa w dostawie w Tychach",
            address
        ));
        assert!(!location_matches_address(
            "Bieruń – ul. Studzienna 1-28 w Bieruniu",
            address
        ));
        assert!(!location_matches_address(
            "Tychy – ul. Wodna 1-28 w Tychach",
            address
        ));

        let laziska = settings_with_address("Łaziska Górne", "Łaziska Górne", "Piaskowa", "55");
        assert!(location_matches_address(
            "Łaziska – ul. Piaskowej 49-71 w Łaziskach",
            &laziska.addresses[0]
        ));

        let village = settings_with_address("Jawiszowice", "Brzeszcze", "Bielska", "8");
        assert!(location_matches_address(
            "Jawiszowice – ulica Bielska 6-18",
            &village.addresses[0]
        ));
    }

    #[test]
    fn multi_locality_notice_keeps_street_groups_together() {
        let notice = "Gostyń, Wyry, Łaziska Górne – komunikat\nSzanowni mieszkańcy ulic: Spokojnej, Pszczyńskiej 70-216 w Gostyni i Wyrach, ulic: Granicznej, Słowackiego w Łaziskach Górnych.";
        let wyry = settings_with_address("Wyry", "Wyry", "Graniczna", "4");
        let laziska = settings_with_address("Łaziska Górne", "Łaziska Górne", "Graniczna", "4");

        assert!(!location_matches_address(notice, &wyry.addresses[0]));
        assert!(location_matches_address(notice, &laziska.addresses[0]));
    }

    #[test]
    fn house_number_matching_uses_shared_number_logic() {
        let in_range = settings_with_address("Tychy", "Tychy", "Studzienna", "12");
        assert!(house_number_matches_notice(
            "Tychy, ul. Studzienna 1-28",
            &in_range.addresses[0]
        ));
        let outside_range = settings_with_address("Tychy", "Tychy", "Studzienna", "35");
        assert!(!house_number_matches_notice(
            "Tychy, ul. Studzienna 1-28",
            &outside_range.addresses[0]
        ));
        let unknown_numbers = settings_with_address("Tychy", "Tychy", "Studzienna", "12");
        assert!(house_number_matches_notice(
            "Tychy, ul. Studzienna, no numbers supplied",
            &unknown_numbers.addresses[0]
        ));
    }

    #[test]
    fn old_archive_items_expire_on_their_local_end_time() {
        let schedule =
            parse_schedule("Tychy 25.07.2025 w godzinach 08:00-13:00\nod 25-07-2025 do 25-07-2025");
        assert!(schedule_is_expired(schedule, utc("2026-10-27T10:00:00Z")));

        let current =
            parse_schedule("Kobiór 27.10.2026 do godz. 17:00\nod 27-10-2026 do 27-10-2026");
        assert!(!schedule_is_expired(current, utc("2026-10-27T10:00:00Z")));
    }

    #[test]
    fn parse_entry_preserves_body_and_marks_a_matching_address_local() {
        let settings = settings_with_address("Kobiór", "Kobiór", "Żołędziowa", "18");
        let alerts =
            parse_rpwik_tychy_html(ARCHIVE_HTML, &settings, utc("2026-10-27T10:00:00Z")).unwrap();
        assert_eq!(alerts.len(), 3);
        let kobior = alerts
            .iter()
            .find(|alert| {
                alert
                    .message
                    .as_deref()
                    .unwrap_or_default()
                    .contains("Żołędziowej")
            })
            .unwrap();
        assert_eq!(kobior.is_local, Some(true));
        assert!(
            kobior
                .message
                .as_deref()
                .unwrap()
                .contains("pilnymi pracami")
        );
        assert_eq!(kobior.startDate.as_deref(), Some("2026-10-27"));
        assert_eq!(kobior.endDate.as_deref(), Some("2026-10-27T17:00:00"));
    }
}
