use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::Html;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const WM_RADOM_NOTICES_URL: &str =
    "https://www.woda.radom.pl/index.php/awaryjne-i-planowane-wylaczenia/";

pub struct WmRadomProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedNotice {
    text: String,
    locality: String,
    kind: &'static str,
    start_date: Option<String>,
    end_date: Option<String>,
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

fn notice_date_regex() -> &'static Regex {
    static DATE_RE: OnceLock<Regex> = OnceLock::new();
    DATE_RE.get_or_init(|| {
        Regex::new(r"(?i)(\d{1,2})\s*[.-]\s*(\d{1,2})\s*[.-]\s*(\d{4})(?:\s*r\.?)?").unwrap()
    })
}

fn time_range_regex() -> &'static Regex {
    static TIME_RE: OnceLock<Regex> = OnceLock::new();
    TIME_RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:od\s+)?(?:godz(?:ina|iny|inach|in)?\.?\s*)?(\d{1,2}[.:]\d{2})\s*(?:do|[-–—])\s*(?:godz(?:ina|iny|inach|in)?\.?\s*)?(\d{1,2}[.:]\d{2})",
        )
        .unwrap()
    })
}

fn locality_regex() -> &'static Regex {
    static LOCALITY_RE: OnceLock<Regex> = OnceLock::new();
    LOCALITY_RE.get_or_init(|| {
        Regex::new(
            r"(?iu)(?:miejscowoś(?:ci|ć)|miejscow(?:osci|osc))\s+([\p{L}\p{M}][\p{L}\p{M}-]*)",
        )
        .unwrap()
    })
}

fn page_block_regex() -> &'static Regex {
    static BLOCK_RE: OnceLock<Regex> = OnceLock::new();
    BLOCK_RE.get_or_init(|| {
        Regex::new(r"(?i)</\s*(?:p|li|div|article|section|h[1-6])\s*>|<\s*hr\b[^>]*>").unwrap()
    })
}

fn script_style_regex() -> &'static Regex {
    static SCRIPT_STYLE_RE: OnceLock<Regex> = OnceLock::new();
    SCRIPT_STYLE_RE.get_or_init(|| {
        Regex::new(r"(?is)<(?:script|style|noscript)\b[^>]*>.*?</(?:script|style|noscript)\s*>")
            .unwrap()
    })
}

fn parse_notice_date(text: &str) -> Option<NaiveDate> {
    let captures = notice_date_regex().captures(text)?;
    let day = captures.get(1)?.as_str().parse().ok()?;
    let month = captures.get(2)?.as_str().parse().ok()?;
    let year = captures.get(3)?.as_str().parse().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

fn parse_time(value: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(&value.replace('.', ":"), "%H:%M").ok()
}

fn schedule(text: &str) -> (Option<String>, Option<String>) {
    let Some(date) = parse_notice_date(text) else {
        return (None, None);
    };

    let Some(captures) = time_range_regex().captures(text) else {
        return (Some(date.format("%Y-%m-%d").to_string()), None);
    };
    let Some(start_time) = captures.get(1).and_then(|value| parse_time(value.as_str())) else {
        return (Some(date.format("%Y-%m-%d").to_string()), None);
    };
    let Some(end_time) = captures.get(2).and_then(|value| parse_time(value.as_str())) else {
        return (Some(date.format("%Y-%m-%d").to_string()), None);
    };

    let start = date.and_time(start_time);
    let mut end = date.and_time(end_time);
    if end <= start {
        end += Duration::days(1);
    }

    (
        Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()),
        Some(end.format("%Y-%m-%dT%H:%M:%S").to_string()),
    )
}

fn has_water_interruption(text: &str) -> bool {
    let normalized = normalize_polish(text);
    normalized.contains("przerwa w dostawie wody")
        || normalized.contains("brak wody")
        || normalized.contains("wylaczenie dostawy wody")
        || normalized.contains("wstrzymanie dostawy wody")
}

fn is_notice_start(text: &str) -> bool {
    has_water_interruption(text) && parse_notice_date(text).is_some()
}

fn notice_kind(text: &str) -> &'static str {
    let normalized = normalize_polish(text);
    if normalized.contains("awari") {
        "Awaria wodociągowa"
    } else if normalized.contains("moderniz")
        || normalized.contains("planowan")
        || normalized.contains("pracami")
        || normalized.contains("robot")
    {
        "Planowana przerwa w dostawie wody"
    } else {
        "Przerwa w dostawie wody"
    }
}

fn notice_locality(text: &str) -> String {
    if let Some(locality) = locality_regex().captures(text).and_then(|caps| caps.get(1)) {
        return locality.as_str().to_string();
    }

    let normalized = normalize_polish(text);
    if normalized.contains("cerekiew") {
        "Cerekiew".to_string()
    } else if normalized.contains("w gminie zakrzew") {
        "Zakrzew".to_string()
    } else if normalized.contains("w gminie wolanow") {
        "Wolanów".to_string()
    } else if normalized.contains("w gminie jastrzebia") {
        "Jastrzębia".to_string()
    } else if normalized.contains("w gminie jedlnia-letnisko")
        || normalized.contains("w gminie jedlnia letnisko")
    {
        "Jedlnia-Letnisko".to_string()
    } else if normalized.contains("w gminie skaryszew") {
        "Skaryszew".to_string()
    } else if normalized.contains("w gminie jedlinsk") {
        "Jedlińsk".to_string()
    } else if normalized.contains("w gminie kowala") {
        "Kowala".to_string()
    } else {
        // The sampled notices without a locality name refer to streets in Radom.
        "Radom".to_string()
    }
}

fn normalize_visible_text(value: &str) -> String {
    value
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn page_content_html(html: &str) -> String {
    let document = Html::parse_document(html);
    for selector_text in [
        ".item-page",
        ".entry-content",
        "main",
        "article",
        "#content",
    ] {
        let Ok(selector) = scraper::Selector::parse(selector_text) else {
            continue;
        };
        if let Some(element) = document.select(&selector).next() {
            return element.inner_html();
        }
    }

    document
        .select(&scraper::Selector::parse("body").unwrap())
        .next()
        .map(|element| element.inner_html())
        .unwrap_or_else(|| html.to_string())
}

fn notice_blocks(html: &str) -> Vec<String> {
    const BLOCK_MARKER: &str = "AWARIA_RADOM_BLOCK_BREAK";
    let content = page_content_html(html);
    let without_scripts = script_style_regex().replace_all(&content, " ");
    let separated = page_block_regex().replace_all(&without_scripts, BLOCK_MARKER);
    let fragment = Html::parse_fragment(&separated);
    let text = fragment.root_element().text().collect::<Vec<_>>().join(" ");

    text.split(BLOCK_MARKER)
        .map(normalize_visible_text)
        .filter(|block| is_notice_start(block))
        .collect()
}

fn parse_notices(html: &str) -> Vec<ParsedNotice> {
    let mut seen = HashSet::new();
    notice_blocks(html)
        .into_iter()
        .filter_map(|text| {
            let key = normalize_polish(&text);
            if !seen.insert(key) {
                return None;
            }

            let (start_date, end_date) = schedule(&text);
            Some(ParsedNotice {
                locality: notice_locality(&text),
                kind: notice_kind(&text),
                text,
                start_date,
                end_date,
            })
        })
        .collect()
}

fn normalized_tokens(value: &str) -> Vec<String> {
    normalize_polish(crate::utils::strip_street_prefixes(value))
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty() && *token != "ul" && *token != "ulica")
        .map(str::to_string)
        .collect()
}

fn street_matches_notice(text: &str, address: &AddressEntry) -> bool {
    let normalized_notice = normalize_polish(text);
    let notice_words: Vec<_> = normalized_notice
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let mut candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    if let Some(second) = address.street_name_2.as_deref()
        && !second.trim().is_empty()
    {
        candidates.push(second.to_string());
    }

    candidates.into_iter().any(|candidate| {
        let tokens = normalized_tokens(&candidate);
        if tokens.is_empty() {
            return false;
        }

        tokens.iter().all(|token| {
            let stem = if token.ends_with('a') && token.chars().count() > 4 {
                &token[..token.len() - 1]
            } else {
                token.as_str()
            };
            notice_words
                .iter()
                .any(|word| word.starts_with(stem) || stem.starts_with(word))
        })
    })
}

fn locality_matches(locality: &str, address: &AddressEntry) -> bool {
    let locality = normalize_polish(locality);
    let city = normalize_polish(&address.city_name);
    let commune = normalize_polish(&address.commune);

    if locality == "radom" {
        return address.city_id == Some(972750) || city == "radom";
    }

    [city.as_str(), commune.as_str()]
        .iter()
        .any(|name| !name.is_empty() && (name == &locality || name.starts_with(&locality)))
        || (locality == "cerekiew" && address.city_id == Some(642253))
}

fn address_matches_notice(notice: &ParsedNotice, address: &AddressEntry) -> bool {
    if !address.is_active || !locality_matches(&notice.locality, address) {
        return false;
    }

    let has_street = !address.street_name.trim().is_empty()
        || !address.street_name_1.trim().is_empty()
        || address
            .street_name_2
            .as_deref()
            .is_some_and(|street| !street.trim().is_empty());

    !has_street || street_matches_notice(&notice.text, address)
}

fn is_expired_without_known_end(alert: &UnifiedAlert, now: NaiveDateTime) -> bool {
    if alert.endDate.is_some() {
        return alert
            .endDate
            .as_deref()
            .and_then(|date| NaiveDateTime::parse_from_str(date, "%Y-%m-%dT%H:%M:%S").ok())
            .is_some_and(|end| end < now);
    }

    alert
        .startDate
        .as_deref()
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .is_some_and(|date| date < now.date())
}

fn to_alerts(notices: Vec<ParsedNotice>, settings: &Settings) -> Vec<UnifiedAlert> {
    notices
        .into_iter()
        .map(|notice| {
            let address_index = settings
                .addresses
                .iter()
                .enumerate()
                .find(|(_, address)| {
                    crate::api_logic::is_address_applicable_for_provider(
                        &AlertSource::WmRadom,
                        address,
                    ) && address_matches_notice(&notice, address)
                })
                .map(|(index, _)| index);

            let mut alert = UnifiedAlert {
                source: AlertSource::WmRadom,
                startDate: notice.start_date,
                endDate: notice.end_date,
                message: Some(format!("{}: {}", notice.kind, notice.text)),
                location: Some(format!("Miejscowość: {}", notice.locality)),
                address_index,
                is_local: Some(address_index.is_some()),
                hash: None,
            };

            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            alert.source.hash(&mut hasher);
            alert.message.hash(&mut hasher);
            alert.startDate.hash(&mut hasher);
            alert.hash = Some(format!("{:x}", hasher.finish()));
            alert
        })
        .collect()
}

#[async_trait]
impl AlertProvider for WmRadomProvider {
    fn id(&self) -> String {
        "wm_radom".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::WmRadom
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

        let result = crate::utils::retry(
            || async {
                let response = client
                    .get(WM_RADOM_NOTICES_URL)
                    .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
                    .send()
                    .await
                    .map_err(|error| error.to_string())?;
                if !response.status().is_success() {
                    return Err(format!("HTTP {}", response.status()));
                }
                response.text().await.map_err(|error| error.to_string())
            },
            3,
        )
        .await;

        match result {
            Ok(html) => {
                let now = Local::now().naive_local();
                let alerts = to_alerts(parse_notices(&html), settings)
                    .into_iter()
                    .filter(|alert| !is_expired_without_known_end(alert, now))
                    .collect();
                (alerts, Vec::new())
            }
            Err(error) => (
                Vec::new(),
                vec![format!("WM Radom outage page request failed: {error}")],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPRESENTATIVE_NOTICES: &str = r#"
        <main>
          <p>Wodociągi Miejskie w Radomiu informują, że w związku z awariami sieci wodociągowej, w dn. 06. 10. 2026 r. wystąpi przerwa w dostawie wody do budynków przy ulicach: Działkowej, Sierpowej oraz Perzanowskiej.</p>
          <p>Wodociągi Miejskie w Radomiu Sp. z o.o. informują, że w związku z awarią wodociągową w ulicy Potkanowskiej dnia 05-10-2026 r. nastąpi przerwa w dostawie wody.</p>
          <p>Wodociągi Miejskie w Radomiu informują, że w związku z modernizacją sieci wodociągowej w miejscowości Cerekiew ul. Złota w dniu 06.10.2026r. nastąpi przerwa w dostawie wody w godzinach 9.00 – 13.00.</p>
        </main>
    "#;

    fn address(city: &str, commune: &str, street: &str, city_id: Option<u64>) -> AddressEntry {
        AddressEntry {
            name: "Home".to_string(),
            city_name: city.to_string(),
            voivodeship: "Mazowieckie".to_string(),
            commune: commune.to_string(),
            street_name: street.to_string(),
            street_name_1: street.to_string(),
            street_name_2: None,
            house_no: "12".to_string(),
            city_id,
            is_active: true,
            ..Default::default()
        }
    }

    #[test]
    fn parses_notice_dates_and_optional_time_ranges() {
        let notices = parse_notices(REPRESENTATIVE_NOTICES);

        assert_eq!(notices.len(), 3);
        assert_eq!(notices[0].start_date.as_deref(), Some("2026-10-06"));
        assert_eq!(notices[0].end_date, None);
        assert_eq!(notices[1].start_date.as_deref(), Some("2026-10-05"));
        assert_eq!(
            notices[2].start_date.as_deref(),
            Some("2026-10-06T09:00:00")
        );
        assert_eq!(notices[2].end_date.as_deref(), Some("2026-10-06T13:00:00"));
    }

    #[test]
    fn classifies_failures_and_planned_modernisation() {
        let notices = parse_notices(REPRESENTATIVE_NOTICES);

        assert_eq!(notices[0].kind, "Awaria wodociągowa");
        assert_eq!(notices[1].kind, "Awaria wodociągowa");
        assert_eq!(notices[2].kind, "Planowana przerwa w dostawie wody");
    }

    #[test]
    fn keeps_multiple_streets_in_the_same_notice() {
        let notices = parse_notices(REPRESENTATIVE_NOTICES);

        assert_eq!(notices.len(), 3);
        for street in ["Działkowej", "Sierpowej", "Perzanowskiej"] {
            assert!(notices[0].text.contains(street));
        }
    }

    #[test]
    fn matches_radom_and_explicit_cerekiew_localities() {
        let settings = Settings {
            addresses: vec![
                address("Radom", "Radom", "Działkowa", Some(972750)),
                address("Cerekiew", "Zakrzew", "Złota", Some(642253)),
                address("Lublin", "Lublin", "Działkowa", Some(959423)),
            ],
            ..Default::default()
        };
        let alerts = to_alerts(parse_notices(REPRESENTATIVE_NOTICES), &settings);

        assert_eq!(alerts[0].address_index, Some(0));
        assert_eq!(alerts[0].is_local, Some(true));
        assert_eq!(alerts[2].address_index, Some(1));
        assert_eq!(alerts[2].is_local, Some(true));
        assert_eq!(alerts[1].address_index, None);
        assert_eq!(alerts[1].is_local, Some(false));
    }

    #[test]
    fn recognizes_only_the_municipalities_listed_by_wm_radom() {
        for commune in [
            "Radom",
            "Wolanów",
            "Jastrzębia",
            "Zakrzew",
            "Jedlnia-Letnisko",
            "Skaryszew",
            "Jedlińsk",
            "Kowala",
        ] {
            let configured = address("Village", commune, "Główna", None);
            assert!(
                crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::WmRadom,
                    &configured
                ),
                "expected {commune} in WM Radom service area"
            );
        }

        let gozd = address("Gózd", "Gózd", "Główna", None);
        assert!(!crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::WmRadom,
            &gozd
        ));

        let mut zakrzew_cerekiew = address("Cerekiew", "Zakrzew", "Złota", None);
        zakrzew_cerekiew.voivodeship.clear();
        assert!(crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::WmRadom,
            &zakrzew_cerekiew
        ));

        let mut ambiguous_cerekiew = address("Cerekiew", "", "Złota", None);
        ambiguous_cerekiew.voivodeship.clear();
        assert!(!crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::WmRadom,
            &ambiguous_cerekiew
        ));
    }

    #[test]
    fn provider_matching_does_not_apply_house_number_filtering() {
        let notice = ParsedNotice {
            text: "Awaria przy ul. Działkowej".to_string(),
            locality: "Radom".to_string(),
            kind: "Awaria wodociągowa",
            start_date: Some("2026-10-06".to_string()),
            end_date: None,
        };
        let mut settings = Settings {
            addresses: vec![address("Radom", "Radom", "Działkowa", Some(972750))],
            filter_by_house_no: true,
            ..Default::default()
        };
        let alert = to_alerts(vec![notice.clone()], &settings).remove(0);
        settings.filter_by_house_no = false;
        let unfiltered_setting_alert = to_alerts(vec![notice], &settings).remove(0);

        assert_eq!(alert.is_local, Some(true));
        assert_eq!(alert.address_index, Some(0));
        assert_eq!(alert.is_local, unfiltered_setting_alert.is_local);
        assert_eq!(alert.address_index, unfiltered_setting_alert.address_index);
    }

    #[test]
    fn empty_and_invalid_pages_produce_no_scheduled_notices() {
        assert!(parse_notices("<main><p>Brak aktualnych wyłączeń.</p></main>").is_empty());
        assert!(parse_notices(
            "<main><p>Awaria sieci wodociągowej: przerwa w dostawie wody dnia 31.02.2026 r.</p></main>"
        )
        .is_empty());
    }

    #[test]
    fn duplicate_notice_blocks_are_emitted_once() {
        let duplicate = r#"<main><p>W dniu 06.10.2026 r. awaria sieci wodociągowej: przerwa w dostawie wody na ulicy Działkowej.</p><p>W dniu 06.10.2026 r. awaria sieci wodociągowej: przerwa w dostawie wody na ulicy Działkowej.</p></main>"#;

        assert_eq!(parse_notices(duplicate).len(), 1);
    }

    #[test]
    fn date_only_notices_expire_after_the_scheduled_day() {
        let alert = UnifiedAlert {
            startDate: Some("2026-10-06".to_string()),
            endDate: None,
            ..Default::default()
        };
        let before_end = NaiveDate::from_ymd_opt(2026, 10, 6)
            .unwrap()
            .and_hms_opt(23, 59, 0)
            .unwrap();
        let after_end = NaiveDate::from_ymd_opt(2026, 10, 7)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        assert!(!is_expired_without_known_end(&alert, before_end));
        assert!(is_expired_without_known_end(&alert, after_end));
    }
}
