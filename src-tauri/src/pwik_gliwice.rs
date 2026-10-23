use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const PWIK_GLIWICE_PAGE_URL: &str =
    "https://pwik.gliwice.pl/wylaczenia-wody-i-awarie/przerwy-w-dostawie-wody/";
pub const PWIK_GLIWICE_REST_URL: &str =
    "https://pwik.gliwice.pl/wp-json/wp/v2/pages?slug=przerwy-w-dostawie-wody";

pub struct PwikGliwiceProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedNotice {
    text: String,
    status: String,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    times: Option<(NaiveTime, NaiveTime)>,
}

fn date_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2}\.\d{1,2}\.\d{4})(?:\s*(?:-|–|—|do)\s*(\d{1,2}\.\d{1,2}\.\d{4}))?\b",
        )
        .unwrap()
    })
}

fn time_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{1,2})[:.](\d{2})\s*(?:-|–|—|do)\s*(\d{1,2})[:.](\d{2})\b").unwrap()
    })
}

fn status_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bplanowane\b").unwrap())
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
fn text_from_html(html: &str) -> String {
    let document = Html::parse_fragment(html);
    collapse_whitespace(&document.root_element().text().collect::<Vec<_>>().join(" "))
}

/// Extract `content.rendered` from the WordPress REST response. A present but empty
/// field is a valid empty page; callers should not fall back to HTML in that case.
fn extract_rest_page_html(json: &str) -> Result<String, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|error| format!("invalid page JSON: {error}"))?;
    let page = if let Some(pages) = value.as_array() {
        pages
            .first()
            .ok_or_else(|| "page response did not contain the requested slug".to_string())?
    } else {
        &value
    };
    page.get("content")
        .and_then(|content| content.get("rendered"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "page response is missing content.rendered".to_string())
}

fn parse_date_range(text: &str) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let Some(captures) = date_range_regex().captures(text) else {
        return (None, None);
    };
    let parse = |value: &str| NaiveDate::parse_from_str(value, "%d.%m.%Y").ok();
    let start = captures.get(1).and_then(|value| parse(value.as_str()));
    let end = match captures.get(2) {
        Some(value) => parse(value.as_str()),
        None => start,
    };
    (start, end)
}

fn parse_time_range(text: &str) -> Option<(NaiveTime, NaiveTime)> {
    let captures = time_range_regex().captures(text)?;
    let parse = |hour: &str, minute: &str| {
        NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
    };
    Some((
        parse(&captures[1], &captures[2])?,
        parse(&captures[3], &captures[4])?,
    ))
}

fn schedule_key(notice: &ParsedNotice) -> (Option<NaiveDate>, Option<NaiveDate>) {
    (notice.start_date, notice.end_date)
}

fn detail_score(text: &str) -> usize {
    let normalized = crate::utils::clean_text(text);
    let mut score = if time_range_regex().is_match(text) {
        8
    } else {
        0
    };
    for marker in [
        "ul.",
        "ulica",
        "adres",
        "gliwice",
        "pyskowice",
        "rudziniec",
        "sosnicowice",
        "zbroslawice",
        "przerwa",
        "dostawie",
        "prac",
        "remont",
        "napraw",
        "wymian",
    ] {
        if normalized.contains(&crate::utils::clean_text(marker)) {
            score += 2;
        }
    }
    if text
        .chars()
        .filter(|character| character.is_alphabetic())
        .count()
        > 45
    {
        score += 1;
    }
    score
}

fn meaningful_notice(text: &str) -> bool {
    let mut remainder = status_regex().replace_all(text, " ").into_owned();
    remainder = date_range_regex().replace_all(&remainder, " ").into_owned();
    remainder = time_range_regex().replace_all(&remainder, " ").into_owned();
    remainder
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count()
        >= 8
}

fn parse_notice_text(text: &str) -> Option<ParsedNotice> {
    let text = collapse_whitespace(text);
    if status_regex().find_iter(&text).count() != 1 {
        return None;
    }
    let (start_date, end_date) = parse_date_range(&text);
    if start_date.is_none() || !meaningful_notice(&text) {
        return None;
    }
    Some(ParsedNotice {
        status: "PLANOWANE".to_string(),
        times: parse_time_range(&text),
        start_date,
        end_date,
        text,
    })
}

/// Parse the page builder's announcement blocks without depending on its CSS names.
/// Candidate containers must hold one planned status and a date range; nested copies
/// of the same notice are reduced to the container with the most useful detail.
fn parse_notice_blocks(html: &str) -> Vec<ParsedNotice> {
    let document = Html::parse_fragment(html);
    let Ok(selector) = Selector::parse("*") else {
        return Vec::new();
    };
    let mut candidates: Vec<ParsedNotice> = document
        .select(&selector)
        .filter_map(|element| {
            let text = collapse_whitespace(&element.text().collect::<Vec<_>>().join(" "));
            parse_notice_text(&text)
        })
        .collect();

    // Some templates wrap a notice in several nested divs. Keep the best copy where
    // one candidate is a text subset of another with the same explicit date range.
    candidates.sort_by_key(|notice| {
        (
            std::cmp::Reverse(detail_score(&notice.text)),
            notice.text.chars().count(),
        )
    });
    let mut notices: Vec<ParsedNotice> = Vec::new();
    for candidate in candidates {
        let duplicate_container = notices.iter().any(|existing| {
            schedule_key(existing) == schedule_key(&candidate)
                && (existing.text.contains(&candidate.text)
                    || candidate.text.contains(&existing.text))
        });
        if !duplicate_container {
            notices.push(candidate);
        }
    }
    notices
}

fn date_for_output(date: Option<NaiveDate>) -> Option<String> {
    date.map(|value| value.format("%Y-%m-%d").to_string())
}

fn schedule_values(notice: &ParsedNotice) -> (Option<String>, Option<String>) {
    if let (Some(start_date), Some(end_date), Some((start_time, end_time))) =
        (notice.start_date, notice.end_date, notice.times)
    {
        let start = start_date.and_time(start_time);
        let end = end_date.and_time(end_time);
        if end >= start {
            return (
                Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(end.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
    }
    (
        date_for_output(notice.start_date),
        date_for_output(notice.end_date),
    )
}

fn notice_is_expired(notice: &ParsedNotice, now_utc: DateTime<Utc>) -> bool {
    let Some(end_date) = notice.end_date else {
        return false;
    };
    let end = notice
        .times
        .map(|(_, end_time)| end_date.and_time(end_time))
        .unwrap_or_else(|| end_date.and_hms_opt(23, 59, 59).unwrap());
    now_utc.with_timezone(&Warsaw).naive_local() > end
}

fn trim_commune_prefix(value: &str) -> &str {
    let value = value.trim();
    for prefix in ["gmina ", "gminy ", "gm. ", "gm "] {
        if value.to_lowercase().starts_with(prefix) {
            return value[prefix.len()..].trim();
        }
    }
    value
}

fn label_is_in_text(text: &str, label: &str) -> bool {
    let normalized_text = crate::utils::clean_text(text);
    let normalized_label = crate::utils::clean_text(trim_commune_prefix(label));
    let label = normalized_label.trim();
    if label.is_empty() {
        return false;
    }
    let mut variants = vec![label.to_string()];
    // City names are often declined in the announcement prose (e.g. Gliwice →
    // Gliwicach, Zbrosławice → Zbrosławicach). Match the stable stem at a word edge.
    if label.ends_with("ice") && label.len() > 4 {
        variants.push(format!("{}\\p{{L}}*", &label[..label.len() - 1]));
    } else if label.ends_with("iec") && label.len() > 4 {
        variants.push(format!("{}\\p{{L}}*", &label[..label.len() - 2]));
    }
    variants.into_iter().any(|variant| {
        Regex::new(&format!(
            r"(?i)(?:^|[^\p{{L}}\p{{N}}]){}(?:[^\p{{L}}\p{{N}}]|$)",
            variant
        ))
        .is_ok_and(|regex| regex.is_match(&normalized_text))
    })
}

fn address_locality_matches(text: &str, address: &AddressEntry) -> bool {
    label_is_in_text(text, &address.city_name) || label_is_in_text(text, &address.commune)
}

fn address_street_matches(text: &str, address: &AddressEntry) -> bool {
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    if candidates.is_empty() {
        return true;
    }
    let normalized_text = crate::utils::clean_text(text);
    candidates.iter().any(|candidate| {
        street_match_regex(candidate).is_some_and(|regex| regex.is_match(&normalized_text))
    })
}

fn street_match_regex(candidate: &str) -> Option<Regex> {
    let words = crate::utils::clean_text(candidate)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if words.is_empty() {
        return None;
    }
    let pattern = words
        .iter()
        .map(|word| {
            let stem_length = word.chars().count().saturating_sub(2).max(3);
            let stem = word.chars().take(stem_length).collect::<String>();
            format!("{}\\p{{L}}*", regex::escape(&stem))
        })
        .collect::<Vec<_>>()
        .join(r"[^\p{L}\p{N}]+");
    Regex::new(&format!(
        r"(?i)(?:^|[^\p{{L}}\p{{N}}]){}(?:[^\p{{L}}\p{{N}}]|$)",
        pattern
    ))
    .ok()
}

/// Re-evaluate the notice's locality and street against an address. The notice must
/// explicitly name that address's locality so a coincident street elsewhere cannot
/// make the outage local.
pub fn location_matches_address(text: &str, address: &AddressEntry) -> bool {
    address_locality_matches(text, address) && address_street_matches(text, address)
}

fn house_number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(\d+)\s*([a-z])?\b").unwrap())
}

fn house_range_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(\d+)\s*(?:[a-z]\s*)?[-–—]\s*(\d+)\s*([a-z])?\b").unwrap())
}

fn without_schedule_numbers(text: &str) -> String {
    let without_dates = date_range_regex().replace_all(text, " ");
    let without_times = time_range_regex().replace_all(&without_dates, " ");
    without_times.into_owned()
}

fn number_scope_for_address(text: &str, address: &AddressEntry) -> Option<String> {
    let text = crate::utils::clean_text(&without_schedule_numbers(text));
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    for candidate in candidates {
        let Some(regex) = street_match_regex(&candidate) else {
            continue;
        };
        for street_match in regex.find_iter(&text) {
            let mut tail_start = street_match.end();
            if let Some(previous) = text[..tail_start].chars().next_back()
                && !previous.is_alphanumeric()
            {
                tail_start -= previous.len_utf8();
            }
            let tail = &text[tail_start..];
            let Some(number_start) = [
                house_range_re().find(tail).map(|matched| matched.start()),
                house_number_re().find(tail).map(|matched| matched.start()),
            ]
            .into_iter()
            .flatten()
            .min() else {
                continue;
            };

            // A comma followed by a word before any number usually starts the next
            // street in PWiK's prose list; don't borrow that street's house numbers.
            if let Some(comma) = tail[..number_start].find(',') {
                let after_comma = tail[comma + 1..number_start].trim_start();
                let begins_with_number_label =
                    after_comma.starts_with("nr ") || after_comma.starts_with("numery ");
                if !begins_with_number_label
                    && after_comma.chars().next().is_some_and(char::is_alphabetic)
                {
                    continue;
                }
            }

            let mut end = tail.len();
            for marker in [";", "\n", " ul.", " ulica", " al.", " aleja"] {
                if let Some(index) = tail[number_start..].find(marker) {
                    end = end.min(number_start + index);
                }
            }
            let mut scope_end = end;
            if let Some(comma) = tail[number_start..end].find(',') {
                let after_comma = tail[number_start + comma + 1..end].trim_start();
                let begins_with_number = after_comma
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_digit());
                let begins_with_number_label =
                    after_comma.starts_with("nr ") || after_comma.starts_with("numery ");
                if !begins_with_number && !begins_with_number_label {
                    scope_end = number_start + comma;
                }
            }
            let scope = tail[number_start..scope_end].to_string();
            if house_range_re().is_match(&scope) || house_number_re().is_match(&scope) {
                return Some(scope);
            }
        }
    }
    None
}

/// Check whether an address number appears in the notice's numbered locations.
/// Unknown-number notices are considered compatible; callers should invoke this only
/// when the user's house-number filter is enabled.
pub fn house_number_matches_notice(text: &str, address: &AddressEntry) -> bool {
    let Some(address_number) = house_number_re().captures(address.house_no.trim()) else {
        return true;
    };
    let Ok(address_number) = address_number[1].parse::<u32>() else {
        return true;
    };
    let Some(number_text) = number_scope_for_address(text, address) else {
        return true;
    };
    for captures in house_range_re().captures_iter(&number_text) {
        let (Ok(start), Ok(end)) = (captures[1].parse::<u32>(), captures[2].parse::<u32>()) else {
            continue;
        };
        if (start.min(end)..=start.max(end)).contains(&address_number) {
            return true;
        }
    }
    for captures in house_number_re().captures_iter(&number_text) {
        let Ok(number) = captures[1].parse::<u32>() else {
            continue;
        };
        if number == address_number {
            return true;
        }
    }
    false
}

fn parse_notice_at(
    notice: &ParsedNotice,
    settings: &Settings,
    now_utc: DateTime<Utc>,
) -> Option<UnifiedAlert> {
    if notice_is_expired(notice, now_utc) {
        return None;
    }
    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::PwikGliwice,
                    address,
                )
                && location_matches_address(&notice.text, address)
        })
        .map(|(index, _)| index);
    let (start_date, end_date) = schedule_values(notice);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    notice.text.hash(&mut hasher);
    start_date.hash(&mut hasher);
    end_date.hash(&mut hasher);

    Some(UnifiedAlert {
        source: AlertSource::PwikGliwice,
        startDate: start_date,
        endDate: end_date,
        message: Some(format!("{}: {}", notice.status, notice.text)),
        location: Some(notice.text.clone()),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(format!("pwik_gliwice_{:x}", hasher.finish())),
    })
}

async fn get_page_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response.text().await.map_err(|error| error.to_string())
}

async fn fetch_page_html(client: &Client) -> Result<String, String> {
    match get_page_text(client, PWIK_GLIWICE_REST_URL).await {
        Ok(json) => match extract_rest_page_html(&json) {
            Ok(html) => Ok(html),
            Err(rest_error) => match get_page_text(client, PWIK_GLIWICE_PAGE_URL).await {
                Ok(html) => Ok(html),
                Err(page_error) => Err(format!(
                    "WordPress page response failed ({rest_error}); direct page failed ({page_error})"
                )),
            },
        },
        Err(rest_error) => match get_page_text(client, PWIK_GLIWICE_PAGE_URL).await {
            Ok(html) => Ok(html),
            Err(page_error) => Err(format!(
                "WordPress REST request failed ({rest_error}); direct page failed ({page_error})"
            )),
        },
    }
}

#[async_trait]
impl AlertProvider for PwikGliwiceProvider {
    fn id(&self) -> String {
        "pwik_gliwice".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PwikGliwice
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
        let html = match fetch_page_html(client).await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("PWiK Gliwice planned page error: {error}")],
                );
            }
        };
        let notices = parse_notice_blocks(&html);
        let now_utc = Utc::now();
        let alerts = notices
            .iter()
            .filter_map(|notice| parse_notice_at(notice, settings, now_utc))
            .collect();
        (alerts, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const CURRENT_NOTICE_HTML: &str = r#"
      <div class="notice">
        <h3>Ziemięcicka, Strażacka, Wakacyjna</h3>
        <span class="status">PLANOWANE</span>
        <span class="date">21.10.2026 – 21.10.2026</span>
        <p>Przerwa w dostawie wody w godz. 08:00–15:00 dla Gliwic, ul. Ziemięcicka 31–59 oraz 22–64 i Wakacyjna 34. Prace naprawcze sieci wodociągowej.</p>
      </div>
      <div class="notice"><h3></h3><span class="status">PLANOWANE</span><span class="date">–</span><p></p></div>
      <div class="notice"><span class="status">PLANOWANE</span><span class="date">–</span></div>
    "#;

    fn settings_for(city: &str, commune: &str, street: &str, house_no: &str) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Dom".into(),
                city_name: city.into(),
                voivodeship: "Śląskie".into(),
                district: "gliwicki".into(),
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
            language: Some("pl".into()),
            font_size: None,
            enabled_sources: None,
            notification_preferences: HashMap::new(),
            upcoming_notification_enabled: false,
            upcoming_notification_hours: 24,
            show_other_outages: true,
            filter_by_house_no: true,
        }
    }

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn extracts_wordpress_rendered_content_and_handles_empty_rendered_content() {
        let html = extract_rest_page_html(
            r#"[{"slug":"przerwy-w-dostawie-wody","content":{"rendered":"<p>notice</p>"}}]"#,
        )
        .unwrap();
        assert_eq!(html, "<p>notice</p>");
        assert_eq!(
            extract_rest_page_html(r#"[{"content":{"rendered":""}}]"#).unwrap(),
            ""
        );
        assert!(extract_rest_page_html("not JSON").is_err());
        assert!(extract_rest_page_html("[]").is_err());
        assert!(extract_rest_page_html(r#"[{"content":{}}]"#).is_err());
    }

    #[test]
    fn parses_notice_details_and_skips_empty_placeholders() {
        let notices = parse_notice_blocks(CURRENT_NOTICE_HTML);
        assert_eq!(notices.len(), 1);
        let notice = &notices[0];
        assert_eq!(notice.status, "PLANOWANE");
        assert_eq!(notice.start_date.unwrap().to_string(), "2026-10-21");
        assert_eq!(notice.end_date.unwrap().to_string(), "2026-10-21");
        assert_eq!(
            notice.times,
            Some((
                NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
                NaiveTime::from_hms_opt(15, 0, 0).unwrap()
            ))
        );
        assert_eq!(
            schedule_values(notice).0.as_deref(),
            Some("2026-10-21T08:00:00")
        );
        assert!(notice.text.contains("Ziemięcicka, Strażacka, Wakacyjna"));
        assert!(notice.text.contains("31–59 oraz 22–64"));
        assert!(notice.text.contains("Prace naprawcze sieci wodociągowej"));
    }

    #[test]
    fn handles_polish_whitespace_partial_dates_and_invalid_times() {
        let html = r#"<section><h2>Gliwice, ul. Krótka</h2><b>PLANOWANE</b><p>22.10.2026 – 23.10.2026</p><p>Brak godzin; prace przy sieci wodociągowej.</p></section>"#;
        let notices = parse_notice_blocks(html);
        assert_eq!(notices.len(), 1);
        let notice = &notices[0];
        assert_eq!(notice.start_date.unwrap().to_string(), "2026-10-22");
        assert_eq!(notice.end_date.unwrap().to_string(), "2026-10-23");
        assert_eq!(notice.times, None);
        assert_eq!(schedule_values(notice).0.as_deref(), Some("2026-10-22"));

        assert_eq!(parse_date_range("Termin: –").0, None);
        assert_eq!(parse_date_range("Termin: 32.13.2026").0, None);
        assert_eq!(
            parse_date_range("Termin: 22.10.2026 – 32.13.2026"),
            (NaiveDate::from_ymd_opt(2026, 10, 22), None)
        );
        assert_eq!(parse_time_range("08:90–15:00"), None);
        assert!(text_from_html("<p>Gliwice&nbsp;— ul. Długa</p>").contains("Gliwice — ul. Długa"));
    }

    #[test]
    fn suppresses_the_expired_notice_even_when_the_page_still_contains_it() {
        let notice = parse_notice_blocks(CURRENT_NOTICE_HTML).remove(0);
        let settings = settings_for("Gliwice", "Gliwice", "Ziemięcicka", "31");
        assert!(parse_notice_at(&notice, &settings, utc("2026-10-23T10:00:00Z")).is_none());
        assert!(!notice_is_expired(&notice, utc("2026-10-21T09:00:00Z")));
    }

    #[test]
    fn matches_explicit_locality_and_street_but_keeps_house_number_filter_separate() {
        let notice = parse_notice_blocks(CURRENT_NOTICE_HTML).remove(0);
        let same_locality = settings_for("Gliwice", "Gliwice", "Ziemięcicka", "44");
        let same_street_elsewhere = settings_for("Pyskowice", "Pyskowice", "Ziemięcicka", "44");
        assert!(location_matches_address(
            &notice.text,
            &same_locality.addresses[0]
        ));
        assert!(!location_matches_address(
            &notice.text,
            &same_street_elsewhere.addresses[0]
        ));
        assert!(house_number_matches_notice(
            &notice.text,
            &same_locality.addresses[0]
        ));

        let wrong_house = settings_for("Gliwice", "Gliwice", "Ziemięcicka", "70");
        assert!(location_matches_address(
            &notice.text,
            &wrong_house.addresses[0]
        ));
        assert!(!house_number_matches_notice(
            &notice.text,
            &wrong_house.addresses[0]
        ));

        let inflected_street = settings_for("Gliwice", "Gliwice", "Ziemięcicka", "44");
        assert!(location_matches_address(
            "PLANOWANE w Gliwicach, przy ul. Ziemięcickiej nr 31-59",
            &inflected_street.addresses[0]
        ));

        let unknown_number = settings_for("Gliwice", "Gliwice", "Ziemięcicka", "12");
        assert!(house_number_matches_notice(
            "PLANOWANE Gliwice, ul. Ziemięcicka — prace sieciowe",
            &unknown_number.addresses[0]
        ));
    }

    #[test]
    fn house_number_filter_uses_the_matching_street_segment() {
        let notice = "Gliwicach: Ziemięcickiej nr 31-59 oraz 22-64, Strażackiej, ul. Wakacyjnej 34";
        let zemi = settings_for("Gliwice", "Gliwice", "Ziemięcicka", "70");
        assert!(!house_number_matches_notice(notice, &zemi.addresses[0]));

        let strazacka = settings_for("Gliwice", "Gliwice", "Strażacka", "70");
        assert!(house_number_matches_notice(notice, &strazacka.addresses[0]));

        let wakacyjna = settings_for("Gliwice", "Gliwice", "Wakacyjna", "34");
        assert!(house_number_matches_notice(notice, &wakacyjna.addresses[0]));
        let other_wakacyjna = settings_for("Gliwice", "Gliwice", "Wakacyjna", "31");
        assert!(!house_number_matches_notice(
            notice,
            &other_wakacyjna.addresses[0]
        ));
    }

    #[test]
    fn empty_content_and_html_fallback_are_parseable_without_network() {
        assert!(parse_notice_blocks("").is_empty());
        assert!(parse_notice_blocks("<p>Brak aktualnych planowanych przerw.</p>").is_empty());
        assert_eq!(
            text_from_html("<main><p>fallback notice</p></main>"),
            "fallback notice"
        );
    }
}
