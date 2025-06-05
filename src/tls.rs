use acme_lib::DirectoryUrl;
use anyhow::Context;
use pingora::listeners::TlsAccept;
use pingora::listeners::tls::TlsSettings;
use pingora::protocols::tls::TlsRef;
use pingora::tls::pkey::PKey;
use pingora::tls::ssl::NameType;
use pingora::tls::x509::X509;

use self::acme::AcmeResolver;
use self::storage::TlsStorage;
use crate::config::provider::ConfigProvider;

mod acme;
mod cert;
mod storage;

static DEV_CRT: &[u8] = include_bytes!("../docker/dev.crt");
static DEV_KEY: &[u8] = include_bytes!("../docker/dev.key");

pub struct TlsResolver<P> {
    storage: TlsStorage,
    acme_resolver: AcmeResolver,
    provider: P,
}

impl<P: ConfigProvider + Send + Sync + 'static> TlsResolver<P> {
    pub fn new(
        provider: P,
        contact: impl Into<String>,
        url: DirectoryUrl<'static>,
    ) -> anyhow::Result<Self> {
        let storage = TlsStorage::from_env().context("failed to create tls storage")?;
        let acme_resolver =
            AcmeResolver::new(contact, url).context("failed to create acme resolver")?;

        Ok(Self {
            storage,
            acme_resolver,
            provider,
        })
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
