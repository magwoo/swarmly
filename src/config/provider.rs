use std::future::Future;
use std::net::SocketAddr;

pub mod docker;

type Value = Vec<(String, Vec<SocketAddr>)>;

pub trait ConfigProvider {
    async fn update_callback(&self, callback: impl Fn(&Value) + Send + Sync + 'static);

    fn update(&self)
    -> impl Future<Output = anyhow::Result<Vec<(String, Vec<SocketAddr>)>>> + Send;
}
