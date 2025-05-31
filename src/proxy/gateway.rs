use pingora::lb::Backend;
use pingora::lb::selection::RoundRobin;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type LoadBalancer = pingora::lb::LoadBalancer<RoundRobin>;

#[derive(Default)]
pub struct Gateway {
    inner: Arc<RwLock<GatewayInner>>,
}

impl Gateway {
    pub async fn update(&self, lb_by_domain: HashMap<String, LoadBalancer>) {
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
