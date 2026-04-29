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
                if !alerts.is_empty() || html.contains("województwo") || html.contains("Polska Spółka Gazownictwa") || html.contains("Przerwy w dostawie gazu") {
                    log::info!("PSG: Data fetched from direct successfully");
                    let _ = save_cached_html(app, &html).await;
                    return (alerts, Vec::new());
                }
            }
            
            // 3. Fallback to WebView
            match fetch_via_webview(app).await {
                Ok(html) => {
                    let alerts = parse_psg_html(&html, settings);
                    let _ = save_cached_html(app, &html).await;
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
        if text.contains("województwo") || text.contains("supply-interruptions") || text.contains("Polska Spółka Gazownictwa") || text.contains("Przerwy w dostawie gazu") {
            return Ok(text);
        }
    }
    
    Err(format!("Direct fetch failed: {}", status))
}async fn fetch_via_webview(#[allow(unused_variables)] app: &AppHandle) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        log::warn!("PSG WebView fetch skipped on Android to avoid showing the website activity.");
        return Err("WebView fetch not supported on Android".to_string());
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
                function check() {
                    const body = document.body ? document.body.innerHTML : '';
                    if (body.includes('województwo') || body.includes('supply-interruptions') || body.includes('miejscowość') || body.includes('Polska Spółka Gazownictwa') || body.includes('Przerwy w dostawie gazu') || body.includes('Brak przerw')) {
                        console.log('PSG-FETCH: Data found, emitting...');
                        const data = {
                            cookies: document.cookie,
                            html: document.documentElement.outerHTML
                        };
                        
                        const payload = JSON.stringify(data);
                        
                        // Try to emit via all possible channels
                        try {
                            if (window.__TAURI__ && window.__TAURI__.event) {
                                window.__TAURI__.event.emit('psg_data_ready', data);
                            }
                        } catch(e) {}
                        
                        try {
                            if (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.emit) {
                                window.__TAURI_INTERNALS__.emit('psg_data_ready', data);
                            }
                        } catch(e) {}

                        // Low-level IPC fallback for remote pages where Tauri might not be fully injected
                        try {
                            const ipcMsg = JSON.stringify({
                                cmd: 'emit',
                                event: 'psg_data_ready',
                                payload: data
                            });
                            if (window.chrome && window.chrome.webview && window.chrome.webview.postMessage) {
                                window.chrome.webview.postMessage(ipcMsg);
                            } else if (window.ipc && window.ipc.postMessage) {
                                window.ipc.postMessage(ipcMsg);
                            }
                        } catch(e) {}

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
                    if (check() || attempts > 80) {
                        clearInterval(interval);
                    }
                }, 1000);
                check();
            })();
        "#;

        let mut builder = WebviewWindowBuilder::new(app, "psg_fetcher", WebviewUrl::External(PSG_URL.parse().unwrap()))
            .user_agent(USER_AGENT)
            .initialization_script(script);

        #[cfg(desktop)]
        {
            builder = builder.title("PSG Fetcher").visible(false);
        }

        let window = builder.build()
            .map_err(|e: tauri::Error| e.to_string())?;

        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
        
        let app_clone = app.clone();
        // Listen globally since events from remote pages might be weirdly scoped
        let _id = app.listen("psg_data_ready", move |event: Event| {
            log::info!("PSG-FETCH: Received psg_data_ready event!");
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                if let Some(cookies) = data.get("cookies").and_then(|v| v.as_str()) {
                    let _ = save_cookies(&app_clone, cookies);
                }
                if let Some(html) = data.get("html").and_then(|v| v.as_str()) {
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
            Err(_) => Err("Timeout waiting for PSG data (Cloudflare challenge might be too slow or blocking JS)".to_string()),
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
    if now - cache_time < 60 * 60 { // 1 hour
        return Ok(state_db::get_kv(&conn, "psg_html_cache")?);
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
        .replace('ą', "a")
        .replace('ć', "c")
        .replace('ę', "e")
        .replace('ł', "l")
        .replace('ń', "n")
        .replace('ó', "o")
        .replace('ś', "s")
        .replace('ź', "z")
        .replace('ż', "z")
        .trim()
        .to_string()
}

pub fn parse_psg_html(html_content: &str, settings: &Settings) -> Vec<UnifiedAlert> {
    let mut alerts = Vec::new();
    let document = Html::parse_document(html_content);
    
    let row_selector = Selector::parse("tr").unwrap();
    let td_selector = Selector::parse("td").unwrap();

    for row in document.select(&row_selector) {
        let cells: Vec<_> = row.select(&td_selector).collect();
        if cells.len() >= 8 {
            let city = cells[1].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let area = cells[2].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let start_date = cells[3].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let end_date = cells[4].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let message = cells[5].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let status = cells[7].text().collect::<Vec<_>>().join(" ").trim().to_string();

            if status.to_lowercase().contains("zakończona") {
                continue;
            }

            let mut matched_index = None;
            let mut is_local = false;

            let norm_city = normalize(&city);
            let norm_area = normalize(&area);

            for (idx, addr) in settings.addresses.iter().enumerate().filter(|(_, a)| a.is_active) {
                let addr_city = normalize(&addr.city_name);
                let addr_street = normalize(&addr.street_name_1);

                if norm_city == addr_city || norm_city.contains(&addr_city) || addr_city.contains(&norm_city) {
                    if norm_area.contains(&addr_street) {
                        matched_index = Some(idx);
                        is_local = true;
                        break;
                    }
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
}
