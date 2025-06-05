use std::future::Future;
use std::net::SocketAddr;

pub mod docker;

type Value = Vec<(String, Vec<SocketAddr>)>;

pub trait ConfigProvider {
    fn set_update_callback<F, Fut>(&self, callback: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static;

    fn update(&self)
    -> impl Future<Output = anyhow::Result<Vec<(String, Vec<SocketAddr>)>>> + Send;
}
