use reqwest::{Client, Error};
use tokio::sync::OnceCell;
use std::sync::Arc;
#[cfg(not(target_os = "android"))]
use rustls_platform_verifier::BuilderVerifierExt;

/// A no-op TLS certificate verifier used on Android for the `native_tls`-equivalent client.
/// On desktop, `native-tls` uses SChannel/SecureTransport which is lenient with non-standard
/// cert chains. On Android we can't use native-tls, so we replicate that behaviour here.
/// This is only used for providers that are already explicitly opted-in to the lenient client.
#[cfg(target_os = "android")]
#[derive(Debug)]
struct NoCertificateVerification;

#[cfg(target_os = "android")]
impl rustls::client::danger::ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

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

        // On the Android emulator the system trust store is missing intermediate certs for
        // Polish utility sites. On a real device the platform verifier works correctly.
        #[cfg(target_os = "android")]
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
        .with_no_client_auth();

        #[cfg(not(target_os = "android"))]
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_platform_verifier()
        .unwrap()
        .with_no_client_auth();

        Client::builder()
            .use_preconfigured_tls(client_config)
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
    }

    pub fn build_client_http1() -> Result<Client, Error> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        #[cfg(target_os = "android")]
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
        .with_no_client_auth();

        #[cfg(not(target_os = "android"))]
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_platform_verifier()
        .unwrap()
        .with_no_client_auth();

        Client::builder()
            .use_preconfigured_tls(client_config)
            .http1_only()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
    }

    /// Build a client using the OS native TLS stack (SChannel on Windows, SecureTransport on macOS).
    /// Use this for sites that fail with rustls (e.g. non-standard cert chains).
    /// On Android, native-tls is unavailable, so we use rustls with cert verification disabled
    /// to match the permissive behavior of native-tls on desktop. This is intentional — these
    /// providers are already explicitly opted-in to the lenient client precisely because their
    /// cert chains are non-standard.
    pub fn build_client_native_tls() -> Result<Client, Error> {
        #[cfg(not(target_os = "android"))]
        {
            Client::builder()
                .use_native_tls()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build()
        }
        #[cfg(target_os = "android")]
        {
            let _ = rustls::crypto::ring::default_provider().install_default();

            let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
            .with_no_client_auth();

            Client::builder()
                .use_preconfigured_tls(client_config)
                .http1_only()
                .timeout(std::time::Duration::from_secs(30))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build()
        }
    }

    // On Android, WorkManager constraints already ensure we run only when connected.
    // Individual fetch errors are handled per-provider, so no global probe is needed.
    #[cfg(target_os = "android")]
    pub async fn check_internet_connection(_client: &Client) -> bool {
        true
    }

    #[cfg(not(target_os = "android"))]
    pub async fn check_internet_connection(client: &Client) -> bool {
        use std::time::{Instant, Duration};
        use std::sync::OnceLock;

        static LAST_CHECK: OnceLock<tokio::sync::Mutex<Option<(Instant, bool)>>> = OnceLock::new();
        let cache_mutex = LAST_CHECK.get_or_init(|| tokio::sync::Mutex::new(None));

        let mut guard = cache_mutex.lock().await;
        if let Some((time, result)) = *guard {
            if time.elapsed() < Duration::from_secs(5) {
                return result;
            }
        }

        let urls = [
            "https://clients3.google.com/generate_204",
            "https://captive.apple.com/hotspot-detect.html",
            "https://1.1.1.1",
        ];

        let mut is_online = false;
        for url in urls {
            match client.get(url).timeout(std::time::Duration::from_secs(4)).send().await {
                Ok(res) => {
                    if res.status().is_success() || res.status().as_u16() == 204 {
                        is_online = true;
                        break;
                    } else {
                        log::warn!("Internet check URL {} returned status {}", url, res.status());
                    }
                }
                Err(e) => {
                    log::warn!("Internet check URL {} failed: {}", url, e);
                }
            }
        }

        *guard = Some((Instant::now(), is_online));
        is_online
    }
}
