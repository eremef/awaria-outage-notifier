use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::OnceLock;

const URL: &str = "https://www.wpec.legnica.pl/Ogloszenia/Remonty-i-awarie";
pub struct WpecLegnicaProvider;

fn normalize(value: &str) -> String {
    crate::utils::clean_text(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn city_in_heading(heading: &str) -> Option<&'static str> {
    let heading = normalize(heading);
    // Require the locality to be introduced as the location, rather than matching
    // the company's name or an unrelated street in the body.
    static CITY: OnceLock<Regex> = OnceLock::new();
    let re = CITY.get_or_init(|| {
        Regex::new(r"\bw\s+(legnicy|lubinie|glogowie|zlotoryi|chojnowie|chocianowie|scinawie)\b")
            .unwrap()
    });
    match re.captures(&heading)?.get(1)?.as_str() {
        "legnicy" => Some("Legnica"),
        "lubinie" => Some("Lubin"),
        "glogowie" => Some("Głogów"),
        "zlotoryi" => Some("Złotoryja"),
        "chojnowie" => Some("Chojnów"),
        "chocianowie" => Some("Chocianów"),
        "scinawie" => Some("Ścinawa"),
        _ => None,
    }
}

fn schedule(text: &str) -> Option<(String, Option<String>)> {
    static DATE: OnceLock<Regex> = OnceLock::new();
    static TIME: OnceLock<Regex> = OnceLock::new();
    let date_re = DATE
        .get_or_init(|| Regex::new(r"(?i)\bw\s+dniu\s+(\d{1,2})\.(\d{1,2})\.(20\d{2})").unwrap());
    let c = date_re.captures(text)?;
    let date = NaiveDate::from_ymd_opt(c[3].parse().ok()?, c[2].parse().ok()?, c[1].parse().ok()?)?;
    let time_re = TIME.get_or_init(|| Regex::new(r"(?i)\bod\s+godz(?:iny|\.)?\s*(\d{1,2}):(\d{2})\s+do\s+godz(?:iny|\.)?\s*(\d{1,2}):(\d{2})").unwrap());
    if let Some(t) = time_re.captures(text) {
        let start = NaiveTime::from_hms_opt(t[1].parse().ok()?, t[2].parse().ok()?, 0)?;
        let end = NaiveTime::from_hms_opt(t[3].parse().ok()?, t[4].parse().ok()?, 0)?;
        let end_date = if end <= start { date.succ_opt()? } else { date };
        Some((
            date.and_time(start).format("%Y-%m-%dT%H:%M:%S").to_string(),
            Some(
                end_date
                    .and_time(end)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
            ),
        ))
    } else {
        static START_TIME: OnceLock<Regex> = OnceLock::new();
        let start_time_re = START_TIME
            .get_or_init(|| Regex::new(r"(?i)\bod\s+godz(?:iny|\.)?\s*(\d{1,2}):(\d{2})").unwrap());
        if let Some(t) = start_time_re.captures(text) {
            let start = NaiveTime::from_hms_opt(t[1].parse().ok()?, t[2].parse().ok()?, 0)?;
            Some((
                date.and_time(start)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string(),
                None,
            ))
        } else {
            Some((date.format("%Y-%m-%d").to_string(), None))
        }
    }
}

fn house_matches(spec: &str, requested: &str) -> bool {
    let requested = requested.trim().to_ascii_lowercase().replace(' ', "");
    if requested.is_empty() {
        return true;
    }
    static HOUSE: OnceLock<Regex> = OnceLock::new();
    let re = HOUSE.get_or_init(|| Regex::new(r"(?i)\d+[a-z]?").unwrap());
    let listed: Vec<String> = re
        .find_iter(spec)
        .map(|value| value.as_str().to_ascii_lowercase())
        .collect();
    if listed.is_empty() || listed.contains(&requested) {
        return true;
    }
    let number = requested
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if requested != number {
        return listed.contains(&number)
            && !listed
                .iter()
                .any(|item| item.starts_with(&number) && item != &number);
    }
    crate::utils::match_house_number(&requested, spec, None)
}

pub fn location_matches_address(
    location: &str,
    address: &AddressEntry,
    filter_house: bool,
) -> bool {
    if !crate::api_logic::is_address_applicable_for_provider(&AlertSource::WpecLegnica, address) {
        return false;
    }
    let mut parts = location.split(';');
    let city = parts
        .next()
        .unwrap_or_default()
        .strip_prefix("Miejscowość: ")
        .unwrap_or_default();
    if normalize(city) != normalize(&address.city_name) {
        return false;
    }
    let streets = parts.collect::<Vec<_>>();
    if streets.is_empty() {
        return true;
    }
    streets.iter().any(|line| {
        let line = normalize(crate::utils::strip_street_prefixes(line.trim()));
        crate::utils::configured_street_name_candidates(
            &address.street_name,
            &address.street_name_1,
            address.street_name_2.as_deref(),
        )
        .into_iter()
        .any(|street| {
            let street = normalize(crate::utils::strip_street_prefixes(street.trim()));
            if street.is_empty() {
                return false;
            }
            let Some(tail) = line.strip_prefix(&street) else {
                return false;
            };
            if !tail.is_empty() && tail.starts_with(|c: char| c.is_alphanumeric()) {
                return false;
            }
            !filter_house
                || address.house_no.trim().is_empty()
                || house_matches(tail, &address.house_no)
        })
    })
}

fn parse_html(html: &str, settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    let document = Html::parse_document(html);
    let root_selector = Selector::parse("main#content .content-area").unwrap();
    let root = document
        .select(&root_selector)
        .next()
        .ok_or("WPEC outage content not found")?;
    let block_selector = Selector::parse(".pimcore_area_txt .brick__txt h3, .pimcore_area_txt .brick__txt p, .pimcore_area_txt .brick__txt li").unwrap();
    let mut notices: Vec<(String, Vec<String>)> = Vec::new();
    for element in root.select(&block_selector) {
        let text = element
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            continue;
        }
        if element.value().name() == "h3" {
            notices.push((text, Vec::new()));
        } else if let Some((_, blocks)) = notices.last_mut() {
            blocks.push(text);
        } else {
            return Err("WPEC notice content without heading".into());
        }
    }
    if notices.is_empty() {
        let content = normalize(&root.text().collect::<Vec<_>>().join(" "));
        if content.is_empty() || content.contains("brak awarii") || content.contains("brak przerw")
        {
            return Ok(Vec::new());
        }
        return Err("WPEC outage content has unrecognized structure".into());
    }
    let mut alerts = Vec::new();
    for (heading, blocks) in notices {
        let city = city_in_heading(&heading).ok_or("WPEC notice city not recognized")?;
        let body = blocks.join(" ");
        let (start, end) = schedule(&body).ok_or("WPEC outage date not recognized")?;
        // The observed page puts affected addresses in separate dash-prefixed paragraphs.
        // The introductory paragraph's street is the repair site, not an affected address.
        let addresses: Vec<String> = blocks
            .iter()
            .filter_map(|block| {
                let item = block.trim().strip_prefix(['-', '–', '—'])?.trim();
                (!item.is_empty() && item.chars().any(|c| c.is_ascii_digit()))
                    .then(|| item.to_string())
            })
            .collect();
        let clean_body = normalize(&body);
        let citywide = clean_body.contains("calego miasta")
            || clean_body.contains("wszystkich odbiorcow miasta")
            || clean_body.contains("wszystkich odbiorcow z terenu miasta");
        if addresses.is_empty() && !citywide {
            return Err("WPEC affected addresses not recognized".into());
        }
        let location = if citywide && addresses.is_empty() {
            format!("Miejscowość: {city}")
        } else {
            format!("Miejscowość: {city}; {}", addresses.join("; "))
        };
        let matched = settings.addresses.iter().enumerate().find(|(_, address)| {
            address.is_active
                && location_matches_address(&location, address, settings.filter_by_house_no)
        });
        let mut alert = UnifiedAlert {
            source: AlertSource::WpecLegnica,
            startDate: Some(start),
            endDate: end,
            message: Some(format!("{heading}. {body}")),
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
impl AlertProvider for WpecLegnicaProvider {
    fn id(&self) -> String {
        "wpec_legnica".into()
    }
    fn source(&self) -> AlertSource {
        AlertSource::WpecLegnica
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
                    .get(URL)
                    .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                let response = response.error_for_status().map_err(|e| e.to_string())?;
                response.text().await.map_err(|e| e.to_string())
            },
            2,
        )
        .await;
        match result.and_then(|html| parse_html(&html, settings)) {
            Ok(alerts) => (crate::api_logic::deduplicate_alerts(alerts), Vec::new()),
            Err(error) => (Vec::new(), vec![format!("WPEC Legnica: {error}")]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_citywide_and_address_specific_notices_together() {
        let html = r#"
            <main id="content">
              <div class="content-area">
                <div class="pimcore_area_txt">
                  <div class="brick__txt">
                    <h3>Awaria sieci ciepłowniczej w Głogowie</h3>
                    <p>W dniu 04.11.2026r. od godziny 01:00 do czasu usunięcia awarii wstrzymana została dostawa ciepła do wszystkich odbiorców z terenu miasta Głogowa.</p>
                    <h3>Awaria sieci ciepłowniczej w Lubinie</h3>
                    <p>W dniu 04.11.2026r. od godziny 07:00 do godziny 15:00 wstrzymana zostanie dostawa ciepła do n/w obiektów przy ul.:</p>
                    <p>-Jastrzębia 2</p>
                    <p>-Legnicka 5</p>
                    <p>-Legnicka 17</p>
                    <p>-Legnicka 27</p>
                    <p>-Legnicka 37</p>
                    <p>-Legnicka 47</p>
                    <p>-Legnicka 53</p>
                    <p>-Legnicka 63</p>
                    <p>-Leśna 5</p>
                  </div>
                </div>
              </div>
            </main>
        "#;
        let mut settings = Settings::default();
        settings.filter_by_house_no = true;
        settings.addresses.push(AddressEntry {
            city_name: "Lubin".to_string(),
            street_name: "Legnicka".to_string(),
            house_no: "17".to_string(),
            is_active: true,
            ..AddressEntry::default()
        });

        let alerts = parse_html(html, &settings).unwrap();

        assert_eq!(alerts.len(), 2);
        assert_eq!(
            alerts[0].startDate.as_deref(),
            Some("2026-11-04T01:00:00")
        );
        assert_eq!(alerts[0].endDate, None);
        assert_eq!(alerts[0].location.as_deref(), Some("Miejscowość: Głogów"));
        assert_eq!(
            alerts[1].startDate.as_deref(),
            Some("2026-11-04T07:00:00")
        );
        assert_eq!(
            alerts[1].endDate.as_deref(),
            Some("2026-11-04T15:00:00")
        );
        assert_eq!(alerts[1].address_index, Some(0));
        assert_eq!(alerts[1].is_local, Some(true));
    }
}
