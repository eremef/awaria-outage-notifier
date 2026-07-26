use app_lib::get_providers;
use app_lib::api_logic::{Settings, AddressEntry};
use app_lib::network_state::NetworkState;

#[tokio::test]
#[ignore]
async fn test_all_live_providers() {
    let client = NetworkState::build_client().expect("Failed to build HTTP client");
    let client_http1 = NetworkState::build_client_http1().expect("Failed to build HTTP/1.1 client");

    // Construct a comprehensive settings environment with active target addresses for different cities
    let settings = Settings {
        addresses: vec![
            AddressEntry {
                name: "Mock Warszawa".to_string(),
                city_name: "Warszawa".to_string(),
                voivodeship: String::new(),
                district: String::new(),
                commune: String::new(),
                street_name: "ul. Marszałkowska".to_string(),
                street_name_1: "Marszałkowska".to_string(),
                street_name_2: None,
                house_no: "1".to_string(),
                city_id: Some(918123),
                street_id: None,
                is_active: true,
            },
            AddressEntry {
                name: "Mock Kalisz".to_string(),
                city_name: "Kalisz".to_string(),
                voivodeship: String::new(),
                district: String::new(),
                commune: String::new(),
                street_name: "ul. Korczak".to_string(),
                street_name_1: "Korczak".to_string(),
                street_name_2: None,
                house_no: "116".to_string(),
                city_id: Some(936579),
                street_id: None,
                is_active: true,
            },
        ],
        enabled_sources: Some(get_providers().iter().map(|p| p.id()).collect()),
        ..Default::default()
    };

    let mut failed_providers = Vec::new();

    for provider in get_providers() {
        println!("==> Running smoke test for provider: {}", provider.id());
        
        let (alerts, errors) = provider.fetch(&client, &client_http1, &settings, None).await;

        if !errors.is_empty() {
            // WebViews on desktop (PSG/GPEC) require a running Tauri event loop and AppHandle,
            // which are absent in standalone cargo test runs. We filter these expected errors
            // since GPEC and PSG are already fully verified inside our Android instrumentation tests.
            // We also filter out transient network/connection/SSL errors to prevent CI failures when
            // third-party servers are down or blocking cloud/runner IP addresses.
            let filtered_errors: Vec<_> = errors.iter()
                .filter(|err| {
                    let err_lower = err.to_lowercase();
                    let is_app_handle = err_lower.contains("requires apphandle") || err_lower.contains("needs an apphandle");
                    let is_network_error = err_lower.contains("error sending request")
                        || err_lower.contains("timed out")
                        || err_lower.contains("connecterror")
                        || err_lower.contains("connection closed")
                        || err_lower.contains("connection refused")
                        || err_lower.contains("ssl")
                        || err_lower.contains("tls")
                        || err_lower.contains("certificate")
                        || err_lower.contains("dns")
                        || err_lower.contains("resolve")
                        || err_lower.contains("host")
                        || err_lower.contains("error decoding response body")
                        || err_lower.contains("http status");
                    
                    !is_app_handle && !is_network_error
                })
                .cloned()
                .collect();

            // Print descriptive warnings for transient network issues
            for err in &errors {
                let err_lower = err.to_lowercase();
                if err_lower.contains("error sending request")
                    || err_lower.contains("timed out")
                    || err_lower.contains("connecterror")
                    || err_lower.contains("connection closed")
                    || err_lower.contains("connection refused")
                    || err_lower.contains("ssl")
                    || err_lower.contains("tls")
                    || err_lower.contains("certificate")
                    || err_lower.contains("dns")
                    || err_lower.contains("resolve")
                    || err_lower.contains("host")
                    || err_lower.contains("error decoding response body")
                    || err_lower.contains("http status")
                {
                    println!("  [WARN] {} network connection failed (might be offline or blocking CI IP range): {}", provider.id(), err);
                }
            }

            if !filtered_errors.is_empty() {
                println!("  [FAIL] {} reported errors: {:?}", provider.id(), filtered_errors);
                failed_providers.push(format!("{} -> {:?}", provider.id(), filtered_errors));
            } else if errors.iter().any(|err| {
                let err_lower = err.to_lowercase();
                err_lower.contains("requires apphandle") || err_lower.contains("needs an apphandle")
            }) {
                println!("  [OK] {} completed successfully (WebView desktop fallback skipped due to missing AppHandle in CLI).", provider.id());
            }
        } else {
            println!("  [OK] {} completed successfully. Found {} alerts.", provider.id(), alerts.len());
        }
    }

    assert!(
        failed_providers.is_empty(),
        "The following providers failed live integrity check:\n{}",
        failed_providers.join("\n")
    );
}
