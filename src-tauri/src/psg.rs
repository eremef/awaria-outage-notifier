use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use async_trait::async_trait;
use scraper::{Html, Selector};
use tauri::{AppHandle, Manager};
#[cfg(not(target_os = "android"))]
use tauri::{WebviewWindowBuilder, WebviewUrl, Event, Listener};
use crate::state_db;
use std::time::{SystemTime, UNIX_EPOCH};

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
        let active_addresses: Vec<_> = settings.addresses.iter().filter(|a| a.is_active).collect();
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
            if let Ok(html) = try_direct_fetch_with_cache(app).await {
                let alerts = parse_psg_html(&html, settings);
                // Stricter check: must look like the real table page
                if !alerts.is_empty() || (html.contains("Wyłączenie od") && html.contains("Miejscowość")) {
                    log::info!("PSG: Data fetched from direct successfully");
                    let _ = save_cached_html(app, &html).await;
                    return (alerts, Vec::new());
                }
            }
            
            // 3. Fallback to WebView
            match fetch_via_webview(app).await {
                Ok(html) => {
                    let alerts = parse_psg_html(&html, settings);
                    if !alerts.is_empty() || (html.contains("Wyłączenie od") && html.contains("Miejscowość")) {
                        let _ = save_cached_html(app, &html).await;
                    }
                    (alerts, Vec::new())
                }
                Err(e) => {
                    log::error!("PSG Fetch Error: {}", e);
                    (Vec::new(), vec![format!("PSG WebView error: {}", e)])
                }
            }
        } else {
            (Vec::new(), vec!["PSG: WebView fetch requires AppHandle".to_string()])
        }
    }
}

async fn try_direct_fetch_with_cache(app: &AppHandle) -> Result<String, String> {
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
    if now - cache_time > 25 * 60 {
        return Err("Cookies expired".to_string());
    }

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let res = client.get(PSG_URL)
        .header("Cookie", cookies)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = res.status();
    if status.is_success() {
        let text = res.text().await.map_err(|e| e.to_string())?;
        // Stricter check: must contain table headers to be considered successful
        if (text.contains("województwo") || text.contains("miejscowość")) && (text.contains("obszar") || text.contains("<table")) {
            return Ok(text);
        }
    }
    
    Err(format!("Direct fetch failed (or blocked by Cloudflare): {}", status))
}async fn fetch_via_webview(#[allow(unused_variables)] app: &AppHandle) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        log::info!("Starting native PSG fetch on Android...");
        crate::get_psg_html_android().await
    }

    #[cfg(not(target_os = "android"))]
    {
        log::info!("Starting PSG WebView fetch (timeout 90s)...");
        
        if let Some(_existing) = app.get_webview_window("psg_fetcher") {
            let _ = _existing.close();
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let script = r#"
            (function() {
                console.log('PSG-FETCH: Script injected');
                
                function trySwitchToPlanned() {
                    // Specific ID provided by user
                    const checkbox1 = document.getElementById('checkbox1');
                    if (checkbox1) {
                        console.log('PSG-FETCH: Found #checkbox1, checked=' + checkbox1.checked);
                        if (!checkbox1.checked) {
                            console.log('PSG-FETCH: Clicking #checkbox1...');
                            checkbox1.click();
                            
                            // Sometimes we need to click the label or a submit button if click doesn't trigger AJAX
                            // But usually checkboxes on these forms have onchange=submit()
                            window._waitingForRefresh = true;
                            window._triedPlanned = true;
                            setTimeout(() => { window._waitingForRefresh = false; }, 3000); 
                            return true;
                        }
                    }

                    const interactive = Array.from(document.querySelectorAll('button, a, span, li, label, input'));
                    const plannedBtn = interactive.find(el => {
                        const text = el.innerText || (el.value && typeof el.value === 'string' ? el.value : '');
                        return /planowane/i.test(text.trim());
                    });
                    
                    if (plannedBtn) {
                        const isAlreadyActive = plannedBtn.classList.contains('active') || 
                                               plannedBtn.classList.contains('selected') ||
                                               (plannedBtn.parentElement && plannedBtn.parentElement.classList.contains('active')) ||
                                               (plannedBtn.tagName === 'INPUT' && plannedBtn.checked);
                        
                        if (!isAlreadyActive) {
                            console.log('PSG-FETCH: Clicking button:', plannedBtn.tagName, 'Text:', plannedBtn.innerText);
                            plannedBtn.click();
                            window._waitingForRefresh = true;
                            window._triedPlanned = true;
                            setTimeout(() => { window._waitingForRefresh = false; }, 3000); 
                            return true;
                        }
                    }
                    return false;
                }

                window._psgState = 'capture_active';
                window._activeHtml = '';
                window._plannedHtml = '';
                window._startTime = Date.now();
                window._lastSwitchTime = 0;

                function check() {
                    const now = Date.now();
                    const body = document.body ? document.body.innerHTML : '';
                    const text = document.body ? document.body.innerText : '';
                    const hasTable = body.includes('<table') || body.includes('<tbody>') || body.includes('supply-interruptions');
                    const isBrak = text.includes('Brak') || text.includes('przerw');

                    if (window._waitingForRefresh && (now - window._lastSwitchTime < 5000)) {
                        return false; 
                    }
                    window._waitingForRefresh = false;

                    switch(window._psgState) {
                        case 'capture_active':
                            if (hasTable || isBrak || (now - window._startTime > 5000)) {
                                console.log('PSG-FETCH: Captured Active view');
                                window._activeHtml = body;
                                window._psgState = 'switching';
                            }
                            break;

                        case 'switching':
                            console.log('PSG-FETCH: Attempting switch to Planned...');
                            if (trySwitchToPlanned()) {
                                window._psgState = 'capture_planned';
                                window._waitingForRefresh = true;
                                window._lastSwitchTime = now;
                            } else {
                                // If can't switch, just emit what we have
                                window._psgState = 'emit';
                            }
                            break;

                        case 'capture_planned':
                            if (hasTable || isBrak || (now - window._lastSwitchTime > 5000)) {
                                console.log('PSG-FETCH: Captured Planned view');
                                window._plannedHtml = body;
                                window._psgState = 'emit';
                            }
                            break;

                        case 'emit':
                            console.log('PSG-FETCH: Final emission');
                            try {
                                const data = {
                                    html: (window._activeHtml || '') + "\n<hr>\n" + (window._plannedHtml || ''),
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
                            return true;
                    }
                    
                    if (body.includes('Checking your browser') || body.includes('Verify you are human') || body.includes('Cloudflare')) {
                        console.log('PSG-FETCH: Cloudflare challenge detected...');
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

        #[cfg(desktop)]
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
        
        #[cfg(desktop)]
        let _ = window.close();
        
        result
    }
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
    if now - cache_time < 60 { // 1 minute during debug
        return state_db::get_kv(&conn, "psg_html_cache");
    }
    
    Ok(None)
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
        .filter(|c| c.is_alphanumeric())
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
        .collect()
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
            let message = cell_texts[5].clone();
            
            // Status is usually the last or second to last cell
            let status = if cells.len() >= 8 { cell_texts[7].clone() } else { cell_texts[cells.len() - 1].clone() };
            let status_low = status.to_lowercase();

            if status_low.contains("zakończona") || status_low.contains("zakonczona") {
                continue;
            }

            let mut matched_index = None;
            let mut is_local = false;

            let norm_city = normalize(&city);
            let norm_area = normalize(&area);

            for (idx, addr) in settings.addresses.iter().enumerate().filter(|(_, a)| a.is_active) {
                let addr_city = normalize(&addr.city_name);
                let addr_street = normalize(&addr.street_name_1);

                // City match: exact or contains
                let city_match = norm_city == addr_city || norm_city.contains(&addr_city) || addr_city.contains(&norm_city);
                
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

                let street_match = if addr_street.is_empty() || is_locality_wide {
                    true 
                } else {
                    norm_area.contains(&addr_street)
                };

                if (city_match || city_in_area) && street_match {
                    println!("[PSG] Match found for addr {}: city={}, area={}", addr.city_name, city, area);
                    matched_index = Some(idx);
                    is_local = true;
                    break;
                } else if city_match || city_in_area {
                    println!("[PSG] City match only for addr {}: city={}, area={}", addr.city_name, city, area);
                }
            }

            if is_local {
                alerts.push(UnifiedAlert {
                    source: AlertSource::Psg,
                    startDate: Some(start_date),
                    endDate: Some(end_date),
                    message: Some(message.clone()),
                    description: Some(format!("Miejscowość: {}, Obszar: {}", city, area)),
                    address_index: matched_index,
                    is_local: Some(true),
                    hash: None,
                });
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
}
