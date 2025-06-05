use anyhow::Context;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::cert::Certificate;

pub struct TlsStorage {
    cache: HashMap<String, Certificate>,
    data_path: String,
}

impl TlsStorage {
    const DEFAULT_DATA_PATH: &str = "/data";

    pub fn new(data_path: impl Into<String>) -> Self {
        Self {
            data_path: data_path.into(),
            ..Default::default()
        }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let data_path = std::env::var("DATA_PATH").context("missing `DATA_PATH` env var")?;
        let data_path = data_path.trim().trim_end_matches("/").to_owned();

        Ok(Self::new(data_path))
    }

    pub async fn is_exists(&self, domain: &str) -> anyhow::Result<bool> {
        if self.cache.contains_key(domain) {
            return Ok(true);
        }

        let path = self.cert_path(domain);

        let is_exists = tokio::fs::try_exists(&path)
            .await
            .with_context(|| format!("failed to check is cert exists, path: {}", path))?;

        Ok(is_exists)
    }

    pub async fn get(&mut self, domain: &str) -> anyhow::Result<Option<&Certificate>> {
        let path = self.cert_path(domain);

        match self.cache.entry(domain.to_owned()) {
            Entry::Occupied(entry) => Ok(Some(entry.into_mut())),
            Entry::Vacant(entry) => {
                let cert_bytes = match tokio::fs::read(&path).await {
                    Ok(bytes) => bytes,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(err) => anyhow::bail!("failed to read cert file, err: {err:?}"),
                };

                let cert = Certificate::from_bytes(&cert_bytes)
                    .context("failed to parse cert from bytes")?;

                Ok(Some(entry.insert(cert)))
            }
        }
    }

    fn cert_path(&self, domain: &str) -> String {
        format!("{}/{}.cert", self.data_path, domain)
    }
}

impl Default for TlsStorage {
    fn default() -> Self {
        TlsStorage {
            cache: HashMap::new(),
            data_path: Self::DEFAULT_DATA_PATH.to_owned(),
        }
    }
}
