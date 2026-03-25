use anyhow::Context;
use pingora::listeners::TlsAccept;
use pingora::listeners::tls::TlsSettings;
use pingora::protocols::tls::TlsRef;
use pingora::tls::ssl::NameType;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use self::acme::AcmeResolver;
use self::storage::TlsStorage;
use crate::config::provider::ConfigProvider;
use crate::redis::RedisClient;

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
    redis: Option<RedisClient>,
    node_id: String,
}

impl<P: ConfigProvider + Send + Sync + 'static> TlsResolver<P> {
    pub async fn new(
        provider: P,
        service: AcmeChallengeService,
        redis: Option<RedisClient>,
    ) -> anyhow::Result<Option<Self>> {
        let acme_resolver =
            match AcmeResolver::from_env().context("failed to create acme resolver from env")? {
                Some(resolver) => resolver,
                None => return Ok(None),
            };

        let inner = TlsResolverInner::new(provider, service, acme_resolver, redis).await?;

        let inner = Arc::new(Mutex::new(inner));
        let instance = Self { inner };
        instance.connect_config_callback();

        Ok(Some(instance))
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
            .try_lock()
            .expect("mutex must be available during init")
            .provider()
            .set_update_callback(move |value| {
                let inner = inner.clone();

                async move {
                    let domains = value.into_iter().map(|(d, _)| d);
                    let mut inner = inner.lock().await;

                    for domain in domains {
                        let needs_renewal = inner.storage.needs_renewal(&domain).await;
                        match needs_renewal {
                            Ok(false) => continue,
                            Ok(true) => {
                                tracing::info!("issuing/renewing cert for domain: {}", domain);
                                if let Err(err) = inner.issue_and_store_cert(&domain).await {
                                    tracing::error!(
                                        "failed to issue cert for domain({}): {err:?}",
                                        domain
                                    )
                                }
                            }
                            Err(err) => {
                                tracing::error!(
                                    "failed to check renewal for domain({domain}): {err:?}"
                                );
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

    pub async fn new(
        provider: P,
        service: AcmeChallengeService,
        acme_resolver: AcmeResolver,
        redis: Option<RedisClient>,
    ) -> anyhow::Result<Self> {
        let storage = TlsStorage::from_env(redis.clone());

        let node_id = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_owned());

        Ok(Self {
            storage,
            service,
            acme_resolver,
            provider,
            redis,
            node_id,
        })
    }

    pub async fn issue_and_store_cert(&mut self, domain: &str) -> anyhow::Result<()> {
        if let Some(redis) = &self.redis {
            self.issue_with_lock(domain, redis.clone()).await
        } else {
            let cert = self
                .acme_resolver
                .issue_cert(domain, &self.service)
                .await
                .with_context(|| format!("failed to issue cert for {domain}"))?;
            self.storage.set(domain, cert).await
        }
    }

    async fn issue_with_lock(&mut self, domain: &str, redis: RedisClient) -> anyhow::Result<()> {
        const LOCK_TTL_SECS: u64 = 300;
        const POLL_INTERVAL: Duration = Duration::from_secs(5);
        const MAX_POLLS: u32 = 60;

        let lock_key = format!("swarmly:lock:{}", domain);

        let acquired = redis
            .set_nx(&lock_key, self.node_id.as_bytes().to_vec(), LOCK_TTL_SECS)
            .await
            .context("failed to acquire cert issuance lock")?;

        if acquired {
            tracing::info!("node {} acquired cert lock for {}, issuing", self.node_id, domain);

            let result = self.acme_resolver.issue_cert(domain, &self.service).await;

            if let Err(err) = redis.del(&lock_key).await {
                tracing::warn!("failed to release cert lock for {domain}: {err:?}");
            }

            let cert = result.with_context(|| format!("failed to issue cert for {domain}"))?;
            self.storage.set(domain, cert).await
        } else {
            tracing::info!(
                "another node is issuing cert for {}, waiting up to {}s",
                domain, LOCK_TTL_SECS
            );

            for attempt in 1..=MAX_POLLS {
                tokio::time::sleep(POLL_INTERVAL).await;

                match self.storage.is_exists(domain).await {
                    Ok(true) => {
                        tracing::info!("cert for {} available after {} polls", domain, attempt);
                        return Ok(());
                    }
                    Ok(false) => continue,
                    Err(err) => tracing::warn!("error polling for cert {domain}: {err:?}"),
                }
            }

            anyhow::bail!("timeout waiting for cert for domain {domain} from another node")
        }
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
