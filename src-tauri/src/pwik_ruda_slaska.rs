use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use futures::{StreamExt, stream};
use regex::{Captures, Regex};
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const PWIK_RUDA_AWARIE_URL: &str = "https://pwik.com.pl/aktualnosci/awarie-i-wylaczenia/awarie";
pub const PWIK_RUDA_WYLACZENIA_URL: &str =
    "https://pwik.com.pl/aktualnosci/awarie-i-wylaczenia/wylaczenia";
const MAX_ARTICLES_PER_LIST: usize = 10;

pub struct PwikRudaSlaskaProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NoticeKind {
    Failure,
    Planned,
}

impl NoticeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Failure => "AWARIA",
            Self::Planned => "PLANOWANE WYŁĄCZENIE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedNotice {
    kind: NoticeKind,
    title: String,
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
            // Ignore the article's publication date. Only an explicitly event-like
            // phrase in the body is an outage date.
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

fn title_event_date(title: &str) -> Option<NaiveDate> {
    if !crate::utils::clean_text(title).contains("informacja na dzien") {
        return None;
    }
    date_regex()
        .captures(title)
        .and_then(|captures| parse_date_capture(&captures))
}

fn time_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?iu)\bod\s*(?:godz(?:iny)?\.?\s*)?(\d{1,2})[:.](\d{2})[\s\S]{0,220}?\bdo\s*(?:godz(?:iny)?\.?\s*)?(\d{1,2})[:.](\d{2})",
        )
        .unwrap()
    })
}

fn dash_time_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)\b(\d{1,2})[:.](\d{2})\s*[-–—]\s*(\d{1,2})[:.](\d{2})\b").unwrap()
    })
}

fn parse_time(hour: &str, minute: &str) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
}

fn first_time_range(text: &str) -> Option<(NaiveTime, NaiveTime)> {
    if let Some(from) = time_range_regex().captures(text) {
        return Some((
            parse_time(&from[1], &from[2])?,
            parse_time(&from[3], &from[4])?,
        ));
    }
    if let Some(captures) = dash_time_range_regex().captures(text) {
        return Some((
            parse_time(&captures[1], &captures[2])?,
            parse_time(&captures[3], &captures[4])?,
        ));
    }

    // Some planned notices put a second date between “do dnia” and the end time.
    // Match the start and end clauses independently so that date text is allowed.
    static FROM_RE: OnceLock<Regex> = OnceLock::new();
    static TO_RE: OnceLock<Regex> = OnceLock::new();
    let from_re = FROM_RE.get_or_init(|| {
        Regex::new(r"(?iu)\bod\s*(?:godz(?:iny)?\.?\s*)?(\d{1,2})[:.](\d{2})").unwrap()
    });
    let to_re =
        TO_RE.get_or_init(|| Regex::new(r"(?iu)\bdo\b[\s\S]{0,220}?(\d{1,2})[:.](\d{2})").unwrap());
    let from = from_re.captures(text)?;
    let from_end = from.get(0)?.end();
    let to = to_re.captures(&text[from_end..])?;
    Some((parse_time(&from[1], &from[2])?, parse_time(&to[1], &to[2])?))
}

fn extension_time(text: &str) -> Option<NaiveTime> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let regex = RE.get_or_init(|| {
        Regex::new(
            r"(?iu)(?:przedłuż\w*|potrwa|wydłuż\w*)[\s\S]{0,100}?\bdo\s*(?:godz(?:iny)?\.?\s*)?(\d{1,2})[:.](\d{2})",
        )
        .unwrap()
    });
    regex
        .captures_iter(text)
        .last()
        .and_then(|captures| parse_time(&captures[1], &captures[2]))
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
        "prace zakonczone",
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
    text: &str,
) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let Some((start_time, mut end_time)) = first_time_range(text) else {
        return (
            start_date.map(|date| date.and_hms_opt(0, 0, 0).unwrap()),
            end_date
                .or(start_date)
                .map(|date| date.and_hms_opt(23, 59, 59).unwrap()),
        );
    };
    if let Some(updated_end) = extension_time(text) {
        end_time = updated_end;
    }
    let (Some(start_date), Some(end_date)) = (start_date, end_date.or(start_date)) else {
        return (None, None);
    };
    let start = start_date.and_time(start_time);
    let mut end = end_date.and_time(end_time);
    // Ruda's same-day announcements often run over midnight without repeating
    // the second date. Treat that as an overnight interval, but never alter an
    // explicitly dated multi-day interval.
    if end < start && end_date == start_date {
        end += chrono::Duration::days(1);
    }
    (Some(start), Some(end))
}

fn parse_notice(title: &str, text: &str, kind: NoticeKind) -> Option<ParsedNotice> {
    let text = collapse_whitespace(text);
    if !contains_water_impact(&text) || is_restored_or_complete(&text) {
        return None;
    }
    let dates = event_dates(&text);
    let start_date = dates
        .first()
        .map(|(_, date)| *date)
        .or_else(|| title_event_date(title));
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
    let (start, end) = to_schedule(start_date, end_date, &text);
    let has_time = first_time_range(&text).is_some();
    Some(ParsedNotice {
        kind,
        title: collapse_whitespace(title),
        text,
        start,
        end,
        has_time,
    })
}

fn extract_article_links(html: &str, base_url: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse("a[href]") else {
        return Vec::new();
    };
    let Ok(base) = Url::parse(base_url) else {
        return Vec::new();
    };
    let mut links = Vec::new();
    for anchor in document.select(&selector) {
        let title = collapse_whitespace(&anchor.text().collect::<Vec<_>>().join(" "));
        if !crate::utils::clean_text(&title).contains("informacja na dzien") {
            continue;
        }
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Ok(url) = base.join(href) else {
            continue;
        };
        if url.host_str() != Some("pwik.com.pl") {
            continue;
        }
        let url = url.to_string();
        if !links.contains(&url) {
            links.push(url);
        }
        if links.len() == MAX_ARTICLES_PER_LIST {
            break;
        }
    }
    links
}

fn article_content(html: &str) -> (String, String) {
    let document = Html::parse_document(html);
    let title = Selector::parse("h1")
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .map(|element| collapse_whitespace(&element.text().collect::<Vec<_>>().join(" ")))
        .unwrap_or_default();

    let content_selector =
        Selector::parse("article, main, .item-page, .com-content-article__body, .entry-content")
            .ok();
    let content = content_selector
        .and_then(|selector| {
            document
                .select(&selector)
                .map(|element| collapse_whitespace(&element.text().collect::<Vec<_>>().join(" ")))
                .filter(|text| text.len() > 80 && contains_water_impact(text))
                .min_by_key(|text| text.len())
        })
        .unwrap_or_else(|| {
            collapse_whitespace(&document.root_element().text().collect::<Vec<_>>().join(" "))
        });
    (title, content)
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

async fn fetch_list(
    client: &Client,
    list_url: &'static str,
    kind: NoticeKind,
) -> (Vec<ParsedNotice>, Vec<String>) {
    let html = match get_page_text(client, list_url).await {
        Ok(html) => html,
        Err(error) => return (Vec::new(), vec![format!("{list_url}: {error}")]),
    };
    let links = extract_article_links(&html, list_url);
    let results = stream::iter(links.into_iter().map(|url| async move {
        let result = get_page_text(client, &url).await;
        (url, result)
    }))
    .buffer_unordered(4)
    .collect::<Vec<_>>()
    .await;

    let mut notices = Vec::new();
    let mut errors = Vec::new();
    for (url, result) in results {
        match result {
            Ok(html) => {
                let (title, content) = article_content(&html);
                if let Some(notice) = parse_notice(&title, &content, kind) {
                    notices.push(notice);
                }
            }
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    (notices, errors)
}

fn is_expired(notice: &ParsedNotice, now_utc: DateTime<Utc>) -> bool {
    let Some(end) = notice.end else {
        return false;
    };
    now_utc.with_timezone(&Warsaw).naive_local() > end
}

fn location_matches_address(text: &str, address: &AddressEntry) -> bool {
    // The official source pages and article routes are Ruda Śląska-specific.
    // Supply that verified locality context because many article bodies list only streets.
    crate::pwik_gliwice::location_matches_address(&format!("Ruda Śląska; {text}"), address)
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
                    &AlertSource::PwikRudaSlaska,
                    address,
                )
                && location_matches_address(&notice.text, address)
        })
        .map(|(index, _)| index);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    notice.title.hash(&mut hasher);
    notice.text.hash(&mut hasher);
    notice.start.hash(&mut hasher);
    notice.end.hash(&mut hasher);
    Some(UnifiedAlert {
        source: AlertSource::PwikRudaSlaska,
        startDate: notice.start.map(|date| {
            if notice.has_time {
                date.format("%Y-%m-%dT%H:%M:%S").to_string()
            } else {
                date.format("%Y-%m-%d").to_string()
            }
        }),
        endDate: notice.end.map(|date| {
            if notice.has_time {
                date.format("%Y-%m-%dT%H:%M:%S").to_string()
            } else {
                date.format("%Y-%m-%d").to_string()
            }
        }),
        message: Some(format!("{}: {}", notice.kind.label(), notice.text)),
        location: Some(notice.text.clone()),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(format!("pwik_ruda_slaska_{:x}", hasher.finish())),
    })
}

#[async_trait]
impl AlertProvider for PwikRudaSlaskaProvider {
    fn id(&self) -> String {
        "pwik_ruda_slaska".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PwikRudaSlaska
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
            fetch_list(client, PWIK_RUDA_AWARIE_URL, NoticeKind::Failure),
            fetch_list(client, PWIK_RUDA_WYLACZENIA_URL, NoticeKind::Planned),
        );
        let now = Utc::now();
        let mut errors = failures.1;
        errors.extend(planned.1);
        let alerts = failures
            .0
            .iter()
            .chain(planned.0.iter())
            .filter_map(|notice| alert_for_notice(notice, settings, now))
            .collect();
        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UPDATED_FAILURE: &str = "Uwaga! W związku z wystąpieniem awarii na sieci wodociągowej od godz. 4:00 do godz. 10:00 nastąpi przerwa w dostawie wody dla odbiorców przy ul. Obrońców Westerplatte 30 - 32 oraz ul. Młyńskiej 1, 3. Aktualizacja godz. 9:43: czas wyłączenia przedłużony zostaje do godz. 13:00.";

    #[test]
    fn extracts_only_recent_article_links_from_the_provider_list() {
        let html = r#"
          <main>
            <a href="/aktualnosci/awarie-i-wylaczenia/awarie/informacja-na-dzien-27102026-1">Informacja na dzień 27.10.2026</a>
            <a href="/aktualnosci/awarie-i-wylaczenia/awarie/informacja-na-dzien-26102026">Więcej</a>
            <a href="https://example.com/informacja-na-dzien">Informacja na dzień 25.10.2026</a>
            <a href="/aktualnosci/awarie-i-wylaczenia">Awarie</a>
          </main>
        "#;
        let links = extract_article_links(html, PWIK_RUDA_AWARIE_URL);
        assert_eq!(links.len(), 1);
        assert!(links[0].ends_with("informacja-na-dzien-27102026-1"));
    }

    #[test]
    fn parses_failure_and_uses_latest_extension_time() {
        let notice = parse_notice(
            "Informacja na dzień 27.10.2026",
            UPDATED_FAILURE,
            NoticeKind::Failure,
        )
        .unwrap();
        assert_eq!(notice.start.unwrap().to_string(), "2026-10-27 04:00:00");
        assert_eq!(notice.end.unwrap().to_string(), "2026-10-27 13:00:00");
        assert!(notice.text.contains("Obrońców Westerplatte 30 - 32"));
        assert!(notice.text.contains("Młyńskiej 1, 3"));
    }

    #[test]
    fn parses_explicit_multi_day_planned_interval_and_keeps_all_affected_places() {
        let body = "Przedsiębiorstwo informuje, iż w dniu 12 lipca 2026 r. od godziny 04:00 do dnia 13 lipca 2026 r. do godziny 04:00 wystąpi czasowe wstrzymanie dostawy wody do terenów przemysłowych: ul. Bukowa, Trasa N-S. Może wystąpić obniżone ciśnienie wody przy ul. Górnej.";
        let notice =
            parse_notice("Informacja na dzień 10.07.2026", body, NoticeKind::Planned).unwrap();
        assert_eq!(notice.start.unwrap().to_string(), "2026-07-12 04:00:00");
        assert_eq!(notice.end.unwrap().to_string(), "2026-07-13 04:00:00");
        assert!(notice.text.contains("Bukowa, Trasa N-S"));
        assert!(notice.text.contains("ul. Górnej"));
    }

    #[test]
    fn skips_restored_and_non_water_traffic_or_sewer_notices() {
        assert!(parse_notice(
            "Informacja na dzień 24.10.2026",
            "Awaria wodociągowa została usunięta, dopływ wody został przywrócony. Przerwa w dostawie wody była przy ul. Kokota.",
            NoticeKind::Failure,
        )
        .is_none());
        assert!(parse_notice(
            "Informacja na dzień 24.10.2026",
            "Zamknięcie ulicy i utrudnienia w ruchu z powodu prac kanalizacyjnych. Ruch wahadłowy.",
            NoticeKind::Failure,
        )
        .is_none());
    }

    #[test]
    fn keeps_house_number_filtering_out_of_the_provider_and_matches_ruda_context() {
        let notice = parse_notice(
            "Informacja na dzień 27.10.2026",
            UPDATED_FAILURE,
            NoticeKind::Failure,
        )
        .unwrap();
        let address = AddressEntry {
            city_name: "Ruda Śląska".into(),
            street_name: "Obrońców Westerplatte".into(),
            street_name_1: "Obrońców Westerplatte".into(),
            house_no: "90".into(),
            ..AddressEntry::default()
        };
        assert!(location_matches_address(&notice.text, &address));
    }
}
