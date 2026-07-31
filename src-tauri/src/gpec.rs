use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use regex::Regex;
#[cfg(not(target_os = "android"))]
use std::sync::{Mutex, OnceLock};
#[cfg(not(target_os = "android"))]
use tokio::sync::broadcast;

use tauri::Manager;
#[cfg(not(target_os = "android"))]
use tauri::{WebviewWindowBuilder, WebviewUrl};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(not(target_os = "android"))]
type InflightSender = Mutex<Option<broadcast::Sender<Result<String, String>>>>;

/// Singleflight guard: if a WebView fetch is already in-flight, new callers
/// subscribe to the same broadcast channel instead of launching a second WebView.
#[cfg(not(target_os = "android"))]
static GPEC_INFLIGHT: OnceLock<InflightSender> = OnceLock::new();

#[cfg(not(target_os = "android"))]
fn gpec_inflight() -> &'static InflightSender {
    GPEC_INFLIGHT.get_or_init(|| Mutex::new(None))
}

pub const GPEC_URL: &str = "https://grupagpec.pl/przerwy-w-dostawie/";

pub struct GpecProvider;

// Helper to parse dates like "27.05.2026 godz. 08:00" or "2026-05-27 08:00"
fn parse_date_with_godz(text: &str) -> Option<NaiveDateTime> {
    let cleaned = text.replace("godz.", "").replace("r.", "").replace("Godz.", "");
    let cleaned = cleaned.trim();

    // Try standard YYYY-MM-DD HH:MM
    if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%d %H:%M") {
        return Some(dt);
    }
    // Try DD.MM.YYYY HH:MM
    if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, "%d.%m.%Y %H:%M") {
        return Some(dt);
    }
    // Try YYYY-MM-DD HH:MM:SS
    if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%d %H:%M:%S") {
        return Some(dt);
    }
    // Try DD.MM.YYYY
    if let Ok(d) = NaiveDate::parse_from_str(cleaned, "%d.%m.%Y") {
        return Some(NaiveDateTime::new(d, NaiveTime::from_hms_opt(0, 0, 0).unwrap()));
    }
    // Try YYYY-MM-DD
    if let Ok(d) = NaiveDate::parse_from_str(cleaned, "%Y-%m-%d") {
        return Some(NaiveDateTime::new(d, NaiveTime::from_hms_opt(0, 0, 0).unwrap()));
    }

    None
}

// Extractor to locate start/end dates in text
fn extract_dates(text: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    // Look for "od <date> do <date>" or similar
    let date_range_re = Regex::new(
        r"(?i)(?:od\s+)?(\d{1,2}\.\d{2}\.\d{4}(?:\s+godz\.\s*\d{2}:\d{2})?|\d{4}-\d{2}-\d{2}(?:\s+godz\.\s*\d{2}:\d{2})?)\s+(?:do\s+)?(\d{1,2}\.\d{2}\.\d{4}(?:\s+godz\.\s*\d{2}:\d{2})?|\d{4}-\d{2}-\d{2}(?:\s+godz\.\s*\d{2}:\d{2})?)"
    ).unwrap();

    if let Some(caps) = date_range_re.captures(text) {
        let start = parse_date_with_godz(&caps[1]);
        let end = parse_date_with_godz(&caps[2]);
        return (start, end);
    }

    // Try fallback simple date match in text
    let single_date_re = Regex::new(r"\d{1,2}\.\d{2}\.\d{4}(?:\s+godz\.\s*\d{2}:\d{2})?|\d{4}-\d{2}-\d{2}(?:\s+godz\.\s*\d{2}:\d{2})?").unwrap();
    let matches: Vec<_> = single_date_re.find_iter(text).collect();
    if matches.len() >= 2 {
        let start = parse_date_with_godz(matches[0].as_str());
        let end = parse_date_with_godz(matches[1].as_str());
        return (start, end);
    } else if matches.len() == 1 {
        let start = parse_date_with_godz(matches[0].as_str());
        return (start, None);
    }

    (None, None)
}

#[async_trait]
impl AlertProvider for GpecProvider {
    fn id(&self) -> String {
        "gpec".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::Gpec
    }

    async fn fetch(
        &self,
        _client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        if !crate::api_logic::is_provider_applicable(self.source(), settings) {
            return (Vec::new(), Vec::new());
        }

        if let Some(app) = _app_handle {
            if let Ok(Some(cached_html)) = get_cached_html(app).await {
                log::info!("GPEC: Using cached HTML (1h TTL)");
                return (parse_gpec_html(&cached_html, settings), Vec::new());
            }
        }

        let mut errors = Vec::new();
        let mut alerts = Vec::new();

        let mut html_to_parse = String::new();

        if let Some(app) = _app_handle {
            log::info!("GPEC: Fetching via WebView...");
            match fetch_via_webview(app).await {
                Ok(html) => {
                    html_to_parse = html;
                }
                Err(e) => {
                    log::error!("GPEC WebView fetch failed: {}", e);
                    errors.push(format!("GPEC WebView fallback failed: {}", e));
                }
            }
        } else {
            let err_msg = "GPEC Gdańsk requires WebView fallback, which needs an AppHandle.".to_string();
            log::warn!("{}", err_msg);
            errors.push(err_msg);
        }

        if !html_to_parse.is_empty() {
            if let Some(app) = _app_handle {
                let _ = save_cached_html(app, &html_to_parse).await;
            }
            alerts = parse_gpec_html(&html_to_parse, settings);
        }

        (alerts, errors)
    }
}

pub fn parse_gpec_html(html_content: &str, settings: &Settings) -> Vec<UnifiedAlert> {
    let mut alerts = Vec::new();
    
    if html_content.contains("Brak przerw") || html_content.contains("brak przerw") {
        return alerts;
    }

    let document = Html::parse_document(html_content);

    let no_acc_selector = Selector::parse(".no-acc-info").unwrap();
    if let Some(no_acc_el) = document.select(&no_acc_selector).next() {
        let text = no_acc_el.text().collect::<Vec<_>>().join(" ");
        if text.contains("Brak przerw") || text.contains("brak przerw") {
            return alerts;
        }
    }

    // Outage cards are .cloud-info elements; .dashed is a child containing city/address
    let cloud_info_selector = Selector::parse(".cloud-info").unwrap();
    let dashed_selector = Selector::parse(".dashed").unwrap();
    let listed_selector = Selector::parse(".listed_steets").unwrap();
    let all_streets_selector = Selector::parse(".all_streets").unwrap();

    for item in document.select(&cloud_info_selector) {
        // Heading: "PRZERWA W DOSTAWIE" or "PLANOWANE WYŁĄCZENIE" etc.
        let heading_text = item.select(&Selector::parse("h3").unwrap())
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default()
            .to_lowercase();

        let is_planned = heading_text.contains("planow") || heading_text.contains("wyłączen");
        let incident_type = if is_planned {
            "Planowane wyłączenie ogrzewania"
        } else {
            "Awaria ogrzewania"
        };

        // Address/neighbourhood from .dashed child
        let dashed_el = item.select(&dashed_selector).next();
        let city = dashed_el
            .as_ref()
            .and_then(|el| el.select(&Selector::parse(".dashed__city").unwrap()).next())
            .map(|el| el.text().collect::<Vec<_>>().join("").trim().to_string())
            .unwrap_or_else(|| "Gdańsk".to_string());

        let listed_streets = dashed_el.as_ref()
            .and_then(|el| el.select(&listed_selector).next())
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();

        let all_streets = dashed_el.as_ref()
            .and_then(|el| el.select(&all_streets_selector).next())
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();

        let dashed_text = dashed_el
            .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();

        let street_details = if !all_streets.is_empty() {
            all_streets
        } else if !listed_streets.is_empty() {
            listed_streets
        } else {
            dashed_text.clone()
        };

        if street_details.is_empty() {
            continue;
        }

        // Dates are in <span> elements directly in .cloud-info (outside .dashed)
        let spans: Vec<String> = item.select(&Selector::parse("span:not(.dashed__city)").unwrap())
            .map(|el| el.text().collect::<Vec<_>>().join("").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let full_text = item.text().collect::<Vec<_>>().join(" ").trim().to_string();
        let mut start_dt = None;
        let mut end_dt = None;
        log::info!("[GPEC] Extracted spans: {:?}", spans);

        if spans.len() >= 2 {
            start_dt = parse_date_with_godz(&spans[0]);
            // End date may include "godziny nocne" etc, try to parse just date part
            let end_word1 = spans[1].split_whitespace().next().unwrap_or("");
            let end_str = spans[1].split_whitespace().take(2).collect::<Vec<_>>().join(" ");
            end_dt = parse_date_with_godz(&end_str)
                .or_else(|| parse_date_with_godz(end_word1))
                .or_else(|| parse_date_with_godz(&spans[1]));
        }

        if start_dt.is_none() && end_dt.is_none() {
            log::info!("[GPEC] Falling back to extract_dates for text: {}", full_text);
            let extracted = extract_dates(&full_text);
            start_dt = extracted.0;
            end_dt = extracted.1;
        }

        // If end_dt is at 00:00:00, the user requested we make it 23:59:59 so it doesn't look like it ends at midnight
        if let Some(mut ed) = end_dt {
            if ed.time() == chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap() {
                ed = chrono::NaiveDateTime::new(ed.date(), chrono::NaiveTime::from_hms_opt(23, 59, 59).unwrap());
                end_dt = Some(ed);
            }
        }

        log::info!("[GPEC] Parsed dates: start={:?}, end={:?}", start_dt, end_dt);

        let message = format!("{} - {}. Wstrzymanie dostawy ciepłej wody i ogrzewania.", incident_type, street_details);

        let mut alert = UnifiedAlert {
            source: AlertSource::Gpec,
            startDate: start_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
            endDate: end_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
            message: Some(message),
            location: Some(format!("Miejscowość: {}", city)),
            address_index: None,
            is_local: Some(false),
            hash: None,
        };

        let combined_text = format!("{} {} {}", city, full_text, street_details).to_lowercase();
        check_local_matching(&mut alert, settings, &city, &combined_text);

        let mut hasher = DefaultHasher::new();
        alert.source.hash(&mut hasher);
        if let Some(msg) = &alert.message {
            msg.hash(&mut hasher);
        }
        if let Some(start) = &alert.startDate {
            start.hash(&mut hasher);
        }
        alert.hash = Some(format!("{:x}", hasher.finish()));

        alerts.push(alert);
    }

    // Fallback: if no .cloud-info found, try legacy .dashed selector directly
    if alerts.is_empty() {
        let dashed_only_selector = Selector::parse(".dashed").unwrap();
        for item in document.select(&dashed_only_selector) {
            let full_text = item.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if full_text.is_empty() { continue; }
            let (start_dt, end_dt) = extract_dates(&full_text);
            let cities = vec!["Gdańsk", "Sopot", "Kowale", "Tczew", "Starogard", "Pelplin"];
            let mut city = "Gdańsk".to_string();
            for c in &cities {
                if full_text.to_lowercase().contains(&c.to_lowercase()) {
                    city = c.to_string();
                    break;
                }
            }
            let is_planned = full_text.to_lowercase().contains("planowan");
            let incident_type = if is_planned { "Planowane wyłączenie ogrzewania" } else { "Awaria ogrzewania" };
            let message = format!("{} - {}.", incident_type, full_text);
            let mut alert = UnifiedAlert {
                source: AlertSource::Gpec,
                startDate: start_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                endDate: end_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                message: Some(message),
                location: Some(format!("Miejscowość: {}", city)),
                address_index: None,
                is_local: Some(false),
                hash: None,
            };
            let combined_text = format!("{} {}", city, full_text).to_lowercase();
            check_local_matching(&mut alert, settings, &city, &combined_text);
            let mut hasher = DefaultHasher::new();
            alert.source.hash(&mut hasher);
            if let Some(msg) = &alert.message { msg.hash(&mut hasher); }
            if let Some(start) = &alert.startDate { start.hash(&mut hasher); }
            alert.hash = Some(format!("{:x}", hasher.finish()));
            alerts.push(alert);
        }
    }

    alerts
}

fn check_local_matching(alert: &mut UnifiedAlert, settings: &Settings, city: &str, combined_text: &str) {
    for (idx, a) in settings.addresses.iter().enumerate().filter(|(_, a)| a.is_active && crate::api_logic::is_address_applicable_for_provider(&AlertSource::Gpec, a)) {
        if !a.is_active { continue; }
        let a_city = a.city_name.to_lowercase();
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


async fn get_cached_html(app: &tauri::AppHandle) -> Result<Option<String>, String> {
    let db = app.state::<crate::state_db::DbState>();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    
    let cache_time = crate::state_db::get_kv(&conn, "gpec_html_time")?
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    if now >= cache_time && now - cache_time < 3600 { // 1 hour TTL
        return crate::state_db::get_kv(&conn, "gpec_html_cache");
    }
    
    Ok(None)
}

async fn save_cached_html(app: &tauri::AppHandle, html: &str) -> Result<(), String> {
    let db = app.state::<crate::state_db::DbState>();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    
    crate::state_db::set_kv(&conn, "gpec_html_cache", html)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    crate::state_db::set_kv(&conn, "gpec_html_time", &now.to_string())?;
    
    Ok(())
}

async fn fetch_via_webview(#[allow(unused_variables)] app: &tauri::AppHandle) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        log::info!("Starting native GPEC fetch on Android...");
        crate::get_gpec_html_android().await
    }

    // Singleflight: on desktop, at most one WebView fetch runs at a time.
    // Additional concurrent callers subscribe to the in-flight broadcast and
    // receive the result the moment the first caller finishes.
    #[cfg(not(target_os = "android"))]
    singleflight_webview_fetch(app).await
}

#[cfg(not(target_os = "android"))]
async fn singleflight_webview_fetch(app: &tauri::AppHandle) -> Result<String, String> {
    // --- Check / register in-flight ---
    let mut rx: Option<broadcast::Receiver<Result<String, String>>> = {
        let mut guard = gpec_inflight().lock().unwrap();
        if let Some(tx) = guard.as_ref() {
            // A fetch is already running — subscribe and wait for its result.
            log::info!("GPEC: Another WebView fetch is already in-flight; joining it.");
            Some(tx.subscribe())
        } else {
            // We are the first caller — create the channel and store the sender.
            let (tx, _) = broadcast::channel(1);
            *guard = Some(tx);
            None
        }
    };

    if let Some(ref mut subscriber) = rx {
        // Secondary caller: wait for primary to broadcast the result.
        return subscriber
            .recv()
            .await
            .unwrap_or_else(|_| Err("GPEC in-flight fetch failed or was dropped".to_string()));
    }

    // Primary caller: do the actual WebView work.
    let result = do_webview_fetch(app).await;

    // Broadcast result to all waiting subscribers, then clear in-flight state.
    {
        let mut guard = gpec_inflight().lock().unwrap();
        if let Some(tx) = guard.take() {
            let _ = tx.send(result.clone());
        }
    }

    result
}

#[cfg(not(target_os = "android"))]
async fn do_webview_fetch(app: &tauri::AppHandle) -> Result<String, String> {
    log::info!("Starting GPEC WebView fetch (timeout 90s)...");
        
        if let Some(_existing) = app.get_webview_window("gpec_fetcher") {
            let _ = _existing.close();
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let script = r#"
            (function() {
                console.log('GPEC-FETCH: Script injected');

                // Block heavy third-party map SDKs & tracking scripts at document-start
                const blockedDomains = [
                    'google.com/maps',
                    'maps.googleapis.com',
                    'cookiebot.com',
                    'googletagmanager.com',
                    'facebook.net',
                    'clarity.ms',
                    'google-analytics.com'
                ];

                try {
                    const originalSrc = Object.getOwnPropertyDescriptor(HTMLScriptElement.prototype, 'src');
                    if (originalSrc && originalSrc.set) {
                        Object.defineProperty(HTMLScriptElement.prototype, 'src', {
                            set: function(val) {
                                if (typeof val === 'string' && blockedDomains.some(d => val.includes(d))) {
                                    console.log('GPEC-FETCH: Blocked heavy script src:', val);
                                    return;
                                }
                                originalSrc.set.call(this, val);
                            },
                            get: function() {
                                return originalSrc.get.call(this);
                            }
                        });
                    }

                    const origCreateElement = document.createElement;
                    document.createElement = function(tagName, options) {
                        const el = origCreateElement.call(document, tagName, options);
                        if (tagName && String(tagName).toLowerCase() === 'script') {
                            const origSetAttribute = el.setAttribute;
                            el.setAttribute = function(name, value) {
                                if (name === 'src' && typeof value === 'string' && blockedDomains.some(d => value.includes(d))) {
                                    console.log('GPEC-FETCH: Blocked script setAttribute:', value);
                                    return;
                                }
                                return origSetAttribute.call(this, name, value);
                            };
                        }
                        return el;
                    };
                } catch(e) {
                    console.error('GPEC-FETCH: Error installing script blocker', e);
                }

                function emitData() {
                    console.log('GPEC-FETCH: Final emission');
                    try {
                        let relevantHtml = '';
                        const noAcc = document.querySelector('.no-acc-info');
                        if (noAcc) relevantHtml += noAcc.outerHTML + '\n';
                        
                        const cloudInfos = document.querySelectorAll('.cloud-info');
                        if (cloudInfos.length > 0) {
                            cloudInfos.forEach(el => relevantHtml += el.outerHTML + '\n');
                        } else {
                            const dashed = document.querySelectorAll('.dashed');
                            dashed.forEach(el => relevantHtml += el.outerHTML + '\n');
                        }
                        
                        if (!relevantHtml.trim()) {
                            relevantHtml = 'Brak przerw';
                        }

                        const data = {
                            html: relevantHtml
                        };
                        
                        // Fallback: Navigation interception for strict CSP environments
                        const encodedData = encodeURIComponent(JSON.stringify(data));
                        window.location.href = "http://localhost/gpec_data_ready?data=" + encodedData;

                        if (window.__TAURI__ && window.__TAURI__.event) {
                            window.__TAURI__.event.emit('gpec_data_ready', data);
                        } else if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.emit) {
                            window.__TAURI_INTERNALS__.emit('gpec_data_ready', data);
                        } else if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) {
                            window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {
                                event: "gpec_data_ready",
                                payload: data
                            });
                        }
                    } catch(e) {
                        console.error('GPEC-FETCH: Emit failed', e);
                    }
                }

                function clickCookieConsent() {
                    const allowBtn = document.getElementById('CybotCookiebotDialogBodyLevelButtonLevelOptinAllowAll');
                    if (allowBtn && allowBtn.offsetParent !== null) {
                        console.log('GPEC-FETCH: Clicking Cookiebot consent');
                        allowBtn.click();
                    }
                }

                function check() {
                    const bodyHtml = document.body ? document.body.innerHTML : '';
                    const bodyText = document.body ? document.body.innerText : '';

                    if (document.title.includes('Just a moment') || bodyHtml.includes('Checking your browser') || bodyHtml.includes('Verify you are human')) {
                        console.log('GPEC-FETCH: Cloudflare challenge detected...');
                        return false;
                    }

                    if (document.querySelector('.no-acc-info') || document.querySelector('.dashed') || document.querySelector('.grupagpec-pl-przerwy-w-dostawie') || bodyText.includes('Brak przerw w dostawie') || bodyText.includes('Brak przerw')) {
                        emitData();
                        return true;
                    }

                    return false;
                }

                let attempts = 0;
                const interval = setInterval(() => {
                    attempts++;
                    clickCookieConsent();
                    if (check() || attempts > 120) {
                        clearInterval(interval);
                    }
                }, 1000);
            })();
        "#;

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
        let tx_clone = std::sync::Arc::clone(&tx);

        let builder = WebviewWindowBuilder::new(app, "gpec_fetcher", WebviewUrl::External(GPEC_URL.parse().unwrap()))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .initialization_script(script)
            .visible(false)
            .on_navigation(move |url| {
                let url_str = url.as_str();
                if url_str.starts_with("http://localhost/gpec_data_ready") {
                    if let Some(query) = url.query() {
                        if query.starts_with("data=") {
                            let encoded = query.trim_start_matches("data=");
                            if let Ok(decoded) = urlencoding::decode(encoded) {
                                log::info!("GPEC-FETCH: on_navigation successfully extracted HTML");
                                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&decoded) {
                                    if let Some(html) = parsed.get("html").and_then(|v| v.as_str()) {
                                        if let Some(tx) = tx_clone.lock().unwrap().take() {
                                            let _ = tx.send(html.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    return false; // Block navigation
                }
                true // Allow other navigations
            });

        let window = builder.build()
            .map_err(|e: tauri::Error| e.to_string())?;

        let result = match tokio::time::timeout(std::time::Duration::from_secs(90), rx).await {
            Ok(Ok(html)) => {
                log::info!("GPEC-FETCH: Success!");
                Ok(html)
            },
            Ok(Err(_)) => Err("Channel closed".to_string()),
            Err(_) => {
                Err("Timeout waiting for GPEC data (Cloudflare challenge might be too slow or blocking JS)".to_string())
            },
        };
        
        #[cfg(not(target_os = "android"))]
        let _ = window.close();
        
        result
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::AddressEntry;

    #[test]
    fn test_extract_dates() {
        let text = "Przerwa od 27.05.2026 godz. 08:00 do 28.05.2026 godz. 16:00 na ulicach Gdańska";
        let (start, end) = extract_dates(text);
        assert_eq!(start.unwrap().format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-27 08:00:00");
        assert_eq!(end.unwrap().format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-28 16:00:00");

        let text_fallback = "Awaria 27.05.2026 28.05.2026";
        let (start_f, end_f) = extract_dates(text_fallback);
        assert_eq!(start_f.unwrap().format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-27 00:00:00");
        assert_eq!(end_f.unwrap().format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-28 00:00:00");
    }

    #[test]
    fn test_extract_single_date() {
        let text = "Planowane od 27.05.2026 godz. 10:00";
        let (start, end) = extract_dates(text);
        assert_eq!(start.unwrap().format("%Y-%m-%d %H:%M:%S").to_string(), "2026-05-27 10:00:00");
        assert!(end.is_none());
    }

    #[test]
    fn test_local_matching() {
        let mut alert = UnifiedAlert {
            source: AlertSource::Gpec,
            startDate: None,
            endDate: None,
            message: Some("Awaria ogrzewania".to_string()),
            location: Some("Miejscowość: Gdańsk".to_string()),
            address_index: None,
            is_local: Some(false),
            hash: None,
        };

        let settings = Settings {
            addresses: vec![
                AddressEntry {
                    name: "Home".to_string(),
                    city_name: "Gdańsk".to_string(),
                    street_name_1: "Słowackiego".to_string(),
                    is_active: true,
                    ..Default::default()
                }
            ],
            ..Default::default()
        };

        let combined_text = "gdańsk ulica słowackiego 159b awaria ogrzewania".to_string();
        check_local_matching(&mut alert, &settings, "Gdańsk", &combined_text);
        assert_eq!(alert.is_local, Some(true));
        assert_eq!(alert.address_index, Some(0));
    }

    #[test]
    fn test_parse_gpec_html_mocked() {
        let html_content = r#"<div class="cloud-info-wrapper">
            <div class="row cloud-info red" style="max-height: 340px; overflow-y: scroll;">
                <h3>PRZERWA W DOSTAWIE</h3>
                <div class="dashed">
                    <span class="dashed__city">Gdańsk</span><br>
                    <div class="listed_steets" style="display: none;"><p>ul. Franciszka Schuberta 70</p></div>
                    <div class="all_streets" style="display: block;"><p>ul. Franciszka Schuberta 70</p></div>
                    <div id="show_streets" class="btn_streets">zwiń</div>
                </div>
                <p>wstrzymanie</p>        
                <span>2026-06-01</span>
                <p>Planowane wznowienie</p>        
                <span>2026-06-01 Godziny popołudniowe</span>                        
            </div>
        </div>"#;
        
        let settings = Settings::default();
        let alerts = parse_gpec_html(html_content, &settings);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-06-01T00:00:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-06-01T23:59:59"));
    }
}
