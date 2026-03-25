use std::future::Future;
use std::net::SocketAddr;

pub mod docker;

#[derive(Clone)]
pub struct ServiceConfig {
    pub addrs: Vec<SocketAddr>,
    pub tls: bool,
}

pub type Value = Vec<(String, ServiceConfig)>;

pub trait ConfigProvider {
    fn set_update_callback<F, Fut>(&self, callback: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static;

    fn update(&self) -> impl Future<Output = anyhow::Result<Value>> + Send;
}
