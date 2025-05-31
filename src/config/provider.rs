use std::collections::HashMap;
use std::net::SocketAddr;

pub mod docker;

pub trait ConfigProvider {
    async fn update(&self) -> anyhow::Result<HashMap<String, Vec<SocketAddr>>>;
}
