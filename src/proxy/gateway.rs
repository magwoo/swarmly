use pingora::lb::selection::RoundRobin;
use pingora::lb::{Backend, Backends};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::discovery::PingDiscovery;
use crate::config::provider::Value;

type LoadBalancer = pingora::lb::LoadBalancer<RoundRobin>;

#[derive(Default, Clone)]
pub struct Gateway {
    inner: Arc<RwLock<GatewayInner>>,
}

impl Gateway {
    pub async fn update(&self, upstreams: Value) {
        let mut entries = HashMap::new();

        for (domain, config) in upstreams {
            let discovery = PingDiscovery::new(config.addrs);
            let backends = Backends::new(Box::new(discovery));
            let lb = LoadBalancer::from_backends(backends);

            entries.insert(domain, (lb, config.tls));
        }

        for (domain, (lb, _)) in entries.iter() {
            if let Err(err) = lb.update().await {
                tracing::warn!("failed to update backends for {domain}: {err:?}");
            }
        }

        let mut inner = self.inner.write().await;
        inner.entries = entries;
    }

    pub async fn process(&self, domain: &str) -> Option<(Backend, bool)> {
        let inner = self.inner.read().await;
        inner.process(domain)
    }
}

#[derive(Default)]
struct GatewayInner {
    entries: HashMap<String, (LoadBalancer, bool)>,
}

impl GatewayInner {
    pub fn process(&self, domain: &str) -> Option<(Backend, bool)> {
        let (lb, tls) = self.entries.get(domain)?;
        let backend = lb.select(b"", 64)?;
        Some((backend, *tls))
    }
}
