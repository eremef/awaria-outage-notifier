use crate::api_logic::{AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::OnceLock;

pub const KPEC_BYDGOSZCZ_URL: &str =
    "https://www.kpec.bydgoszcz.pl/strona/informacje-o-zakloceniach";

pub struct KpecBydgoszczProvider;

fn date_regex() -> &'static Regex {
    static DATE_RE: OnceLock<Regex> = OnceLock::new();
    DATE_RE.get_or_init(|| {
        Regex::new(r"(?i)(?:w\s+dniu|dnia|termin[:\s]*)?\s*(\d{1,2}\.\d{1,2}\.\d{4})\s*r?\.?")
            .unwrap()
    })
}

fn start_time_regex() -> &'static Regex {
    static START_TIME_RE: OnceLock<Regex> = OnceLock::new();
    START_TIME_RE.get_or_init(|| {
        Regex::new(r"(?i)\bod\s+(?:godz(?:iny)?\.?\s*)?(\d{1,2}[:.]\d{2})").unwrap()
    })
}

fn end_time_regex() -> &'static Regex {
    static END_TIME_RE: OnceLock<Regex> = OnceLock::new();
    END_TIME_RE.get_or_init(|| {
        Regex::new(
            r"(?i)\bdo\s+(?:dnia\s+(\d{1,2}\.\d{1,2}\.\d{4})\s*r?\.?\s*(?:do\s+)?)?(?:(?:godz(?:iny)?\.?\s*)?(\d{1,2}[:.]\d{2}))",
        )
        .unwrap()
    })
}

fn parse_polish_time(value: &str) -> Option<(u32, u32)> {
    let (hour, minute) = value.split_once([':', '.'])?;
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    (hour < 24 && minute < 60).then_some((hour, minute))
}

fn parse_schedule(text: &str) -> (Option<String>, Option<String>) {
    let date = date_regex()
        .captures(text)
        .and_then(|caps| NaiveDate::parse_from_str(&caps[1], "%d.%m.%Y").ok());
    let Some(date) = date else {
        return (None, None);
    };

    let start_match = start_time_regex().captures(text);
    let start_time = start_match
        .as_ref()
        .and_then(|caps| parse_polish_time(&caps[1]));
    let (start_hour, start_minute) = start_time.unwrap_or((0, 0));
    let start = date.and_hms_opt(start_hour, start_minute, 0).unwrap();

    let end_match = start_match.as_ref().and_then(|caps| {
        let remainder = &text[caps.get(0)?.end()..];
        end_time_regex().captures(remainder)
    });
    let end = end_match.as_ref().and_then(|caps| {
        let end_date = caps
            .get(1)
            .and_then(|date| NaiveDate::parse_from_str(date.as_str(), "%d.%m.%Y").ok())
            .unwrap_or(date);
        let (hour, minute) = parse_polish_time(caps.get(2)?.as_str())?;
        let mut end = end_date.and_hms_opt(hour, minute, 0)?;
        if end <= start {
            end += Duration::days(1);
        }
        Some(end)
    });

    (Some(format_datetime(start)), end.map(format_datetime))
}

fn format_datetime(datetime: NaiveDateTime) -> String {
    datetime.format("%Y-%m-%dT%H:%M:%S").to_string()
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

fn street_matches_address(text: &str, address: &crate::api_logic::AddressEntry) -> bool {
    let mut streets = vec![address.street_name_1.as_str(), address.street_name.as_str()];
    if let Some(street_name_2) = address.street_name_2.as_deref() {
        streets.push(street_name_2);
    }

    streets.into_iter().any(|street| {
        let street = crate::utils::strip_street_prefixes(street.trim());
        let tokens: Vec<_> = normalize_polish(street)
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .map(str::to_string)
            .collect();
        if tokens.is_empty() {
            return false;
        }

        let notice_tokens: Vec<_> = normalize_polish(text)
            .split(|ch: char| !ch.is_alphanumeric())
            .map(str::to_string)
            .collect();
        tokens.iter().all(|token| {
            let stem = if token.ends_with('a') && token.chars().count() > 5 {
                &token[..token.len() - 1]
            } else {
                token.as_str()
            };
            notice_tokens
                .iter()
                .any(|notice_token| notice_token.starts_with(stem))
        })
    })
}

fn is_local_to_address(text: &str, address: &crate::api_logic::AddressEntry) -> bool {
    if address.street_name_1.trim().is_empty()
        && address.street_name.trim().is_empty()
        && address
            .street_name_2
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return true;
    }
    street_matches_address(text, address)
}

pub fn parse_kpec_html(html: &str, settings: &Settings) -> Vec<UnifiedAlert> {
    let document = Html::parse_document(html);
    let Ok(card_selector) = Selector::parse("main .card") else {
        return Vec::new();
    };
    let Ok(body_selector) = Selector::parse(".card-body") else {
        return Vec::new();
    };
    let Ok(title_selector) = Selector::parse("h5") else {
        return Vec::new();
    };
    let Ok(notice_body_selector) = Selector::parse(".mb-n3") else {
        return Vec::new();
    };

    let active_addresses: Vec<_> = settings
        .addresses
        .iter()
        .enumerate()
        .filter(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::KpecBydgoszcz,
                    address,
                )
        })
        .collect();

    let mut alerts = Vec::new();
    for card in document.select(&card_selector) {
        let Some(card_body) = card.select(&body_selector).next() else {
            continue;
        };
        let heading = card_body
            .select(&title_selector)
            .next()
            .map(|title| {
                title
                    .text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        let body = card_body
            .select(&notice_body_selector)
            .next()
            .map(|content| content.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if heading.is_empty() || body.is_empty() {
            continue;
        }

        let combined_text = format!("{} {}", heading, body);
        let normalized = normalize_polish(&combined_text);
        let incident_type = if normalized.contains("awaria") {
            "Awaria ogrzewania"
        } else if normalized.contains("prac")
            || normalized.contains("wylaczen")
            || normalized.contains("wstrzyman")
        {
            "Planowane wyłączenie ogrzewania"
        } else {
            "Zakłócenia w dostawie ciepła"
        };
        let (start_date, end_date) = parse_schedule(&body);
        let message = format!("{} — {}. {}", incident_type, heading, body);

        let local_match = active_addresses
            .iter()
            .find(|(_, address)| is_local_to_address(&combined_text, address));
        let (address_index, is_local) = match local_match {
            Some((index, _)) => (Some(*index), Some(true)),
            None => (None, Some(false)),
        };

        alerts.push(UnifiedAlert {
            source: AlertSource::KpecBydgoszcz,
            startDate: start_date,
            endDate: end_date,
            message: Some(message),
            location: Some("Miejscowość: Bydgoszcz".to_string()),
            address_index,
            is_local,
            hash: None,
        });
    }

    alerts
}

#[async_trait]
impl AlertProvider for KpecBydgoszczProvider {
    fn id(&self) -> String {
        "kpec_bydgoszcz".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::KpecBydgoszcz
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

        let response = match client
            .get(KPEC_BYDGOSZCZ_URL)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("KPEC Bydgoszcz request error: {}", error)],
                )
            }
        };
        if !response.status().is_success() {
            return (
                Vec::new(),
                vec![format!("KPEC Bydgoszcz HTTP error: {}", response.status())],
            );
        }
        let html = match response.text().await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("KPEC Bydgoszcz response error: {}", error)],
                )
            }
        };

        (parse_kpec_html(&html, settings), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_logic::AddressEntry;

    fn settings(filter_by_house_no: bool) -> Settings {
        Settings {
            filter_by_house_no,
            addresses: vec![AddressEntry {
                name: "Home".to_string(),
                city_name: "Bydgoszcz".to_string(),
                voivodeship: "Kujawsko-Pomorskie".to_string(),
                district: "Bydgoszcz".to_string(),
                commune: "Bydgoszcz".to_string(),
                street_name: "ul. Białogardzka".to_string(),
                street_name_1: "Białogardzka".to_string(),
                street_name_2: None,
                house_no: "2A".to_string(),
                city_id: Some(928363),
                street_id: None,
                is_active: true,
            }],
            ..Settings::default()
        }
    }

    const ARCHIVED_NOTICE: &str = r#"
        <main>
          <div class="card"><div class="card-body">
            <p class="mb-2 overline">2026-04-20 11:04:00</p>
            <h5>Prace na sieci ciepłowniczej przy ul. Białogardzkiej</h5>
            <div class="mb-n3">
              <p>W dniu 23.04.2026r. od godz. 8:00 do godz. 14:00
              z powodu prowadzonych prac zostanie wstrzymana dostawa ciepła:</p>
              <ul><li>ul. Białogardzka 2A, 4, 7, 8.</li></ul>
            </div>
          </div></div>
        </main>
    "#;

    const ARCHIVED_MULTI_STREET_NOTICE: &str = r#"
        <div class="card"><div class="card-body">
          <p class="mb-2 overline">2026-04-20 08:35:00</p>
          <h5>Włączenie nowego odbiorcy</h5>
          <div class="mb-n3">
            <p>Informacja o wyłączeniach dostawy energii cieplnej dla odbiorców:
            22.04.2026r. od godz. 9:00 do godziny 15:30:</p>
            <p>- Jaskółcza 43 ALDI, - Żwirki i Wigury 54 A-C, 54 D-X, 54 E-J.</p>
          </div>
        </div></div>
    "#;

    #[test]
    fn parses_archived_notice_and_local_address() {
        let alerts = parse_kpec_html(ARCHIVED_NOTICE, &settings(false));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-04-23T08:00:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-04-23T14:00:00"));
        assert_eq!(alerts[0].address_index, Some(0));
        assert_eq!(alerts[0].is_local, Some(true));
        assert!(alerts[0]
            .message
            .as_deref()
            .unwrap()
            .contains("2A, 4, 7, 8"));
    }

    #[test]
    fn parses_multiple_cards_and_multi_street_notice() {
        let html = format!(
            "{}{}",
            ARCHIVED_NOTICE.replace("</main>", ""),
            format!("{}</main>", ARCHIVED_MULTI_STREET_NOTICE)
        );
        let alerts = parse_kpec_html(&html, &settings(false));
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[1].startDate.as_deref(), Some("2026-04-22T09:00:00"));
        assert_eq!(alerts[1].endDate.as_deref(), Some("2026-04-22T15:30:00"));
        let message = alerts[1].message.as_deref().unwrap();
        assert!(message.contains("Jaskółcza 43"));
        assert!(message.contains("Żwirki i Wigury 54 A-C"));
    }

    #[test]
    fn house_number_preference_does_not_change_street_matching() {
        let without_filter = parse_kpec_html(ARCHIVED_NOTICE, &settings(false));
        let with_filter = parse_kpec_html(ARCHIVED_NOTICE, &settings(true));
        assert_eq!(without_filter.len(), 1);
        assert_eq!(with_filter.len(), 1);
        assert_eq!(
            without_filter[0].address_index,
            with_filter[0].address_index
        );
        assert_eq!(without_filter[0].is_local, Some(true));
        assert_eq!(with_filter[0].is_local, Some(true));
    }

    #[test]
    fn no_outage_state_returns_no_alerts() {
        let html = "<main><p>Aktualnie nie występują żadne zakłócenia</p></main>";
        assert!(parse_kpec_html(html, &settings(false)).is_empty());
    }

    #[test]
    fn vague_end_time_stays_unknown() {
        let text = "dnia 17.09.2026 od godz. 1:30 do dnia 18.09.2026 do godzin nocnych";
        let (start, end) = parse_schedule(text);
        assert_eq!(start.as_deref(), Some("2026-09-17T01:30:00"));
        assert_eq!(end, None);
    }
}
