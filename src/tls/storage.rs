use anyhow::Context;
use std::collections::HashMap;

use super::cert::Certificate;
use crate::redis::RedisClient;

enum Backend {
    Filesystem(String),
    Redis(RedisClient),
}

pub struct TlsStorage {
    cache: HashMap<String, Certificate>,
    backend: Backend,
}

impl TlsStorage {
    const DEFAULT_DATA_DIR: &str = "/opt/swarmly/certs";
    const CERT_KEY_PREFIX: &str = "swarmly:cert:";
    const CERT_TTL_SECS: u64 = 80 * 24 * 3600;

    pub fn from_env(redis: Option<RedisClient>) -> Self {
        match redis {
            Some(client) => Self {
                cache: HashMap::new(),
                backend: Backend::Redis(client),
            },
            None => {
                let dir = std::env::var("DATA_DIR")
                    .unwrap_or_else(|_| Self::DEFAULT_DATA_DIR.to_owned());
                let dir = dir.trim().trim_end_matches('/').to_owned();
                Self {
                    cache: HashMap::new(),
                    backend: Backend::Filesystem(dir),
                }
            }
        }
    }

    pub async fn set(&mut self, domain: &str, cert: Certificate) -> anyhow::Result<()> {
        let bytes = cert.to_bytes();

        match &self.backend {
            Backend::Filesystem(dir) => {
                let path = cert_path(dir, domain);
                tokio::fs::create_dir_all(dir)
                    .await
                    .context("failed to create certs directory")?;
                tokio::fs::write(&path, &bytes)
                    .await
                    .context("failed to save cert to file")?;
            }
            Backend::Redis(client) => {
                let key = format!("{}{}", Self::CERT_KEY_PREFIX, domain);
                client
                    .set(&key, bytes, Self::CERT_TTL_SECS)
                    .await
                    .context("failed to save cert to redis")?;
            }
        }

        self.cache.insert(domain.to_owned(), cert);

        Ok(())
    }

    pub async fn needs_renewal(&mut self, domain: &str) -> anyhow::Result<bool> {
        match self.fetch_from_backend(domain).await? {
            Some(cert) => Ok(cert.is_expiring()),
            None => Ok(true),
        }
    }

    pub async fn get(&mut self, domain: &str) -> anyhow::Result<Option<&Certificate>> {
        if !self.cache.contains_key(domain) {
            self.fetch_from_backend(domain).await?;
        }
        Ok(self.cache.get(domain))
    }

    pub async fn fetch_from_backend(&mut self, domain: &str) -> anyhow::Result<Option<&Certificate>> {
        let bytes = match &self.backend {
            Backend::Filesystem(dir) => {
                let path = cert_path(dir, domain);
                match tokio::fs::read(&path).await {
                    Ok(b) => b,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(e) => anyhow::bail!("failed to read cert file: {e:?}"),
                }
            }
            Backend::Redis(client) => {
                let key = format!("{}{}", Self::CERT_KEY_PREFIX, domain);
                match client.get(&key).await? {
                    Some(b) => b,
                    None => return Ok(None),
                }
            }
        };

        let cert = Certificate::from_bytes(&bytes).context("failed to parse certificate")?;
        self.cache.insert(domain.to_owned(), cert);

        Ok(self.cache.get(domain))
    }
}

fn cert_path(dir: &str, domain: &str) -> String {
    format!("{}/{}.cert", dir, domain)
}
