use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::{Captures, Regex};
use reqwest::Client;
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const PWIK_RYBNIK_AWARIE_URL: &str =
    "https://www.pwik-rybnik.pl/aktualne-awarie-i-usterki/awarie";
pub const PWIK_RYBNIK_PLANNED_URL: &str =
    "https://www.pwik-rybnik.pl/aktualne-awarie-i-usterki/planowane-przerwy-w-dostawie-wody";

pub struct PwikRybnikProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NoticeKind {
    Failure,
    Planned,
}

impl NoticeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Failure => "AWARIA",
            Self::Planned => "PLANOWANA PRZERWA",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedNotice {
    kind: NoticeKind,
    text: String,
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
    has_time: bool,
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?iu)(\d{1,2})[./-](\d{1,2})[./-](\d{2,4})|\b(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|września|wrzesnia|października|pazdziernika|listopada|grudnia)\s+(\d{4})\b",
        )
        .unwrap()
    })
}

fn parse_date_capture(captures: &Captures<'_>) -> Option<NaiveDate> {
    if let (Some(day), Some(month), Some(year)) =
        (captures.get(1), captures.get(2), captures.get(3))
    {
        let mut year: i32 = year.as_str().parse().ok()?;
        if year < 100 {
            year += 2000;
        }
        return NaiveDate::from_ymd_opt(
            year,
            month.as_str().parse().ok()?,
            day.as_str().parse().ok()?,
        );
    }
    let day: u32 = captures.get(4)?.as_str().parse().ok()?;
    let month = match captures.get(5)?.as_str().to_lowercase().as_str() {
        "stycznia" => 1,
        "lutego" => 2,
        "marca" => 3,
        "kwietnia" => 4,
        "maja" => 5,
        "czerwca" => 6,
        "lipca" => 7,
        "sierpnia" => 8,
        "września" | "wrzesnia" => 9,
        "października" | "pazdziernika" => 10,
        "listopada" => 11,
        "grudnia" => 12,
        _ => return None,
    };
    NaiveDate::from_ymd_opt(captures.get(6)?.as_str().parse().ok()?, month, day)
}

fn event_dates(text: &str) -> Vec<(usize, NaiveDate)> {
    date_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            let matched = captures.get(0)?;
            let prefix = crate::utils::clean_text(&text[..matched.start()]);
            let prefix = prefix.trim_end();
            let is_event_date = ["dnia", "w dniu", "w dniach"]
                .iter()
                .any(|phrase| prefix.ends_with(phrase));
            if !is_event_date {
                return None;
            }
            Some((matched.start(), parse_date_capture(&captures)?))
        })
        .collect()
}

fn first_time_range(text: &str) -> Option<(NaiveTime, NaiveTime)> {
    static RANGE_RE: OnceLock<Regex> = OnceLock::new();
    let range = RANGE_RE.get_or_init(|| {
        Regex::new(r"(?iu)\bod\s*(?:godz(?:iny)?\.?\s*)?(\d{1,2})[:.](\d{2})[\s\S]{0,240}?\bdo\b[\s\S]{0,200}?(\d{1,2})[:.](\d{2})")
            .unwrap()
    });
    if let Some(captures) = range.captures(text) {
        return Some((
            parse_time(&captures[1], &captures[2])?,
            parse_time(&captures[3], &captures[4])?,
        ));
    }
    static DASH_RE: OnceLock<Regex> = OnceLock::new();
    let captures = DASH_RE
        .get_or_init(|| {
            Regex::new(r"(?iu)\b(\d{1,2})[:.](\d{2})\s*[-–—]\s*(\d{1,2})[:.](\d{2})\b").unwrap()
        })
        .captures(text)?;
    Some((
        parse_time(&captures[1], &captures[2])?,
        parse_time(&captures[3], &captures[4])?,
    ))
}

fn parse_time(hour: &str, minute: &str) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
}

fn contains_water_impact(text: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let regex_match = RE.get_or_init(|| {
        Regex::new(
            r"(?iu)(?:przerw\w*\s+w\s+dostaw\w*\s+wod\w*|brak\s+wod\w*|wstrzym\w*\s+(?:dostaw\w*|dopływ\w*)\s+wod\w*|obniż\w*\s+ciśnien\w*\s+wod\w*|ciśnien\w*\s+wod\w*\s+(?:obniż\w*|spad\w*)|dopływ\s+wod\w*)",
        )
        .unwrap()
    })
    .is_match(text);
    let normalized = crate::utils::clean_text(text);
    regex_match
        || [
            "przerwa w dostawie wody",
            "przerwy w dostawie wody",
            "brak wody",
            "wstrzymanie dostawy wody",
            "wstrzymanie doplywu wody",
            "obnizone cisnienie wody",
            "doplyw wody",
        ]
        .iter()
        .any(|phrase| normalized.contains(phrase))
}

fn is_restored_or_complete(text: &str) -> bool {
    let normalized = crate::utils::clean_text(text);
    let phrases = [
        "przywrocono doplyw wody",
        "doplyw wody zostal przywrocony",
        "dostawa wody zostala przywrocona",
        "awaria zostala usunieta",
        "awarie usunieto",
        "prace zostaly zakonczone",
        "prace na sieci wodociagowej zostaly zakonczone",
        "aktualnie brak awarii",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase));
    static RE: OnceLock<Regex> = OnceLock::new();
    let restoration_pattern = RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:awari\w*|usuwan\w* awari\w*)[\s\S]{0,160}(?:usuniet\w*|zakonczon\w*)|(?:sieci wodociagowej|prace na sieci wodociagowej)[\s\S]{0,180}zakonczon\w*|doplyw wod\w*[\s\S]{0,80}przywrocon\w*",
        )
        .unwrap()
    });
    phrases || restoration_pattern.is_match(&normalized)
}

fn to_schedule(
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    times: Option<(NaiveTime, NaiveTime)>,
) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let Some(start_date) = start_date else {
        return (None, None);
    };
    if let Some((start_time, end_time)) = times {
        let end_date = end_date.unwrap_or(start_date);
        let start = start_date.and_time(start_time);
        let mut end = end_date.and_time(end_time);
        if end < start && end_date == start_date {
            end += chrono::Duration::days(1);
        }
        return (Some(start), Some(end));
    }
    (
        Some(start_date.and_hms_opt(0, 0, 0).unwrap()),
        Some(
            end_date
                .unwrap_or(start_date)
                .and_hms_opt(23, 59, 59)
                .unwrap(),
        ),
    )
}

fn parse_notice(text: &str, kind: NoticeKind) -> Option<ParsedNotice> {
    let text = collapse_whitespace(text);
    if is_restored_or_complete(&text) || !contains_water_impact(&text) {
        return None;
    }
    let dates = event_dates(&text);
    let start_date = dates.first().map(|(_, date)| *date);
    let end_date = dates
        .iter()
        .skip(1)
        .find(|(position, _)| {
            let prefix = crate::utils::clean_text(&text[..*position]);
            let prefix = prefix.trim_end();
            prefix.ends_with("do dnia") || prefix.ends_with("do w dniu")
        })
        .map(|(_, date)| *date)
        .or(start_date);
    let times = first_time_range(&text);
    let (start, end) = to_schedule(start_date, end_date, times);
    Some(ParsedNotice {
        kind,
        text,
        start,
        end,
        has_time: times.is_some(),
    })
}

fn page_text(html: &str) -> String {
    let document = Html::parse_document(html);
    let candidates = Selector::parse("main, article, .content, .entry-content, .article-content")
        .ok()
        .and_then(|selector| {
            document
                .select(&selector)
                .map(|element| collapse_whitespace(&element.text().collect::<Vec<_>>().join(" ")))
                .filter(|text| {
                    text.len() > 40
                        && (contains_water_impact(text) || is_restored_or_complete(text))
                })
                .min_by_key(|text| text.len())
        });
    candidates.unwrap_or_else(|| {
        collapse_whitespace(&document.root_element().text().collect::<Vec<_>>().join(" "))
    })
}

fn strip_page_headings(text: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)Aktualne awarie i usterki|Planowane przerwy w dostawie wody|\bAWARIA\b")
            .unwrap()
    })
    .replace_all(text, " ")
    .into_owned()
}

fn parse_page(html: &str, kind: NoticeKind) -> Vec<ParsedNotice> {
    let text = page_text(html);
    if is_restored_or_complete(&text) {
        return Vec::new();
    }
    let body_without_headings = strip_page_headings(&text);
    parse_notice(&body_without_headings, kind)
        .into_iter()
        .collect()
}

fn locality_sections(text: &str) -> Vec<(String, String)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let regex = RE.get_or_init(|| {
        Regex::new(r"(?iu)(?:^|\s)\d+\.\s*(Rybnik|Jejkowice|Gaszowice)\s*,\s*(?:ul\.?|ulica)")
            .unwrap()
    });
    let matches = regex.captures_iter(text).collect::<Vec<_>>();
    matches
        .iter()
        .enumerate()
        .filter_map(|(index, captures)| {
            let whole = captures.get(0)?;
            let city = captures.get(1)?.as_str().to_string();
            let end = matches
                .get(index + 1)
                .and_then(|next| next.get(0))
                .map(|next| next.start())
                .unwrap_or(text.len());
            Some((city, text[whole.start()..end].to_string()))
        })
        .collect()
}

fn locality_name_matches(section_city: &str, address: &AddressEntry) -> bool {
    let section_city = crate::utils::clean_text(section_city);
    [address.city_name.as_str(), address.commune.as_str()]
        .iter()
        .map(|name| crate::utils::clean_text(name))
        .any(|name| !name.is_empty() && name == section_city)
}

fn location_matches_address(text: &str, address: &AddressEntry) -> bool {
    let sections = locality_sections(text);
    if !sections.is_empty() {
        return sections.iter().any(|(city, section)| {
            locality_name_matches(city, address)
                && crate::pwik_gliwice::location_matches_address(section, address)
        });
    }
    // Some older articles use prose rather than numbered locality blocks. In that
    // format require both the explicit locality and a matching street.
    crate::pwik_gliwice::location_matches_address(text, address)
}

fn is_expired(notice: &ParsedNotice, now_utc: DateTime<Utc>) -> bool {
    let Some(end) = notice.end else {
        return false;
    };
    now_utc.with_timezone(&Warsaw).naive_local() > end
}

fn alert_for_notice(
    notice: &ParsedNotice,
    settings: &Settings,
    now_utc: DateTime<Utc>,
) -> Option<UnifiedAlert> {
    if is_expired(notice, now_utc) {
        return None;
    }
    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::PwikRybnik,
                    address,
                )
                && location_matches_address(&notice.text, address)
        })
        .map(|(index, _)| index);
    let start_date = notice.start.map(|date| {
        if notice.has_time {
            date.format("%Y-%m-%dT%H:%M:%S").to_string()
        } else {
            date.format("%Y-%m-%d").to_string()
        }
    });
    let end_date = notice.end.map(|date| {
        if notice.has_time {
            date.format("%Y-%m-%dT%H:%M:%S").to_string()
        } else {
            date.format("%Y-%m-%d").to_string()
        }
    });
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    notice.kind.hash(&mut hasher);
    notice.text.hash(&mut hasher);
    notice.start.hash(&mut hasher);
    notice.end.hash(&mut hasher);
    Some(UnifiedAlert {
        source: AlertSource::PwikRybnik,
        startDate: start_date,
        endDate: end_date,
        message: Some(format!("{}: {}", notice.kind.label(), notice.text)),
        location: Some(notice.text.clone()),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(format!("pwik_rybnik_{:x}", hasher.finish())),
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

#[async_trait]
impl AlertProvider for PwikRybnikProvider {
    fn id(&self) -> String {
        "pwik_rybnik".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PwikRybnik
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
        let (failures, planned) = tokio::join!(
            get_page_text(client, PWIK_RYBNIK_AWARIE_URL),
            get_page_text(client, PWIK_RYBNIK_PLANNED_URL),
        );
        let mut errors = Vec::new();
        let mut notices = Vec::new();
        match failures {
            Ok(html) => notices.extend(parse_page(&html, NoticeKind::Failure)),
            Err(error) => errors.push(format!("PWiK Rybnik failures page: {error}")),
        }
        match planned {
            Ok(html) => notices.extend(parse_page(&html, NoticeKind::Planned)),
            Err(error) => errors.push(format!("PWiK Rybnik planned page: {error}")),
        }
        let now = Utc::now();
        let alerts = notices
            .iter()
            .filter_map(|notice| alert_for_notice(notice, settings, now))
            .collect();
        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLANNED_PAGE: &str = r#"
      <html><body><main>
        <h1>Aktualne awarie i usterki</h1>
        <p>26.10.2026</p>
        <h2>Planowane przerwy w dostawie wody</h2>
        <p>PWiK Rybnik Sp. z o. o. informuje, że dnia 28.10.2026r w godzinach 16:00 - 20:00 nastąpi brak wody z powodu prac na sieci wodociągowej.</p>
        <p>1. Rybnik, ul.:</p>
        <ul><li>Mikołaja Gogola nr parzyste,</li><li>Józefa Poniatowskiego odcinek od ul. Wyzwolenia do ul. PCK.</li></ul>
        <p>2. Jejkowice, ul.:</p><ul><li>Główna 12-20.</li></ul>
        <p>Za utrudnienia przepraszamy.</p>
      </main></body></html>
    "#;

    #[test]
    fn separates_post_date_from_outage_schedule_and_preserves_parity_and_segments() {
        let notices = parse_page(PLANNED_PAGE, NoticeKind::Planned);
        assert_eq!(notices.len(), 1);
        let notice = &notices[0];
        assert_eq!(notice.start.unwrap().to_string(), "2026-10-28 16:00:00");
        assert_eq!(notice.end.unwrap().to_string(), "2026-10-28 20:00:00");
        assert!(notice.text.contains("26.10.2026"));
        assert!(notice.text.contains("Mikołaja Gogola nr parzyste"));
        assert!(notice.text.contains("odcinek od ul. Wyzwolenia do ul. PCK"));
        assert!(notice.text.contains("Jejkowice"));
    }

    #[test]
    fn an_empty_failures_page_is_a_valid_empty_result() {
        let html = r#"<main><h1>Aktualne awarie i usterki</h1><p>26.10.2026</p><h2>AWARIA</h2><p>Aktualnie brak awarii</p></main>"#;
        assert!(parse_page(html, NoticeKind::Failure).is_empty());
    }

    #[test]
    fn a_planned_page_heading_alone_is_not_an_outage_notice() {
        let html = r#"<main><h1>Aktualne awarie i usterki</h1><p>26.10.2026</p><h2>Planowane przerwy w dostawie wody</h2><p>Brak komunikatów.</p></main>"#;
        assert!(parse_page(html, NoticeKind::Planned).is_empty());
    }

    #[test]
    fn parses_explicit_multi_day_schedule_and_ignores_publication_date() {
        let text = "Data publikacji 10.07.2026. Przerwa: w dniu 12 lipca 2026 r. od godziny 04:00 do dnia 13 lipca 2026 r. do godziny 04:00 nastąpi wstrzymanie dostawy wody przy ul. Bukowej.";
        let notice = parse_notice(text, NoticeKind::Planned).unwrap();
        assert_eq!(notice.start.unwrap().to_string(), "2026-07-12 04:00:00");
        assert_eq!(notice.end.unwrap().to_string(), "2026-07-13 04:00:00");
    }

    #[test]
    fn skips_restored_or_unrelated_sewer_notices() {
        assert!(
            parse_notice(
                "Awaria usunięta, dopływ wody przywrócony po przerwie w dostawie wody.",
                NoticeKind::Failure,
            )
            .is_none()
        );
        assert!(
            parse_notice(
                "Prace kanalizacyjne i zamknięcie drogi. Brak utrudnień w dostawie wody.",
                NoticeKind::Failure,
            )
            .is_none()
        );
    }

    #[test]
    fn locality_sections_keep_streets_scoped_to_their_named_town() {
        let text = "1. Rybnik, ul.: Krótka 1-3. 2. Jejkowice, ul.: Długa nr parzyste.";
        let sections = locality_sections(text);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].0, "Rybnik");
        assert!(sections[0].1.contains("Krótka 1-3"));
        assert_eq!(sections[1].0, "Jejkowice");
        assert!(sections[1].1.contains("Długa nr parzyste"));
    }

    #[test]
    fn does_not_infer_an_outage_date_from_the_page_publication_date() {
        let notice = parse_notice(
            "26.10.2026 Planowane prace przy sieci wodociągowej; możliwa przerwa w dostawie wody.",
            NoticeKind::Planned,
        )
        .unwrap();
        assert_eq!(notice.start, None);
        assert_eq!(notice.end, None);
    }
}
