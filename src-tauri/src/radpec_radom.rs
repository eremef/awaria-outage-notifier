use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::NaiveDate;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::OnceLock;

const URL: &str = "https://radpec.com.pl/przerwy-w-dostawach.html";
pub struct RadpecRadomProvider;

fn numeric_date() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(20\d{2})\b").unwrap())
}
fn polish_date() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(\d{1,2})(?:\s*[-–—]\s*(\d{1,2}))?\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|września|października|listopada|grudnia)\s+(20\d{2})\b").unwrap())
}
fn schedule(text: &str) -> Option<(String, Option<String>)> {
    let dates: Vec<NaiveDate> = numeric_date()
        .captures_iter(text)
        .filter_map(|c| {
            NaiveDate::from_ymd_opt(c[3].parse().ok()?, c[2].parse().ok()?, c[1].parse().ok()?)
        })
        .collect();
    if let Some(start) = dates.first().copied() {
        static TIME: OnceLock<Regex> = OnceLock::new();
        let without_dates = numeric_date().replace_all(text, " ");
        let times: Vec<chrono::NaiveTime> = TIME
            .get_or_init(|| Regex::new(r"\b(\d{1,2}):(\d{2})\b").unwrap())
            .captures_iter(&without_dates)
            .filter_map(|c| {
                chrono::NaiveTime::from_hms_opt(c[1].parse().ok()?, c[2].parse().ok()?, 0)
            })
            .collect();
        let from = times
            .first()
            .map(|time| {
                start
                    .and_time(*time)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| start.format("%Y-%m-%d").to_string());
        let end = if let Some(end_date) = dates.get(1).copied() {
            if end_date < start {
                return None;
            }
            Some(
                end_date
                    .and_time(
                        times
                            .get(1)
                            .copied()
                            .unwrap_or(chrono::NaiveTime::from_hms_opt(23, 59, 59)?),
                    )
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
            )
        } else if let (Some(first), Some(last)) = (times.first(), times.get(1)) {
            let end_date = if last <= first {
                start.succ_opt()?
            } else {
                start
            };
            Some(
                end_date
                    .and_time(*last)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
            )
        } else {
            None
        };
        return Some((from, end));
    }
    let c = polish_date().captures(text)?;
    let month = match crate::utils::clean_text(&c[3]).as_str() {
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
    };
    let year = c[4].parse().ok()?;
    let start = NaiveDate::from_ymd_opt(year, month, c[1].parse().ok()?)?;
    let end = match c.get(2) {
        Some(day) => {
            let end = NaiveDate::from_ymd_opt(year, month, day.as_str().parse().ok()?)?;
            if end < start {
                return None;
            }
            // An inclusive date range expires at the end of its last day in Warsaw.
            Some(format!("{}T23:59:59", end.format("%Y-%m-%d")))
        }
        None => None,
    };
    Some((start.format("%Y-%m-%d").to_string(), end))
}
fn normalize(text: &str) -> String {
    crate::utils::clean_text(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// A location consists of independently separated address lines; matching is scoped to one line.
fn matching_house_spec<'a>(line: &'a str, address: &AddressEntry) -> Option<String> {
    let clean_line = normalize(crate::utils::strip_street_prefixes(line.trim()));
    for street in crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    ) {
        let street = normalize(crate::utils::strip_street_prefixes(street.trim()));
        if street.is_empty() {
            continue;
        }
        if let Some(tail) = clean_line.strip_prefix(&street)
            && (tail.is_empty() || tail.starts_with(|c: char| !c.is_alphanumeric()))
        {
            // Entrance numbers and building descriptions are not house numbers.
            let tail = tail
                .split("kl.")
                .next()
                .unwrap_or(tail)
                .split("hydrofornia")
                .next()
                .unwrap_or(tail);
            return Some(tail.to_string());
        }
    }
    None
}

fn house_matches(spec: &str, requested: &str) -> bool {
    if requested.trim().is_empty() {
        return true;
    }
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?i)\d+\s*[a-z]?").unwrap());
    let norm = |s: &str| s.to_ascii_lowercase().replace(' ', "");
    let requested = norm(requested);
    let listed: Vec<String> = re.find_iter(spec).map(|m| norm(m.as_str())).collect();
    if listed.is_empty() {
        return true;
    }
    if listed.iter().any(|n| n == &requested) {
        return true;
    }
    // A bare source number represents its building, including lettered addresses.
    let digits = requested
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if !digits.is_empty()
        && listed.iter().any(|n| n == &digits)
        && !listed.iter().any(|n| {
            n.starts_with(&digits)
                && n.len() > digits.len()
                && n[digits.len()..].chars().all(|c| c.is_ascii_alphabetic())
        })
    {
        return true;
    }
    if requested.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    crate::utils::match_house_number(&requested, spec, None)
}

pub fn location_matches_address(
    location: &str,
    address: &AddressEntry,
    filter_house: bool,
) -> bool {
    if !crate::api_logic::is_address_applicable_for_provider(&AlertSource::RadpecRadom, address) {
        return false;
    }
    location.split(';').skip(1).any(|line| {
        matching_house_spec(line, address)
            .is_some_and(|spec| !filter_house || house_matches(&spec, &address.house_no))
    })
}

#[derive(Default)]
struct Draft {
    heading: String,
    start: String,
    end: Option<String>,
    addresses: Vec<String>,
    details: Vec<String>,
}

fn parse_html(html: &str, settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("div.article-body[itemprop=articleBody]").unwrap();
    let root = document
        .select(&selector)
        .next()
        .ok_or("RADPEC article body not found")?;
    let blocks = Selector::parse("p, li").unwrap();
    let mut drafts: Vec<Draft> = Vec::new();
    let mut current: Option<Draft> = None;
    let mut other: Vec<String> = Vec::new();
    for block in root.select(&blocks) {
        let text = block
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            continue;
        }
        if block.value().name() == "p" {
            if let Some((start, end)) = schedule(&text) {
                if let Some(previous) = current.take() {
                    drafts.push(previous);
                }
                current = Some(Draft {
                    heading: text,
                    start,
                    end,
                    addresses: Vec::new(),
                    details: Vec::new(),
                });
            } else if let Some(draft) = current.as_mut() {
                draft.details.push(text);
            } else {
                other.push(text);
            }
        } else if let Some(draft) = current.as_mut() {
            if !draft
                .addresses
                .iter()
                .any(|item| normalize(item) == normalize(&text))
            {
                draft.addresses.push(text);
            }
        } else {
            return Err("RADPEC address list without a recognized outage date".into());
        }
    }
    if let Some(last) = current {
        drafts.push(last);
    }
    if drafts.is_empty() {
        let contents = normalize(&root.text().collect::<Vec<_>>().join(" "));
        if contents.trim().is_empty()
            || contents.contains("brak przerw")
            || contents.contains("nie ma przerw")
        {
            return Ok(Vec::new());
        }
        return Err("RADPEC outage content has no recognized schedule".into());
    }
    let mut alerts = Vec::new();
    for draft in drafts {
        if draft.addresses.is_empty() {
            return Err("RADPEC dated notice has no recognized address list".into());
        }
        let location = format!("Radom; {}", draft.addresses.join("; "));
        let detail = [other.join(" "), draft.details.join(" ")].join(" ");
        let kind = if normalize(&format!("{} {}", draft.heading, detail)).contains("awari") {
            "Awaria ciepła"
        } else if normalize(&format!("{} {}", draft.heading, detail)).contains("planowan") {
            "Planowana przerwa w dostawie ciepła"
        } else {
            "Przerwa w dostawie ciepła"
        };
        let matched = settings.addresses.iter().enumerate().find(|(_, addr)| {
            addr.is_active && location_matches_address(&location, addr, settings.filter_by_house_no)
        });
        let mut alert = UnifiedAlert {
            source: AlertSource::RadpecRadom,
            startDate: Some(draft.start),
            endDate: draft.end,
            message: Some(format!(
                "{kind}. {} {}\nAdresy: {}",
                draft.heading,
                detail.trim(),
                draft.addresses.join("; ")
            )),
            location: Some(location),
            address_index: matched.map(|(i, _)| i),
            is_local: Some(matched.is_some()),
            hash: None,
        };
        alert.hash = Some(alert.to_hash());
        alerts.push(alert);
    }
    Ok(alerts)
}

#[async_trait]
impl AlertProvider for RadpecRadomProvider {
    fn id(&self) -> String {
        "radpec_radom".to_string()
    }
    fn source(&self) -> AlertSource {
        AlertSource::RadpecRadom
    }
    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        if !crate::api_logic::is_provider_applicable(self.source(), settings) {
            return (vec![], vec![]);
        }
        let result = crate::utils::retry(
            || async {
                client
                    .get(URL)
                    .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .error_for_status()
                    .map_err(|e| e.to_string())?
                    .text()
                    .await
                    .map_err(|e| e.to_string())
            },
            2,
        )
        .await;
        match result.and_then(|html| parse_html(&html, settings)) {
            Ok(alerts) => (crate::api_logic::deduplicate_alerts(alerts), vec![]),
            Err(error) => (vec![], vec![format!("RADPEC: {error}")]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn address(street: &str, house: &str) -> AddressEntry {
        AddressEntry {
            city_name: "Radom".into(),
            voivodeship: "MAZOWIECKIE".into(),
            street_name: street.into(),
            house_no: house.into(),
            is_active: true,
            ..Default::default()
        }
    }
    fn fixture(body: &str) -> String {
        format!("<div class='article-body' itemprop='articleBody'>{body}</div>")
    }
    #[test]
    fn live_notice_and_house_filter() {
        let html = fixture(
            "<p>Informujemy, że w dniu 3.11.2026 nastąpi czasowe wstrzymanie dostaw ciepła:</p><ul><li>Olsztyńska 33,</li><li>Olsztyńska 37, kl.1,</li><li>Struga 108.</li></ul><p>Przyczyną przerwy jest awaria sieci ciepłowniczej.</p>",
        );
        let mut settings = Settings {
            addresses: vec![address("Olsztyńska", "38")],
            ..Default::default()
        };
        let alerts = parse_html(&html, &settings).unwrap();
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-11-03"));
        assert_eq!(alerts[0].endDate, None);
        assert_eq!(alerts[0].is_local, Some(true));
        settings.filter_by_house_no = true;
        assert_eq!(
            parse_html(&html, &settings).unwrap()[0].is_local,
            Some(false)
        );
        settings.addresses[0].house_no = "37".into();
        assert_eq!(
            parse_html(&html, &settings).unwrap()[0].is_local,
            Some(true)
        );
        settings.addresses[0].house_no = "1".into();
        assert_eq!(
            parse_html(&html, &settings).unwrap()[0].is_local,
            Some(false)
        );
    }
    #[test]
    fn dated_groups_and_suffixes() {
        let html = fixture(
            "<p>W dniach 10-12 września 2025 r.</p><ul><li>25 Czerwca 70, 81A;</li></ul><p>W dniach 15-19 września 2025 r.</p><ul><li>Hallera 5, 6;</li></ul>",
        );
        let settings = Settings {
            addresses: vec![address("25 Czerwca", "81B")],
            filter_by_house_no: true,
            ..Default::default()
        };
        let alerts = parse_html(&html, &settings).unwrap();
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].endDate.as_deref(), Some("2025-09-12T23:59:59"));
        assert_eq!(alerts[0].is_local, Some(false));
        assert!(!alerts[1].location.as_ref().unwrap().contains("25 Czerwca"));
        let mut a = settings.addresses[0].clone();
        a.house_no = "81A".into();
        assert!(location_matches_address(
            alerts[0].location.as_ref().unwrap(),
            &a,
            true
        ));
    }
    #[test]
    fn slash_addresses_and_city() {
        let loc = "Radom; 25 Czerwca 30/30A, 41/43; Struga 108";
        assert!(location_matches_address(
            loc,
            &address("25 Czerwca", "30A"),
            true
        ));
        assert!(!location_matches_address(
            loc,
            &address("25 Czerwca", "30B"),
            true
        ));
        assert!(location_matches_address(
            loc,
            &address("25 Czerwca", "41"),
            true
        ));
        assert!(!location_matches_address(
            loc,
            &address("25 Czerwca", "42"),
            true
        ));
        assert!(!location_matches_address(
            loc,
            &address("25 Czerwca", "108"),
            true
        ));
        let mut outside = address("Struga", "108");
        outside.city_name = "Płock".into();
        assert!(!location_matches_address(loc, &outside, true));
        outside.city_name = "Radom".into();
        outside.voivodeship = "ŚLĄSKIE".into();
        assert!(!location_matches_address(loc, &outside, true));
    }
    #[test]
    fn numeric_dates_and_times() {
        assert_eq!(
            schedule("Przerwa 03.11.2026 od 08:00 do 14:30"),
            Some((
                "2026-11-03T08:00:00".into(),
                Some("2026-11-03T14:30:00".into())
            ))
        );
        assert_eq!(
            schedule("Przerwa od 03.11.2026 do 05.11.2026"),
            Some(("2026-11-03".into(), Some("2026-11-05T23:59:59".into())))
        );
    }
    #[test]
    fn empty_and_bad_markup() {
        assert!(
            parse_html(&fixture("<p></p>"), &Settings::default())
                .unwrap()
                .is_empty()
        );
        assert!(parse_html("<html></html>", &Settings::default()).is_err());
        assert!(
            parse_html(
                &fixture("<p>Niespodziewany komunikat o awarii</p>"),
                &Settings::default()
            )
            .is_err()
        );
    }
    #[tokio::test]
    #[ignore = "requires network access to RADPEC"]
    async fn live_official_page_fetch() {
        let client = crate::network_state::NetworkState::build_client().unwrap();
        let html = client
            .get(URL)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap();
        let alerts = parse_html(&html, &Settings::default()).expect("RADPEC HTML should parse");
        eprintln!("RADPEC live response: {} alert(s)", alerts.len());
    }
}
