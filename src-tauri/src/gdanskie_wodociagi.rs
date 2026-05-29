use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use async_trait::async_trait;
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, Duration};
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use regex::Regex;

pub const GDANSKIE_WODOCIAGI_URL: &str = "https://www.gdanskiewodociagi.pl/StrefaKlienta/Awarie,Komunikaty.aspx";

pub struct GdanskieWodociagiProvider;

fn parse_unplanned_date(title: &str, time_from: &str, desc: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    // Title format e.g. "2026-05-27: Gdańsk, Myczkowskiego 11"
    let date_re = Regex::new(r"(\d{4})-(\d{2})-(\d{2})").unwrap();
    let base_date = if let Some(caps) = date_re.captures(title) {
        if let (Ok(y), Ok(m), Ok(d)) = (caps[1].parse::<i32>(), caps[2].parse::<u32>(), caps[3].parse::<u32>()) {
            NaiveDate::from_ymd_opt(y, m, d)
        } else {
            None
        }
    } else {
        None
    };

    let base_date = match base_date {
        Some(d) => d,
        None => Local::now().naive_local().date(),
    };

    // time_from format e.g. "05:00:00"
    let start_time = NaiveTime::parse_from_str(time_from.trim(), "%H:%M:%S")
        .unwrap_or_else(|_| NaiveTime::from_hms_opt(0, 0, 0).unwrap());

    // End date from description e.g. "zakończyć się do: 2026-05-27 14:00"
    let end_re = Regex::new(r"zakończyć się do:\s*(\d{4})-(\d{2})-(\d{2})\s+(\d{2}):(\d{2})").unwrap();
    let end_dt = if let Some(caps) = end_re.captures(desc) {
        if let (Ok(y), Ok(mon), Ok(d), Ok(h), Ok(min)) = (
            caps[1].parse::<i32>(),
            caps[2].parse::<u32>(),
            caps[3].parse::<u32>(),
            caps[4].parse::<u32>(),
            caps[5].parse::<u32>(),
        ) {
            NaiveDate::from_ymd_opt(y, mon, d)
                .and_then(|date| NaiveTime::from_hms_opt(h, min, 0).map(|time| NaiveDateTime::new(date, time)))
        } else {
            None
        }
    } else {
        None
    };

    let start_dt = NaiveDateTime::new(base_date, start_time);
    let end_dt = end_dt.unwrap_or_else(|| start_dt + Duration::hours(8));

    (Some(start_dt), Some(end_dt))
}

fn parse_planned_date(text: &str) -> Option<(NaiveDateTime, NaiveDateTime)> {
    // Regex for: "22.05.2026 r. godz. 07:00 - 13:00 -" or "28/29.05.2026 r. godz. 22:00 - 06:00 -"
    let planned_re = Regex::new(
        r"(?x)
        (\d{1,2})                 # Start day
        (?:/(\d{1,2}))?           # Optional end day (e.g. /29)
        \.\s*(\d{2})\.(\d{4})     # Month, Year (optional space after dot)
        \s*r\.\s*godz\.\s*
        (\d{2}):(\d{2})           # Start hour:min
        \s*-\s*
        (\d{2}):(\d{2})           # End hour:min
        "
    ).unwrap();

    if let Some(caps) = planned_re.captures(text) {
        let start_day = caps[1].parse::<u32>().ok()?;
        let end_day_opt = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
        let month = caps[3].parse::<u32>().ok()?;
        let year = caps[4].parse::<i32>().ok()?;

        let start_hour = caps[5].parse::<u32>().ok()?;
        let start_min = caps[6].parse::<u32>().ok()?;

        let end_hour = caps[7].parse::<u32>().ok()?;
        let end_min = caps[8].parse::<u32>().ok()?;

        let start_date = NaiveDate::from_ymd_opt(year, month, start_day)?;
        let start_time = NaiveTime::from_hms_opt(start_hour, start_min, 0)?;
        let start_dt = NaiveDateTime::new(start_date, start_time);

        let end_date = if let Some(end_day) = end_day_opt {
            NaiveDate::from_ymd_opt(year, month, end_day).unwrap_or(start_date)
        } else if end_hour < start_hour {
            start_date + Duration::days(1)
        } else {
            start_date
        };

        let end_time = NaiveTime::from_hms_opt(end_hour, end_min, 0)?;
        let end_dt = NaiveDateTime::new(end_date, end_time);

        Some((start_dt, end_dt))
    } else {
        None
    }
}

#[async_trait]
impl AlertProvider for GdanskieWodociagiProvider {
    fn id(&self) -> String {
        "gdanskie_wodociagi".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::GdanskieWodociagi
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

        let mut errors = Vec::new();
        let mut alerts = Vec::new();

        match retry(|| async {
            client.get(GDANSKIE_WODOCIAGI_URL)
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
                .send()
                .await
                .map_err(|e| e.to_string())?
                .text()
                .await
                .map_err(|e| e.to_string())
        }, 3).await {
            Ok(html_content) => {
                let document = Html::parse_document(&html_content);

                // --- 1. UNPLANNED OUTAGES (Awarie) ---
                let accordion_selector = Selector::parse("#accordion article").unwrap();
                let header_selector = Selector::parse("header h2").unwrap();
                let p_selector = Selector::parse(".art-content p").unwrap();

                for article in document.select(&accordion_selector) {
                    let title = article.select(&header_selector)
                        .next()
                        .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
                        .unwrap_or_default();

                    // Skip empty placeholders
                    if title.is_empty() || title.starts_with(':') {
                        continue;
                    }

                    // Extract fields
                    let mut time_from = String::new();
                    let mut beczkowoz = String::new();
                    let mut description = String::new();

                    for p in article.select(&p_selector) {
                        let text = p.text().collect::<Vec<_>>().join(" ").trim().to_string();
                        if text.starts_with("Godzina od:") {
                            time_from = text.replace("Godzina od:", "").trim().to_string();
                        } else if text.starts_with("Beczkowóz/Nastawa hydrantowa:") {
                            beczkowoz = text.replace("Beczkowóz/Nastawa hydrantowa:", "").trim().to_string();
                        } else if text.contains("zakończyć się do:") || text.contains("informują o") || text.contains("Informujemy o") {
                            description = text;
                        }
                    }

                    let (start_dt, end_dt) = parse_unplanned_date(&title, &time_from, &description);

                    // Title format: "YYYY-MM-DD: City, Street"
                    // E.g. "2026-05-27: Gdańsk, Myczkowskiego 11"
                    let mut city = "Gdańsk".to_string();
                    let mut location_details = title.clone();
                    if let Some(pos) = title.find(':') {
                        let address_part = &title[pos + 1..];
                        let parts: Vec<&str> = address_part.split(',').collect();
                        if !parts.is_empty() {
                            city = parts[0].trim().to_string();
                        }
                        location_details = address_part.trim().to_string();
                    }

                    let mut message_parts = vec![format!("Awaria wody - {}", location_details)];
                    if !beczkowoz.is_empty() {
                        message_parts.push(format!("Beczkowóz: {}", beczkowoz));
                    }
                    if !description.is_empty() {
                        message_parts.push(description.clone());
                    }
                    let message = message_parts.join(". ");

                    let mut alert = UnifiedAlert {
                        source: AlertSource::GdanskieWodociagi,
                        startDate: start_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                        endDate: end_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                        message: Some(message),
                        location: Some(format!("Miejscowość: {}", city)),
                        address_index: None,
                        is_local: Some(false),
                        hash: None,
                    };

                    let combined_text = format!("{} {} {}", city, title, description).to_lowercase();
                    check_local_matching(&mut alert, settings, &city, &combined_text);

                    alerts.push(alert);
                }

                // --- 2. PLANNED OUTAGES (Planowe wyłączenia) ---
                // SNGModule contains planned outages
                let sng_selector = Selector::parse(".DnnModule-SNGModule .ModSNGModuleC p").unwrap();
                for p in document.select(&sng_selector) {
                    let text = p.text().collect::<Vec<_>>().join(" ").trim().to_string();
                    // Must have the date and time block
                    if let Some((start_dt, end_dt)) = parse_planned_date(&text) {
                        let city = "Gdańsk".to_string();
                        let message = format!("Planowane wyłączenie wody - {}", text);

                        let mut alert = UnifiedAlert {
                            source: AlertSource::GdanskieWodociagi,
                            startDate: Some(start_dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
                            endDate: Some(end_dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
                            message: Some(message),
                            location: Some(format!("Miejscowość: {}", city)),
                            address_index: None,
                            is_local: Some(false),
                            hash: None,
                        };

                        let combined_text = format!("{} {}", city, text).to_lowercase();
                        check_local_matching(&mut alert, settings, &city, &combined_text);

                        alerts.push(alert);
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("Gdańskie Wodociągi error: {}", e);
                log::error!("{}", err_msg);
                errors.push(err_msg);
            }
        }

        // Deduplicate and calculate hashes
        for alert in &mut alerts {
            let mut hasher = DefaultHasher::new();
            alert.source.hash(&mut hasher);
            if let Some(msg) = &alert.message {
                msg.hash(&mut hasher);
            }
            if let Some(start) = &alert.startDate {
                start.hash(&mut hasher);
            }
            alert.hash = Some(format!("{:x}", hasher.finish()));
        }

        (alerts, errors)
    }
}

fn check_local_matching(alert: &mut UnifiedAlert, settings: &Settings, city: &str, combined_text: &str) {
    for (idx, a) in settings.addresses.iter().enumerate() {
        if !a.is_active { continue; }
        let a_city = a.city_name.to_lowercase();
        // Allow fallback matching if configured address city is Gdańsk (or similar)
        if a_city == city.to_lowercase() || a_city.is_empty() {
            let check_street = |street: &str| -> bool {
                if street.is_empty() { return false; }
                let s_lower = street.to_lowercase();
                let cleaned = s_lower
                    .replace("ul.", "")
                    .replace("ulica", "")
                    .replace("al.", "")
                    .replace("aleja", "")
                    .replace("pl.", "")
                    .replace("plac", "")
                    .replace("\"", "");
                let words: Vec<&str> = cleaned.split_whitespace().collect();
                let significant_words: Vec<&str> = words.into_iter()
                    .filter(|w| w.chars().count() > 3 && !w.chars().all(|c| c.is_numeric()))
                    .collect();
                if significant_words.is_empty() {
                    return combined_text.contains(&s_lower);
                }
                for w in significant_words {
                    let stem = if w.ends_with('a') && w.chars().count() > 4 {
                        let mut chars = w.chars();
                        chars.next_back();
                        chars.as_str()
                    } else {
                        w
                    };
                    if !combined_text.contains(stem) {
                        return false;
                    }
                }
                true
            };

            let mut is_match = false;
            if check_street(&a.street_name_1) {
                is_match = true;
            }
            if let Some(s2) = &a.street_name_2 {
                if check_street(s2) {
                    is_match = true;
                }
            }
            if a.street_name_1.is_empty() && a.street_name_2.as_deref().unwrap_or("").is_empty() && a_city == city.to_lowercase() {
                is_match = true;
            }

            if is_match {
                alert.is_local = Some(true);
                alert.address_index = Some(idx);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unplanned_date() {
        let title = "2026-05-27: Gdańsk, Myczkowskiego 11";
        let time_from = "05:00:00";
        let desc = "Informujemy o awarii. Prace naprawcze powinny zakończyć się do: 2026-05-27 14:00";
        let (start, end) = parse_unplanned_date(title, time_from, desc);
        assert_eq!(start.unwrap().format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-27 05:00:00");
        assert_eq!(end.unwrap().format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-27 14:00:00");
    }

    #[test]
    fn test_parse_planned_date() {
        let text1 = "22.05.2026 r. godz. 07:00 - 13:00 - ul. Czyżewskiego 31A";
        let (start1, end1) = parse_planned_date(text1).unwrap();
        assert_eq!(start1.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-22 07:00:00");
        assert_eq!(end1.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-22 13:00:00");

        let text2 = "28/29.05.2026 r. godz. 22:00 - 06:00 - ul. Tatrzańska 11";
        let (start2, end2) = parse_planned_date(text2).unwrap();
        assert_eq!(start2.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-28 22:00:00");
        assert_eq!(end2.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-29 06:00:00");

        let text3 = "01/02.06.2026 r. godz. 22:00 - 04:00 - ul. Magellana 2, 2A, 3, 4, 4A, 6, 6A, 8, 8A, 12, 14, Kolumba 5 ABCDE, 6 ABCDE,";
        let (start3, end3) = parse_planned_date(text3).unwrap();
        assert_eq!(start3.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-06-01 22:00:00");
        assert_eq!(end3.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-06-02 04:00:00");
    }
}
