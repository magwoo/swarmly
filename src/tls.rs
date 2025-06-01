use pingora::listeners::TlsAccept;
use pingora::listeners::tls::TlsSettings;
use pingora::protocols::tls::TlsRef;
use pingora::tls::pkey::PKey;
use pingora::tls::ssl::NameType;
use pingora::tls::x509::X509;

use self::storage::TlsStorage;
use crate::config::provider::ConfigProvider;

mod storage;

static DEV_CRT: &[u8] = include_bytes!("../docker/dev.crt");
static DEV_KEY: &[u8] = include_bytes!("../docker/dev.key");

pub struct TlsResolver<P> {
    provider: P,
    storage: TlsStorage,
}

impl<P: ConfigProvider + Send + Sync + 'static> TlsResolver<P> {
    pub fn new(provider: P) -> Self {
        let storage = TlsStorage::default();

        Self { provider, storage }
    }

    pub fn into_tls_settings(self) -> TlsSettings {
        let callback = Box::new(self);
        let mut settings =
            TlsSettings::with_callbacks(callback).expect("failed to create tls settings");

        settings.enable_h2();

        settings
    }
}

#[async_trait::async_trait]
impl<P: ConfigProvider + Send + Sync> TlsAccept for TlsResolver<P> {
    async fn certificate_callback(&self, ssl: &mut TlsRef) -> () {
        println!(
            "called tls resolver, servername: {:?}",
            ssl.servername(NameType::HOST_NAME)
        );

        if ssl.servername(NameType::HOST_NAME).is_some() {
            let crt = X509::from_pem(DEV_CRT).unwrap();
            ssl.set_certificate(&crt).unwrap();

            let key = PKey::private_key_from_pem(DEV_KEY).unwrap();
            ssl.set_private_key(&key).unwrap();
        }
    }
}
