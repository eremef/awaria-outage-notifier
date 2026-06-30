use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use async_trait::async_trait;
use scraper::{Html, Selector};
use tauri::{AppHandle, Manager};
#[cfg(not(target_os = "android"))]
use tauri::{WebviewWindowBuilder, WebviewUrl, Event, Listener};
use crate::state_db;
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(not(target_os = "android"))]
use std::sync::{Mutex, OnceLock};
#[cfg(not(target_os = "android"))]
use tokio::sync::broadcast;
#[cfg(not(target_os = "android"))]
type InflightSender = Mutex<Option<broadcast::Sender<Result<String, String>>>>;

/// Singleflight guard: concurrent calls share one WebView fetch instead of
/// each spawning their own browser window.
#[cfg(not(target_os = "android"))]
static PSG_INFLIGHT: OnceLock<InflightSender> = OnceLock::new();

#[cfg(not(target_os = "android"))]
fn psg_inflight() -> &'static InflightSender {
    PSG_INFLIGHT.get_or_init(|| Mutex::new(None))
}

pub const PSG_URL: &str = "https://www.psgaz.pl/przerwy-w-dostawie-gazu";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub struct PsgProvider;

#[async_trait]
impl AlertProvider for PsgProvider {
    fn id(&self) -> String {
        "psg".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::Psg
    }

    async fn fetch(
        &self,
        _client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        app_handle: Option<&AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let active_addresses: Vec<_> = settings.addresses.iter().filter(|a| a.is_active && crate::api_logic::is_address_applicable_for_provider(&AlertSource::Psg, a)).collect();
        if active_addresses.is_empty() {
            return (Vec::new(), Vec::new());
        }

        if let Some(app) = app_handle {
            // 1. Try persistent HTML cache (1 hour TTL)
            if let Ok(Some(cached_html)) = get_cached_html(app).await {
                log::info!("PSG: Using cached HTML (1h TTL)");
                let alerts = parse_psg_html(&cached_html, settings);
                return (alerts, Vec::new());
            }

            // 2. Try direct fetch with cached cookies (25 min TTL)
            if let Ok(html) = try_direct_fetch_both(app).await {
                let alerts = parse_psg_html(&html, settings);
                // Stricter check: must look like the real table page or successful empty response
                if !alerts.is_empty() || html.contains("supply-interruptions") || html.contains("Brak trwających") || html.contains("Brak planowanych") {
                    log::info!("PSG: Data fetched from direct successfully");
                    let _ = save_cached_html(app, &html).await;
                    return (alerts, Vec::new());
                }
            }
            
             // 3. Fallback to WebView
             match fetch_via_webview(app).await {
                 Ok(html) => {
                     let alerts = parse_psg_html(&html, settings);
                     if !alerts.is_empty() || html.contains("supply-interruptions") || html.contains("Brak trwających") || html.contains("Brak planowanych") {
                         let _ = save_cached_html(app, &html).await;
                     }
                     (alerts, Vec::new())
                 }
                Err(e) => {
                    log::error!("PSG Fetch Error: {}. Trying to use stale cache...", e);
                    if let Ok(Some(stale_html)) = get_stale_html(app).await {
                        log::info!("PSG: Using stale HTML cache as fallback");
                        let alerts = parse_psg_html(&stale_html, settings);
                        return (alerts, vec![format!("PSG fetch error: {}. Showing stale data.", e)]);
                    }
                    (Vec::new(), vec![format!("PSG WebView error: {}", e)])
                }
            }
        } else {
            (Vec::new(), vec!["PSG: WebView fetch requires AppHandle".to_string()])
        }
    }
}

async fn try_direct_fetch_both(app: &AppHandle) -> Result<String, String> {
    let (cookies, cache_time) = {
        let db = app.state::<crate::state_db::DbState>();
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        
        let cookies = state_db::get_kv(&conn, "psg_cookies")?.ok_or("No cached cookies")?;
        
        let cache_time = state_db::get_kv(&conn, "psg_cookies_time")?
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        (cookies, cache_time)
    };
    
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    if now < cache_time || now - cache_time > 25 * 60 {
        return Err("Cookies expired".to_string());
    }

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    // Fetch active outages via POST
    let res_active = client.post(PSG_URL)
        .header("Cookie", &cookies)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body("state=active&sort_col=shutdownDateTime&sort_ord=asc&title=")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res_active.status().is_success() {
        return Err(format!("Active direct POST failed: {}", res_active.status()));
    }
    let html_active = res_active.text().await.map_err(|e| e.to_string())?;

    // Fetch planned outages via POST
    let res_planned = client.post(PSG_URL)
        .header("Cookie", &cookies)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body("state=disabled&sort_col=shutdownDateTime&sort_ord=asc&title=")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res_planned.status().is_success() {
        return Err(format!("Planned direct POST failed: {}", res_planned.status()));
    }
    let html_planned = res_planned.text().await.map_err(|e| e.to_string())?;

    // Combine them with <hr> separator so that parse_psg_html can parse both tables
    let combined_html = format!("{}\n<hr>\n{}", html_active, html_planned);
    
    Ok(combined_html)
}async fn fetch_via_webview(#[allow(unused_variables)] app: &AppHandle) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        log::info!("Starting native PSG fetch on Android...");
        crate::get_psg_html_android().await
    }

    // Singleflight: on desktop, at most one WebView fetch runs at a time.
    // Additional concurrent callers subscribe to the in-flight broadcast and
    // receive the result the moment the first caller finishes.
    #[cfg(not(target_os = "android"))]
    singleflight_webview_fetch(app).await
}

#[cfg(not(target_os = "android"))]
async fn singleflight_webview_fetch(app: &AppHandle) -> Result<String, String> {
    let mut rx: Option<broadcast::Receiver<Result<String, String>>> = {
        let mut guard = psg_inflight().lock().unwrap();
        if let Some(tx) = guard.as_ref() {
            log::info!("PSG: Another WebView fetch is already in-flight; joining it.");
            Some(tx.subscribe())
        } else {
            let (tx, _) = broadcast::channel(1);
            *guard = Some(tx);
            None
        }
    };

    if let Some(ref mut subscriber) = rx {
        return subscriber
            .recv()
            .await
            .unwrap_or_else(|_| Err("PSG in-flight fetch failed or was dropped".to_string()));
    }

    let result = do_webview_fetch(app).await;

    {
        let mut guard = psg_inflight().lock().unwrap();
        if let Some(tx) = guard.take() {
            let _ = tx.send(result.clone());
        }
    }

    result
}

#[cfg(not(target_os = "android"))]
async fn do_webview_fetch(app: &AppHandle) -> Result<String, String> {
    log::info!("Starting PSG WebView fetch (timeout 90s)...");
        
        if let Some(_existing) = app.get_webview_window("psg_fetcher") {
            let _ = _existing.close();
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let script = r#"
            (function() {
                console.log('PSG-FETCH: Script injected');

                let activeCaptured = sessionStorage.getItem('psg_activeCaptured') === 'true';
                let plannedCaptured = sessionStorage.getItem('psg_plannedCaptured') === 'true';
                let activeHtml = sessionStorage.getItem('psg_activeHtml') || '';
                let plannedHtml = sessionStorage.getItem('psg_plannedHtml') || '';
                
                let startTimeStr = sessionStorage.getItem('psg_startTime');
                if (!startTimeStr) {
                    startTimeStr = Date.now().toString();
                    sessionStorage.setItem('psg_startTime', startTimeStr);
                    sessionStorage.setItem('psg_lastActionTime', startTimeStr);
                }
                let startTime = parseInt(startTimeStr);

                function isTableMatchingState(state) {
                    const container = document.getElementById('supply-interruptions-filter-form') || document.body;
                    const text = container.innerText || '';
                    
                    if (state === 'active') {
                        if (text.includes('Brak trwających') || text.includes('Brak przerw')) {
                            return true;
                        }
                    } else {
                        if (text.includes('Brak planowanych')) {
                            return true;
                        }
                    }
                    
                    const rows = document.querySelectorAll('table tr');
                    for (let i = 1; i < rows.length; i++) {
                        const rowText = rows[i].innerText || '';
                        if (state === 'active') {
                            if (rowText.toLowerCase().includes('awaria') || rowText.toLowerCase().includes('aktywna')) {
                                return true;
                            }
                        } else {
                            if (rowText.toLowerCase().includes('planowane') || rowText.toLowerCase().includes('planowana')) {
                                return true;
                            }
                        }
                    }
                    
                    // Fallback: if we have waited more than 5 seconds after clicking, assume it loaded
                    const lastAction = parseInt(sessionStorage.getItem('psg_lastActionTime') || '0');
                    if (lastAction > 0 && (Date.now() - lastAction > 5000)) {
                        console.log('PSG-FETCH: Matching state fallback triggered');
                        return true;
                    }
                    
                    return false;
                }

                function emitData() {
                    console.log('PSG-FETCH: Final emission');
                    try {
                        const data = {
                            html: (activeHtml || '') + "\n<hr>\n" + (plannedHtml || ''),
                            cookies: document.cookie
                        };
                        if (window.__TAURI__ && window.__TAURI__.event) {
                            window.__TAURI__.event.emit('psg_data_ready', data);
                        } else if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.emit) {
                            window.__TAURI_INTERNALS__.emit('psg_data_ready', data);
                        }
                    } catch(e) {
                        console.error('PSG-FETCH: Emit failed', e);
                    }
                    
                    sessionStorage.removeItem('psg_activeCaptured');
                    sessionStorage.removeItem('psg_plannedCaptured');
                    sessionStorage.removeItem('psg_activeHtml');
                    sessionStorage.removeItem('psg_plannedHtml');
                    sessionStorage.removeItem('psg_startTime');
                    sessionStorage.removeItem('psg_lastActionTime');
                }

                function check() {
                    const now = Date.now();
                    const body = document.body ? document.body.innerHTML : '';

                    if (body.includes('Checking your browser') || body.includes('Verify you are human') || body.includes('Cloudflare')) {
                        console.log('PSG-FETCH: Cloudflare challenge detected...');
                        return false;
                    }

                    const checkbox0 = document.getElementById('checkbox0'); // active (aktywna)
                    const checkbox1 = document.getElementById('checkbox1'); // planned (planowana)

                    const isActiveChecked = checkbox0 && checkbox0.checked;
                    const isPlannedChecked = checkbox1 && checkbox1.checked;

                    if (isActiveChecked) {
                        if (!activeCaptured && isTableMatchingState('active')) {
                            console.log('PSG-FETCH: Captured Active view');
                            activeHtml = body;
                            activeCaptured = true;
                            sessionStorage.setItem('psg_activeHtml', body);
                            sessionStorage.setItem('psg_activeCaptured', 'true');
                        }
                    } else if (isPlannedChecked) {
                        if (!plannedCaptured && isTableMatchingState('planned')) {
                            console.log('PSG-FETCH: Captured Planned view');
                            plannedHtml = body;
                            plannedCaptured = true;
                            sessionStorage.setItem('psg_plannedHtml', body);
                            sessionStorage.setItem('psg_plannedCaptured', 'true');
                        }
                    }

                    if (activeCaptured && plannedCaptured) {
                        emitData();
                        return true;
                    }

                    if (!activeCaptured) {
                        if (!isActiveChecked && checkbox0) {
                            console.log('PSG-FETCH: Switching to Active...');
                            sessionStorage.setItem('psg_lastActionTime', now.toString());
                            checkbox0.click();
                            checkbox0.dispatchEvent(new Event('change', { bubbles: true }));
                        }
                        // Always return to let the active view load before attempting to switch to planned
                        return false;
                    }

                    if (!plannedCaptured) {
                        if (!isPlannedChecked && checkbox1) {
                            console.log('PSG-FETCH: Switching to Planned...');
                            sessionStorage.setItem('psg_lastActionTime', now.toString());
                            checkbox1.click();
                            checkbox1.dispatchEvent(new Event('change', { bubbles: true }));
                        }
                        return false;
                    }

                    if (now - startTime > 45000) {
                        console.log('PSG-FETCH: Safety timeout. Emitting partial data.');
                        emitData();
                        return true;
                    }

                    return false;
                }

                let attempts = 0;
                const interval = setInterval(() => {
                    attempts++;
                    if (check() || attempts > 120) {
                        clearInterval(interval);
                    }
                }, 1000);
            })();
        "#;

        let mut builder = WebviewWindowBuilder::new(app, "psg_fetcher", WebviewUrl::External(PSG_URL.parse().unwrap()))
            .user_agent(USER_AGENT)
            .initialization_script(script);

        #[cfg(not(target_os = "android"))]
        {
            #[cfg(debug_assertions)]
            let visible = true;
            #[cfg(not(debug_assertions))]
            let visible = false;

            builder = builder.title("PSG Fetcher").visible(visible);
        }

        let window = builder.build()
            .map_err(|e: tauri::Error| e.to_string())?;

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));

        // Helper to log state (screenshot removed due to compilation error in core Tauri v2)
        let log_state = move |label: &str| {
            log::info!("PSG-FETCH: Debugging state: {}", label);
        };

        let ls_clone = log_state;
        let app_clone = app.clone();
        
        // Periodic debug listener
        let _ts_id = app.listen("psg_debug_screenshot", move |_event: Event| {
            log::info!("PSG-FETCH: Periodic check triggered...");
        });

        let _id = app.listen("psg_data_ready", move |event: Event| {
            log::info!("PSG-FETCH: Received psg_data_ready event!");
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                if let Some(cookies) = data.get("cookies").and_then(|v| v.as_str()) {
                    let _ = save_cookies(&app_clone, cookies);
                }
                if let Some(html) = data.get("html").and_then(|v| v.as_str()) {
                    ls_clone("success");
                    
                    if let Some(tx) = tx.lock().unwrap().take() {
                        let _ = tx.send(html.to_string());
                    }
                }
            }
        });

        let result = match tokio::time::timeout(std::time::Duration::from_secs(90), rx).await {
            Ok(Ok(html)) => {
                log::info!("PSG-FETCH: Success!");
                Ok(html)
            },
            Ok(Err(_)) => Err("Channel closed".to_string()),
            Err(_) => {
                log_state("timeout");
                Err("Timeout waiting for PSG data (Cloudflare challenge might be too slow or blocking JS)".to_string())
            },
        };
        
        #[cfg(not(target_os = "android"))]
        let _ = window.close();
        
        result
}


#[cfg(not(target_os = "android"))]
fn save_cookies(app: &AppHandle, cookies: &str) -> Result<(), String> {
    let db = app.state::<crate::state_db::DbState>();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    
    state_db::set_kv(&conn, "psg_cookies", cookies)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    state_db::set_kv(&conn, "psg_cookies_time", &now.to_string())?;
    
    Ok(())
}

async fn get_cached_html(app: &AppHandle) -> Result<Option<String>, String> {
    let db = app.state::<crate::state_db::DbState>();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    
    let cache_time = state_db::get_kv(&conn, "psg_html_time")?
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    if now >= cache_time && now - cache_time < 3600 { // 1 hour TTL
        return state_db::get_kv(&conn, "psg_html_cache");
    }
    
    Ok(None)
}

async fn get_stale_html(app: &AppHandle) -> Result<Option<String>, String> {
    let db = app.state::<crate::state_db::DbState>();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    state_db::get_kv(&conn, "psg_html_cache")
}

async fn save_cached_html(app: &AppHandle, html: &str) -> Result<(), String> {
    let db = app.state::<crate::state_db::DbState>();
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    
    state_db::set_kv(&conn, "psg_html_cache", html)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    state_db::set_kv(&conn, "psg_html_time", &now.to_string())?;
    
    Ok(())
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            _ => c,
        })
        .filter(|c| c.is_alphanumeric())
        .collect()
}



fn get_core_street_name(street: &str) -> String {
    let words: Vec<&str> = street.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }
    for &word in words.iter().rev() {
        let normalized_word = normalize(word);
        if normalized_word.is_empty() {
            continue;
        }
        let is_roman = matches!(normalized_word.as_str(),
            "i" | "ii" | "iii" | "iv" | "v" | "vi" | "vii" | "viii" | "ix" | "x"
        );
        let is_numeric = normalized_word.chars().all(|c| c.is_numeric());
        if !is_roman && !is_numeric && normalized_word.len() >= 3 {
            return normalized_word;
        }
    }
    normalize(words.last().unwrap())
}

pub fn parse_psg_html(html_content: &str, settings: &Settings) -> Vec<UnifiedAlert> {
    let mut alerts = Vec::new();
    let document = Html::parse_document(html_content);
    
    let row_selector = Selector::parse("tr").unwrap();
    let td_selector = Selector::parse("td").unwrap();

    let mut row_count = 0;
    for row in document.select(&row_selector) {
        row_count += 1;
        let cells: Vec<_> = row.select(&td_selector).collect();
        let cell_texts: Vec<String> = cells.iter()
            .map(|c| {
                c.text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .replace("&nbsp;", " ")
                    .replace("&amp;", "&")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        
        println!("[PSG] Row {}: cells (count={})={:?}", row_count, cells.len(), cell_texts);

        if cells.len() >= 7 {
            let city = cell_texts[1].clone();
            let area = cell_texts[2].clone();
            
            // Skip header row
            if city.contains("Miejscowość") || area.contains("Obszar") {
                continue;
            }
            
            // Skip empty rows
            if cell_texts.iter().any(|t| t.contains("Brak trwających przerw") || t.contains("Brak przerw")) {
                continue;
            }

            let start_date = cell_texts[3].clone();
            let end_date = cell_texts[4].clone();
            
            // Reason / Cause
            let reason = cell_texts[5].clone();
            let outage_type = cell_texts[6].clone();
            
            // Status is usually the last or second to last cell
            let status = if cells.len() >= 8 { cell_texts[7].clone() } else { cell_texts[cells.len() - 1].clone() };
            let status_low = status.to_lowercase();

            if status_low.contains("zakończona") || status_low.contains("zakonczona") {
                continue;
            }

            let mut matched_indices = Vec::new();

            let norm_city = normalize(&city);
            let norm_area = normalize(&area);

            for (idx, addr) in settings.addresses.iter().enumerate().filter(|(_, a)| a.is_active && crate::api_logic::is_address_applicable_for_provider(&AlertSource::Psg, a)) {
                let addr_street = normalize(&addr.street_name_1);

                // For direct matching, check if the normalized scraped city matches the user's city
                let addr_city = normalize(&addr.city_name);
                let city_match = norm_city == addr_city 
                    || norm_city.contains(&addr_city) 
                    || addr_city.contains(&norm_city);
                
                // If city didn't match directly, check if the city name is mentioned in the AREA field
                let city_in_area = !city_match && (norm_area.contains(&addr_city) || addr_city.contains(&norm_area));
                
                // Check if the outage is locality-wide
                let is_locality_wide = {
                    let city_with_prefix = format!("m{}", norm_city);
                    norm_area == norm_city 
                        || norm_area == city_with_prefix
                        || norm_area.contains(&city_with_prefix)
                        || norm_area.contains("calamiejscowosc")
                        || norm_area.contains("calyobszarmiejscowosci")
                };

                let clean_addr_street = crate::utils::strip_street_prefixes(&addr_street);
                let core_street = get_core_street_name(&addr.street_name_1);

                let street_match = if addr_street.is_empty() || is_locality_wide {
                    true 
                } else {
                    norm_area.contains(&addr_street)
                        || (!clean_addr_street.is_empty() && norm_area.contains(clean_addr_street))
                        || (!core_street.is_empty() && norm_area.contains(&core_street))
                };

                if (city_match || city_in_area) && street_match {
                    println!("[PSG] Match found for addr {}: city={}, area={}", addr.city_name, city, area);
                    matched_indices.push(idx);
                } else if city_match || city_in_area {
                    println!("[PSG] City match only for addr {}: city={}, area={}", addr.city_name, city, area);
                }
            }

            if !matched_indices.is_empty() {
                let reason_trimmed = reason.trim();
                let final_message = if reason_trimmed.is_empty() || reason_trimmed == "Info Button Text" || reason_trimmed == "Info" {
                    outage_type.clone() + " - " + &area.clone()
                } else {
                    format!("{}: {}", reason_trimmed, outage_type.clone() + " - " + &area)
                };

                let parsed_start = crate::utils::parse_date(&start_date).map(|dt| dt.to_rfc3339()).unwrap_or(start_date.clone());
                let parsed_end = crate::utils::parse_date(&end_date).map(|dt| dt.to_rfc3339()).unwrap_or(end_date.clone());

                for &idx in &matched_indices {
                    alerts.push(UnifiedAlert {
                        source: AlertSource::Psg,
                        startDate: Some(parsed_start.clone()),
                        endDate: Some(parsed_end.clone()),
                        message: Some(final_message.clone()),
                        location: Some(format!("Miejscowość: {}", city)),
                        address_index: Some(idx),
                        is_local: Some(true),
                        hash: None,
                    });
                }
            }
        }
    }
    
    println!("[PSG] Found {} rows, parsed {} alerts", row_count, alerts.len());
    alerts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_logic::{AddressEntry, Settings};

    #[test]
    fn test_parse_psg_html_mock() {
        let html = r#"
            <table>
                <tr>
                    <td>Wielkopolskie</td>
                    <td>Poznań</td>
                    <td>ul. Bratumiły, Bożymira</td>
                    <td>2024-05-20 10:00</td>
                    <td>2024-05-20 14:00</td>
                    <td>Prace serwisowe</td>
                    <td>Planowana</td>
                    <td>Aktywna</td>
                </tr>
            </table>
        "#;
        
        let settings = Settings {
            addresses: vec![
                AddressEntry {
                    city_name: "Poznań".to_string(),
                    street_name_1: "Bratumiły".to_string(),
                    is_active: true,
                    ..Default::default()
                }
            ],
            ..Default::default()
        };
        
        let alerts = parse_psg_html(html, &settings);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].source, AlertSource::Psg);
    }

    #[test]
    fn test_wschowa_active_outage() {
        let html = r#"
            <table>
                <tr>
                    <td>Lubuskie</td>
                    <td>Wschowa</td>
                    <td>Wschowa ul 31 – go Stycznia 1-19, ul Grunwaldu 1,3 ul Wolsztyńska 11, 13,15.</td>
                    <td>17.05.2026 godz. 17:30</td>
                    <td>termin zostanie podany wkrótce</td>
                    <td>Info Button Text</td>
                    <td>awaria</td>
                    <td>aktywna</td>
                </tr>
            </table>
        "#;
        
        let settings = Settings {
            addresses: vec![
                AddressEntry {
                    city_name: "Wschowa".to_string(),
                    street_name_1: "Wolsztyńska".to_string(),
                    is_active: true,
                    ..Default::default()
                },
                AddressEntry {
                    city_name: "Wschowa".to_string(),
                    street_name_1: "Plac Grunwaldu".to_string(),
                    is_active: true,
                    ..Default::default()
                }
            ],
            ..Default::default()
        };
        
        let alerts = parse_psg_html(html, &settings);
        assert_eq!(alerts.len(), 2, "Both streets should match the outage");
    }

    #[test]
    fn test_bialystok_abbreviated_outage() {
        let html = r#"
            <table>
                <tr>
                    <td>Podlaskie</td>
                    <td>Białystok</td>
                    <td>BIAŁYSTOK ul. L.Mierosławskiego, W.Lewandowskiego, Wąska, Fabryczna, W.Siedleckiego, Węglowa, Błękitna, Wasilkowska, W.Łokietka, K.Chodakowskiego, K.Z.Agusta, S.Żółkiewskiego, WŁ.Jagiełły, W.Warneńczyka, Królowej Jadwigi, S.Batorego, gen.W.Andersa.</td>
                    <td>18.05.2026 godz. 10:00</td>
                    <td>18.05.2026 godz. 14:00</td>
                    <td>Planowane</td>
                    <td>Planowana</td>
                    <td>Aktywna</td>
                </tr>
            </table>
        "#;
        
        let settings = Settings {
            addresses: vec![
                AddressEntry {
                    city_name: "Białystok".to_string(),
                    street_name_1: "Stanisława Żółkiewskiego".to_string(),
                    is_active: true,
                    ..Default::default()
                }
            ],
            ..Default::default()
        };
        
        let alerts = parse_psg_html(html, &settings);
        assert_eq!(alerts.len(), 1, "Stanisława Żółkiewskiego should match S.Żółkiewskiego in Bialystok");
    }



    #[test]
    fn test_brzezowka_no_streets() {
        let html = r#"
            <table>
                <tr>
                    <td>Podkarpackie</td>
                    <td>Brzezówka</td>
                    <td>m. Brzezówka gm. Ropczyce</td>
                    <td>2024-05-20 10:00</td>
                    <td>2024-05-20 14:00</td>
                    <td>Info</td>
                    <td>Planowana</td>
                    <td>Aktywna</td>
                </tr>
            </table>
        "#;
        
        let settings = Settings {
            addresses: vec![
                AddressEntry {
                    city_name: "Brzezówka".to_string(),
                    street_name_1: "".to_string(), // No streets
                    is_active: true,
                    ..Default::default()
                }
            ],
            ..Default::default()
        };
        
        let alerts = parse_psg_html(html, &settings);
        assert_eq!(alerts.len(), 1, "Should match city without streets");
    }

    #[test]
    fn test_brzezowka_with_street_locality_wide() {
        let html = r#"
            <table>
                <tr>
                    <td>Podkarpackie</td>
                    <td>Brzezówka</td>
                    <td>m. Brzezówka gm. Ropczyce</td>
                    <td>2024-05-20 10:00</td>
                    <td>2024-05-20 14:00</td>
                    <td>Info</td>
                    <td>Planowana</td>
                    <td>Aktywna</td>
                </tr>
            </table>
        "#;
        
        let settings = Settings {
            addresses: vec![
                AddressEntry {
                    city_name: "Brzezówka".to_string(),
                    street_name_1: "Główna".to_string(), // Specific street
                    is_active: true,
                    ..Default::default()
                }
            ],
            ..Default::default()
        };
        
        let alerts = parse_psg_html(html, &settings);
        assert_eq!(alerts.len(), 1, "Locality-wide outage should match specific street");
    }

    #[test]
    fn test_psg_wschowa() {
        let html = r#"
            <table>
                <tr>
                    <td>LUBUSKIE</td>
                    <td>Wschowa</td>
                    <td>ul. Wolsztyńska</td>
                    <td>2024-05-20</td>
                    <td>termin zostanie podany wkrótce</td>
                    <td>Awaria gazociągu</td>
                    <td>awaria</td>
                </tr>
            </table>
        "#;
        
        let settings = Settings {
            addresses: vec![
                AddressEntry {
                    city_name: "Wschowa".to_string(),
                    street_name_1: "ul. Wolsztyńska".to_string(), // Specific street
                    is_active: true,
                    ..Default::default()
                }
            ],
            ..Default::default()
        };
        
        let alerts = parse_psg_html(html, &settings);
        assert_eq!(alerts.len(), 1, "awaria in Wschowa should match");
    }
}
