use app_lib::get_providers;
use app_lib::api_logic::{Settings, AddressEntry};
use app_lib::network_state::NetworkState;

#[tokio::test]
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
            let filtered_errors: Vec<_> = errors.iter()
                .filter(|err| !err.contains("requires AppHandle") && !err.contains("needs an AppHandle"))
                .cloned()
                .collect();

            if !filtered_errors.is_empty() {
                println!("  [FAIL] {} reported errors: {:?}", provider.id(), filtered_errors);
                failed_providers.push(format!("{} -> {:?}", provider.id(), filtered_errors));
            } else {
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
