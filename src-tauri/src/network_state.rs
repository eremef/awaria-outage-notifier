use reqwest::{Client, Error};
use tokio::sync::OnceCell;
use std::sync::Arc;
use rustls_platform_verifier::BuilderVerifierExt;

pub struct NetworkState {
    client_cell: OnceCell<Client>,
    client_http1_cell: OnceCell<Client>,
}

impl NetworkState {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            client_cell: OnceCell::new(),
            client_http1_cell: OnceCell::new(),
        })
    }

    pub async fn get_client(&self) -> Result<Client, String> {
        self.client_cell
            .get_or_try_init(|| async {
                Self::build_client().map_err(|e| e.to_string())
            })
            .await
            .cloned()
    }

    pub async fn get_client_http1(&self) -> Result<Client, String> {
        self.client_http1_cell
            .get_or_try_init(|| async {
                Self::build_client_http1().map_err(|e| e.to_string())
            })
            .await
            .cloned()
    }

    pub fn build_client() -> Result<Client, Error> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_platform_verifier()
            .unwrap()
            .with_no_client_auth();

        Client::builder()
            .use_preconfigured_tls(client_config)
            .timeout(std::time::Duration::from_secs(30))
            .build()
    }

    pub fn build_client_http1() -> Result<Client, Error> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_platform_verifier()
            .unwrap()
            .with_no_client_auth();

        Client::builder()
            .use_preconfigured_tls(client_config)
            .http1_only()
            .timeout(std::time::Duration::from_secs(30))
            .build()
    }

    /// Build a client using the OS native TLS stack (SChannel on Windows, SecureTransport on macOS).
    /// Use this for sites that fail with rustls (e.g. non-standard cert chains).
    /// On Android, falls back to the http1 rustls client (Android has its own cert store).
    pub fn build_client_native_tls() -> Result<Client, Error> {
        #[cfg(not(target_os = "android"))]
        {
            Client::builder()
                .use_native_tls()
                .timeout(std::time::Duration::from_secs(30))
                .build()
        }
        #[cfg(target_os = "android")]
        {
            Self::build_client_http1()
        }
    }
}
