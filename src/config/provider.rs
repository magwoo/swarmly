use std::future::Future;
use std::net::SocketAddr;

pub mod docker;

type Value = Vec<(String, Vec<SocketAddr>)>;

pub trait ConfigProvider {
    fn get_last(&self) -> impl Future<Output = Option<Value>>;

    fn update(&self)
    -> impl Future<Output = anyhow::Result<Vec<(String, Vec<SocketAddr>)>>> + Send;
}
