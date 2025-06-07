use acme_lib::DirectoryUrl;
use anyhow::Context;
use pingora::listeners::TlsAccept;
use pingora::listeners::tls::TlsSettings;
use pingora::protocols::tls::TlsRef;
use pingora::tls::ssl::NameType;
use std::sync::Arc;
use tokio::sync::Mutex;

use self::acme::AcmeResolver;
use self::storage::TlsStorage;
use crate::config::provider::ConfigProvider;

pub use self::acme::service::AcmeChallengeService;

mod acme;
mod cert;
mod storage;

static _DEV_CRT: &[u8] = include_bytes!("../docker/dev.crt");
static _DEV_KEY: &[u8] = include_bytes!("../docker/dev.key");

pub struct TlsResolver<P> {
    inner: Arc<Mutex<TlsResolverInner<P>>>,
}

struct TlsResolverInner<P> {
    storage: TlsStorage,
    acme_resolver: AcmeResolver,
    service: AcmeChallengeService,
    provider: P,
}

impl<P: ConfigProvider + Send + Sync + 'static> TlsResolver<P> {
    pub fn new(
        provider: P,
        service: AcmeChallengeService,
        contact: impl Into<String>,
        url: DirectoryUrl<'static>,
    ) -> anyhow::Result<Self> {
        let inner = TlsResolverInner::new(provider, service, contact, url)?;
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
                    let mut inner = inner.lock().await;

                    for domain in domains {
                        let is_exists = inner.storage().is_exists(&domain).await;
                        match is_exists {
                            Ok(true) => continue,
                            Ok(false) => {
                                if let Err(err) = inner.issue_and_store_cert(&domain).await {
                                    eprintln!(
                                        "failed to issue cert for domain({}): {err:?}",
                                        domain
                                    )
                                }
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

    pub async fn issue_and_store_cert(&mut self, domain: &str) -> anyhow::Result<()> {
        let order = self
            .acme_resolver
            .issue_cert(domain)
            .with_context(|| format!("failed to issue domain({})", domain))?;

        let service = self.service.clone();

        let cert = order
            .challenge_blocked(|c| {
                service.add_challenge(domain, c);
            })
            .with_context(|| format!("failed to challenge domain({})", domain))?;

        self.storage
            .set(domain, cert)
            .await
            .with_context(|| format!("failed to save domain({})", domain))?;

        Ok(())
    }

    pub fn new(
        provider: P,
        service: AcmeChallengeService,
        contact: impl Into<String>,
        url: DirectoryUrl<'static>,
    ) -> anyhow::Result<Self> {
        let storage = TlsStorage::from_env().context("failed to create tls storage")?;
        let acme_resolver =
            AcmeResolver::new(contact, url).context("failed to create acme resolver")?;

        Ok(Self {
            storage,
            service,
            acme_resolver,
            provider,
        })
    }
}

#[async_trait::async_trait]
impl<P: ConfigProvider + Send + Sync> TlsAccept for TlsResolver<P> {
    async fn certificate_callback(&self, ssl: &mut TlsRef) -> () {
        if let Some(domain) = ssl.servername(NameType::HOST_NAME) {
            let mut inner = self.inner.lock().await;

            let cert = match inner.storage.get(domain).await {
                Ok(Some(cert)) => cert,
                Ok(None) => return,
                Err(err) => {
                    tracing::error!("failed to get cert from storage: {err:?}");
                    return;
                }
            };

            let key = cert.private_key();
            let crt = cert.certificate();

            ssl.set_certificate(crt).unwrap();
            ssl.set_private_key(key).unwrap();
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
