use acme_lib::DirectoryUrl;
use anyhow::Context;
use pingora::listeners::TlsAccept;
use pingora::listeners::tls::TlsSettings;
use pingora::protocols::tls::TlsRef;
use pingora::tls::pkey::PKey;
use pingora::tls::ssl::NameType;
use pingora::tls::x509::X509;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

use self::acme::AcmeResolver;
use self::storage::TlsStorage;
use crate::config::provider::ConfigProvider;

mod acme;
mod cert;
mod storage;

static DEV_CRT: &[u8] = include_bytes!("../docker/dev.crt");
static DEV_KEY: &[u8] = include_bytes!("../docker/dev.key");

pub struct TlsResolver<P> {
    inner: Arc<Mutex<TlsResolverInner<P>>>,
}

struct TlsResolverInner<P> {
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
        let inner = TlsResolverInner::new(provider, contact, url)?;
        let inner = Arc::new(Mutex::new(inner));

        let instance = Self { inner };

        instance.connect_config_callback();

        Ok(instance)
    }

    pub fn as_tls_settings(&self) -> TlsSettings {
        let callback = Box::new(self.clone());
        let mut settings =
            TlsSettings::with_callbacks(callback).expect("failed to create tls settings");

        settings.enable_h2();

        settings
    }

    fn connect_config_callback(&self) {
        let inner = self.inner.clone();

        self.inner
            .blocking_lock()
            .provider()
            .set_update_callback(move |value| {
                let inner = inner.clone();

                async move {
                    let domains = value.into_iter().map(|(d, _)| d);
                    let inner = inner.lock().await;

                    let storage = inner.storage();
                    let acme_resolver = inner.acme_resolver();

                    for domain in domains {
                        match storage.is_exists(&domain).await {
                            Ok(true) => continue,
                            Ok(false) => {
                                // TODO: make worked cert issue and http challenge pass
                                acme_resolver.issue_cert(&domain);
                            }
                            Err(err) => {
                                eprintln!("failed to check is domain({domain}) exists: {err:?}");
                                continue;
                            }
                        }
                    }
                }
            });
    }
}

impl<P: ConfigProvider + Send + Sync + 'static> TlsResolverInner<P> {
    pub fn provider(&self) -> &P {
        &self.provider
    }

    pub fn storage(&self) -> &TlsStorage {
        &self.storage
    }

    pub fn acme_resolver(&self) -> &AcmeResolver {
        &self.acme_resolver
    }

    pub fn new(
        provider: P,
        contact: impl Into<String>,
        url: DirectoryUrl<'static>,
    ) -> anyhow::Result<Self> {
        let storage = TlsStorage::from_env().context("failed to create tls storage")?;
        let acme_resolver =
            AcmeResolver::new(contact, url).context("failed to create acme resolver")?;

        provider.set_update_callback(Self::config_update_callback);

        Ok(Self {
            storage,
            acme_resolver,
            provider,
        })
    }

    async fn config_update_callback(config: Vec<(String, Vec<SocketAddr>)>) {
        let domains = config.into_iter().map(|(d, _)| d).collect::<Vec<_>>();

        // for domain in domains {
        //     self.
        // }
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

impl<P: ConfigProvider> Clone for TlsResolver<P> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
