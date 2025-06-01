use pingora::lb::selection::RoundRobin;
use pingora::lb::{Backend, Backends};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::discovery::PingDiscovery;

type LoadBalancer = pingora::lb::LoadBalancer<RoundRobin>;

#[derive(Default, Clone)]
pub struct Gateway {
    inner: Arc<RwLock<GatewayInner>>,
}

impl Gateway {
    pub async fn update(&self, upstreams: Vec<(String, Vec<SocketAddr>)>) {
        let mut lb_by_domain = HashMap::new();

        upstreams.into_iter().for_each(|(domain, addrs)| {
            let discovery = PingDiscovery::new(addrs);
            let backends = Backends::new(Box::new(discovery));
            let lb = LoadBalancer::from_backends(backends);

            lb_by_domain.insert(domain, lb);
        });

        for lb in lb_by_domain.values() {
            let _ = lb.update().await;
        }

        let mut inner = self.inner.write().await;
        inner.update(lb_by_domain);
    }

    pub async fn process(&self, domain: &str) -> Option<Backend> {
        let inner = self.inner.read().await;
        inner.process(domain)
    }
}

#[derive(Default)]
struct GatewayInner {
    lb_by_domain: HashMap<String, LoadBalancer>,
}

impl GatewayInner {
    pub fn update(&mut self, lb_by_domain: HashMap<String, LoadBalancer>) {
        self.lb_by_domain = lb_by_domain;
    }

    pub fn process(&self, domain: &str) -> Option<Backend> {
        let lb = self.lb_by_domain.get(domain)?;

        lb.select(b"", 64)
    }
}
